import { randomUUID } from 'node:crypto';
import type { ProviderCallbacks } from './contracts.js';
import type {
  ProviderCall,
  ProviderEventBatch,
  ProviderReply,
  ValidatedProviderRecipe,
} from '@42ch/nexus-contracts';
import type { ProviderError, ProviderHostEvent } from './contracts.js';
import { OperationDelivery } from './delivery.js';
import { CleanupUnconfirmedError, ProviderNextError } from './errors.js';
import {
  cleanupOwnedConnection,
  closeOwnedConnection,
  createAcpSession,
  MAX_PROMPT_INPUT_BYTES,
  spawnOwnedConnection,
  type OwnedConnection,
  type ReapResult,
} from './process-owner.js';
import { parseAdmittedRecipe } from './recipe.js';

const MAX_LIVE_PAYLOAD_OPERATIONS = 64;

type SessionState = {
  sessionId: string;
  providerId: string;
  recipeGeneration: string;
  cwd: string;
  owned: OwnedConnection;
  activeOperationId: string | null;
};

type ActiveOperation = {
  sessionId: string;
  opId: string;
  delivery: OperationDelivery | null;
  promptTask: Promise<void> | null;
  cancelled: boolean;
  terminalDelivered: boolean;
  inspectUrl: string;
};

function okReply(requestId: string, extra: Partial<ProviderReply> = {}): ProviderReply {
  return { request_id: requestId, ok: true, ...extra };
}

function errReply(requestId: string, error: ProviderError): ProviderReply {
  return { request_id: requestId, ok: false, error };
}

function interruptedReply(requestId: string, message: string): ProviderReply {
  return errReply(requestId, {
    code: 'interrupted',
    message,
    details: { cleanup_unconfirmed: true },
    http_status: 503,
  });
}

function inspectUrlFor(opId: string): string {
  return `nexus://host/operation/${opId}/status`;
}

function sanitizeError(message: string): ProviderError {
  return {
    code: 'internal',
    message: message.replace(/\/[^\s]+/g, '[path]').slice(0, 240),
    details: {},
    http_status: 500,
  };
}

function mapSessionUpdate(
  sessionId: string,
  opId: string,
  update: unknown,
): ProviderHostEvent | null {
  const record = update as { sessionUpdate?: string; content?: { type?: string; text?: string } };
  if (record.sessionUpdate === 'agent_message_chunk' && record.content?.type === 'text') {
    const text = record.content.text ?? '';
    return { MessageDelta: { session_id: sessionId, op_id: opId, text } };
  }
  return null;
}

export class AcpProviderEngine {
  private sessions = new Map<string, SessionState>();
  private operations = new Map<string, ActiveOperation>();
  private recipeOwners = new Map<string, OwnedConnection>();
  private cleanupFences = new Map<string, OwnedConnection>();
  private livePayloadOrder: string[] = [];

  async call(request: ProviderCall): Promise<ProviderReply> {
    try {
      switch (request.method) {
        case 'probe':
          return await this.handleProbe(request);
        case 'launch':
          return await this.handleLaunch(request);
        case 'execute':
          return await this.handleExecute(request);
        case 'cancel':
          return await this.handleCancel(request);
        case 'shutdown':
          return await this.handleShutdown(request);
        default:
          return errReply(request.request_id, {
            code: 'invalid_input',
            message: 'unknown_method',
            details: {},
            http_status: 400,
          });
      }
    } catch (error) {
      if (error instanceof CleanupUnconfirmedError) {
        return interruptedReply(request.request_id, error.message);
      }
      const message = error instanceof Error ? error.message : 'provider_failed';
      if (message === 'invalid_recipe') {
        return errReply(request.request_id, {
          code: 'invalid_input',
          message: 'invalid_recipe',
          details: {},
          http_status: 400,
        });
      }
      if (message === 'cleanup_fence_active' || message === 'generation_blocked_by_cleanup_fence') {
        return interruptedReply(request.request_id, message);
      }
      if (message === 'process_identity_mismatch') {
        return interruptedReply(request.request_id, 'process_identity_mismatch');
      }
      if (message === 'process_identity_unsupported') {
        return interruptedReply(request.request_id, 'process_identity_unsupported');
      }
      if (message === 'session_busy') {
        return errReply(request.request_id, {
          code: 'busy',
          message: 'session_busy',
          details: {},
          http_status: 503,
        });
      }
      return errReply(request.request_id, sanitizeError(message));
    }
  }

  async next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch> {
    const op = this.operations.get(operationId);
    if (!op) throw new ProviderNextError(operationId);
    if (op.terminalDelivered) {
      return { operation_id: operationId, events: [], has_more: false };
    }
    if (!op.delivery) {
      return {
        operation_id: operationId,
        events: [],
        has_more: false,
        gap: {
          reason: 'history_unavailable',
          operation_id: operationId,
          resync_required: true,
          inspect_url: op.inspectUrl,
        },
      };
    }
    if (!op.delivery.tryBeginPull()) throw new Error('pull already in flight');
    try {
      const batch = op.delivery.pull(maxEvents, maxBytes);
      if (batch.events.length === 0 && op.delivery.hasPendingTerminal) {
        const terminalEvents = op.delivery.consumeTerminal(op.sessionId, op.opId);
        if (terminalEvents) {
          this.ackTerminalDelivery(operationId, op);
          return {
            operation_id: operationId,
            events: terminalEvents,
            has_more: false,
            gap: batch.gap,
          };
        }
      }
      if (batch.events.length > 0 && op.delivery.hasPendingTerminal) {
        return { ...batch, has_more: true };
      }
      if (!batch.has_more && !op.delivery.hasPendingTerminal && op.promptTask) {
        return { ...batch, has_more: true };
      }
      return batch;
    } finally {
      op.delivery?.endPull();
    }
  }

  private assertNoCleanupFence(): void {
    if (this.cleanupFences.size > 0) throw new Error('cleanup_fence_active');
  }

  private ownerIdentityKey(owned: OwnedConnection): string {
    const id = owned.boundIdentity;
    return `${id.pid}:${id.process_birth ?? ''}:${id.group_id ?? ''}`;
  }

  private retainCleanupFence(key: string, owned: OwnedConnection): void {
    const identity = this.ownerIdentityKey(owned);
    for (const existing of this.cleanupFences.values()) {
      if (this.ownerIdentityKey(existing) === identity) return;
    }
    this.cleanupFences.set(key, owned);
  }

  private clearFencesForOwner(owned: OwnedConnection): void {
    const identity = this.ownerIdentityKey(owned);
    for (const [key, existing] of [...this.cleanupFences.entries()]) {
      if (this.ownerIdentityKey(existing) === identity) {
        this.cleanupFences.delete(key);
      }
    }
  }

  private async requireConfirmedCleanup(
    owned: OwnedConnection,
    fenceKey: string,
    failureMessage: string,
  ): Promise<ReapResult> {
    const reap = await cleanupOwnedConnection(owned);
    if (!reap.confirmed) {
      this.retainCleanupFence(fenceKey, owned);
      throw new CleanupUnconfirmedError(failureMessage);
    }
    this.cleanupFences.delete(fenceKey);
    return reap;
  }

  private trackLivePayload(opId: string): void {
    if (!this.livePayloadOrder.includes(opId)) this.livePayloadOrder.push(opId);
  }

  private pruneLivePayloads(): void {
    while (this.livePayloadOrder.length > MAX_LIVE_PAYLOAD_OPERATIONS) {
      let evictIdx = -1;
      for (let i = 0; i < this.livePayloadOrder.length; i += 1) {
        const candidate = this.livePayloadOrder[i];
        const candidateOp = this.operations.get(candidate);
        if (candidateOp?.terminalDelivered) {
          evictIdx = i;
          break;
        }
      }
      if (evictIdx < 0) break;
      const evictId = this.livePayloadOrder.splice(evictIdx, 1)[0]!;
      const op = this.operations.get(evictId);
      if (op) {
        if (op.delivery) {
          op.delivery.close();
          op.delivery = null;
        }
      }
    }
  }

  private ackTerminalDelivery(opId: string, op: ActiveOperation): void {
    if (op.terminalDelivered) return;
    op.terminalDelivered = true;
    if (op.delivery) {
      op.delivery.close();
      op.delivery = null;
    }
    this.pruneLivePayloads();
  }

  private routeSessionUpdate(acpSessionId: string, update: unknown): void {
    for (const session of this.sessions.values()) {
      if (session.owned.acpSessionId !== acpSessionId) continue;
      const opId = session.activeOperationId;
      if (!opId) continue;
      const op = this.operations.get(opId);
      if (!op?.delivery) continue;
      const mapped = mapSessionUpdate(session.sessionId, opId, update);
      if (mapped) op.delivery.enqueue(mapped);
    }
  }

  private async handleProbe(request: ProviderCall): Promise<ProviderReply> {
    this.assertNoCleanupFence();
    const started = Date.now();
    const recipe = parseAdmittedRecipe(request.payload);
    let owned: OwnedConnection | null = null;
    try {
      owned = await spawnOwnedConnection(recipe, () => undefined);
    } catch (error) {
      if (error instanceof CleanupUnconfirmedError) {
        return this.fenceCleanupError(
          error,
          `pre-probe:${recipe.provider_id}`,
          request.request_id,
        );
      }
      const message = error instanceof Error ? error.message : 'probe_failed';
      if (message === 'provider_eof' || message === 'stderr_overflow') {
        return {
          request_id: request.request_id,
          ok: true,
          health: {
            provider_id: recipe.provider_id,
            available: false,
            latency_ms: Date.now() - started,
            message,
          },
        };
      }
      throw error;
    }
    try {
      await this.requireConfirmedCleanup(owned, `probe:${recipe.recipe_generation}`, 'probe_child_not_reaped');
    } catch (error) {
      if (error instanceof CleanupUnconfirmedError) {
        return this.fenceCleanupError(
          error,
          `probe:${recipe.recipe_generation}`,
          request.request_id,
        );
      }
      throw error;
    }
    return okReply(request.request_id, {
      health: {
        provider_id: recipe.provider_id,
        available: true,
        latency_ms: Date.now() - started,
        message: null,
      },
    });
  }

  private async handleLaunch(request: ProviderCall): Promise<ProviderReply> {
    this.assertNoCleanupFence();
    const recipe = parseAdmittedRecipe(request.payload);
    await this.evictStaleGeneration(recipe.recipe_generation);
    const sessionId = request.session_id ?? randomUUID();
    const ownerKey = `${recipe.recipe_generation}:${sessionId}`;
    let owned: OwnedConnection | null = null;
    try {
      owned = await this.connectForRecipe(recipe, sessionId);
      await createAcpSession(owned, recipe.cwd);
      this.sessions.set(sessionId, {
        sessionId,
        providerId: recipe.provider_id,
        recipeGeneration: recipe.recipe_generation,
        cwd: recipe.cwd,
        owned,
        activeOperationId: null,
      });
      return okReply(request.request_id, { session_id: sessionId });
    } catch (error) {
      return await this.finalizeLaunchFailure(
        error,
        owned,
        ownerKey,
        request.request_id,
      );
    }
  }

  /** @internal test hook for launch cleanup retry / fence semantics. */
  async finalizeLaunchFailure(
    error: unknown,
    owned: OwnedConnection | null,
    ownerKey: string,
    requestId: string,
  ): Promise<ProviderReply> {
    const cleanupTarget =
      owned ??
      (error instanceof CleanupUnconfirmedError ? error.owner : null);
    if (cleanupTarget) {
      try {
        await this.requireConfirmedCleanup(cleanupTarget, ownerKey, 'launch_child_not_reaped');
        this.recipeOwners.delete(ownerKey);
        this.clearFencesForOwner(cleanupTarget);
      } catch (cleanupError) {
        if (cleanupError instanceof CleanupUnconfirmedError) {
          return this.fenceCleanupError(cleanupError, ownerKey, requestId);
        }
        throw cleanupError;
      }
    }
    if (error instanceof CleanupUnconfirmedError) {
      if (error.cause !== undefined) throw error.cause;
      return this.fenceCleanupError(error, ownerKey, requestId);
    }
    throw error;
  }

  private async handleExecute(request: ProviderCall): Promise<ProviderReply> {
    this.assertNoCleanupFence();
    const sessionId = request.session_id;
    if (!sessionId) throw new Error('execute_requires_session');
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error('session_not_found');
    if (session.activeOperationId) throw new Error('session_busy');
    const payload = request.payload as { kind?: string; content?: string };
    if (payload.kind !== 'prompt') throw new Error('unsupported_operation');
    const content = payload.content ?? '';
    if (new TextEncoder().encode(content).length > MAX_PROMPT_INPUT_BYTES) {
      return errReply(request.request_id, {
        code: 'invalid_input',
        message: 'prompt_too_large',
        details: {},
        http_status: 400,
      });
    }
    const opId = randomUUID();
    const delivery = new OperationDelivery(opId);
    const active: ActiveOperation = {
      sessionId,
      opId,
      delivery,
      promptTask: null,
      cancelled: false,
      terminalDelivered: false,
      inspectUrl: inspectUrlFor(opId),
    };
    this.operations.set(opId, active);
    this.trackLivePayload(opId);
    session.activeOperationId = opId;
    delivery.enqueue({ OpStarted: { session_id: sessionId, op_id: opId } });
    const task = this.runPrompt(session, active, content);
    active.promptTask = task;
    void task.finally(() => {
      if (session.activeOperationId === opId) session.activeOperationId = null;
    });
    return okReply(request.request_id, { operation_id: opId, session_id: sessionId });
  }

  private async runPrompt(
    session: SessionState,
    op: ActiveOperation,
    content: string,
  ): Promise<void> {
    const acpSessionId = session.owned.acpSessionId;
    if (!acpSessionId) {
      op.delivery?.trySetTerminal({
        kind: 'failed',
        category: 'provider_error',
        message: 'missing_acp_session',
      });
      return;
    }
    try {
      const response = await session.owned.connection.prompt({
        sessionId: acpSessionId,
        prompt: [{ type: 'text', text: content }],
      });
      if (op.cancelled) return;
      const reason = String(response.stopReason ?? 'end_turn') as
        | 'end_turn'
        | 'cancelled'
        | 'max_tokens'
        | 'max_turn_requests'
        | 'refusal';
      op.delivery?.trySetTerminal({ kind: 'finished', reason });
    } catch (error) {
      if (op.cancelled) return;
      const message = error instanceof Error ? error.message : 'prompt_failed';
      if (message.includes('frame_too_large')) {
        op.delivery?.trySetTerminal({
          kind: 'failed',
          category: 'provider_error',
          message: 'delivery_overflow',
        });
      } else if (
        message === 'provider_eof' ||
        message.toLowerCase().includes('eof') ||
        message.includes('closed')
      ) {
        op.delivery?.trySetTerminal({
          kind: 'failed',
          category: 'provider_error',
          message: 'provider_eof',
        });
      } else {
        op.delivery?.trySetTerminal({
          kind: 'failed',
          category: 'provider_error',
          message: 'prompt_failed',
        });
      }
    }
  }

  private async handleCancel(request: ProviderCall): Promise<ProviderReply> {
    const operationId = request.operation_id;
    if (!operationId) throw new Error('cancel_requires_operation');
    const op = this.operations.get(operationId);
    if (!op) throw new Error('operation_not_found');
    const session = this.sessions.get(op.sessionId);
    if (!session?.owned.acpSessionId) throw new Error('session_not_found');
    op.cancelled = true;
    if (!op.delivery?.trySetTerminal({ kind: 'finished', reason: 'cancelled' })) {
      return okReply(request.request_id, { operation_id: operationId, session_id: op.sessionId });
    }
    try {
      await session.owned.connection.cancel({ sessionId: session.owned.acpSessionId });
    } catch {
      // best effort
    }
    if (op.promptTask) await op.promptTask.catch(() => undefined);
    if (session.activeOperationId === operationId) session.activeOperationId = null;
    const ownerKey = `${session.recipeGeneration}:${session.sessionId}`;
    try {
      await this.requireConfirmedCleanup(session.owned, ownerKey, 'cancel_child_not_reaped');
    } catch (error) {
      if (error instanceof CleanupUnconfirmedError) {
        return interruptedReply(request.request_id, error.message);
      }
      throw error;
    }
    this.sessions.delete(op.sessionId);
    this.recipeOwners.delete(ownerKey);
    return okReply(request.request_id, { operation_id: operationId, session_id: op.sessionId });
  }

  private async handleShutdown(request: ProviderCall): Promise<ProviderReply> {
    const sessionId = request.session_id;
    if (!sessionId) throw new Error('shutdown_requires_session');
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error('session_not_found');
    // A session may be shutting down while a prompt is still in flight (a
    // native close reaps sessions that are mid-operation). Cooperate with the
    // owned child first: without the ACP cancel notification a blocked prompt
    // never settles, so awaiting the prompt task would hang forever instead of
    // reaching the owned process reap.
    if (session.activeOperationId) {
      const active = this.operations.get(session.activeOperationId);
      if (active) {
        active.cancelled = true;
        active.delivery?.trySetTerminal({ kind: 'finished', reason: 'cancelled' });
        const acpSessionId = session.owned.acpSessionId;
        if (acpSessionId) {
          try {
            await session.owned.connection.cancel({ sessionId: acpSessionId });
          } catch {
            // best effort; the owned reap below is what must be confirmed
          }
        }
        if (active.promptTask) await active.promptTask.catch(() => undefined);
      }
      session.activeOperationId = null;
    }
    const ownerKey = `${session.recipeGeneration}:${sessionId}`;
    try {
      await this.requireConfirmedCleanup(session.owned, ownerKey, 'shutdown_child_not_reaped');
    } catch (error) {
      if (error instanceof CleanupUnconfirmedError) {
        return interruptedReply(request.request_id, error.message);
      }
      throw error;
    }
    this.sessions.delete(sessionId);
    this.recipeOwners.delete(ownerKey);
    return okReply(request.request_id, { session_id: sessionId });
  }

  private async connectForRecipe(
    recipe: ValidatedProviderRecipe,
    sessionId: string,
  ): Promise<OwnedConnection> {
    const owned = await spawnOwnedConnection(recipe, (params) => {
      this.routeSessionUpdate(String(params.sessionId), params.update);
    });
    this.recipeOwners.set(`${recipe.recipe_generation}:${sessionId}`, owned);
    return owned;
  }

  private terminalizeSessionOperations(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    for (const op of this.operations.values()) {
      if (op.sessionId !== sessionId) continue;
      op.cancelled = true;
      op.delivery?.trySetTerminal({
        kind: 'failed',
        category: 'provider_error',
        message: 'generation_replaced',
      });
    }
    if (session) session.activeOperationId = null;
  }

  seedTerminalTombstone(opId: string, sessionId = 'sess'): void {
    this.operations.set(opId, {
      sessionId,
      opId,
      delivery: null,
      promptTask: null,
      cancelled: false,
      terminalDelivered: true,
      inspectUrl: inspectUrlFor(opId),
    });
  }

  operationCount(): number {
    return this.operations.size;
  }


  private fenceCleanupError(
    error: CleanupUnconfirmedError,
    fenceKey: string,
    requestId: string,
  ): ProviderReply {
    if (error.owner) {
      this.retainCleanupFence(fenceKey, error.owner);
    }
    return interruptedReply(requestId, error.message);
  }

  /** @internal test hook: attempt confirmed settlement of all cleanup fences. */
  async trySettleCleanupFences(): Promise<{ settled: string[]; pending: string[] }> {
    const settled: string[] = [];
    const pending: string[] = [];
    for (const [key, owned] of [...this.cleanupFences.entries()]) {
      const reap = await cleanupOwnedConnection(owned);
      if (reap.confirmed) {
        this.cleanupFences.delete(key);
        settled.push(key);
      } else {
        pending.push(key);
      }
    }
    return { settled, pending };
  }

  cleanupFenceCount(): number {
    return this.cleanupFences.size;
  }

  /** @internal tests: adopt a pre-engine owner into the engine fence. */
  adoptCleanupOwner(key: string, owner: OwnedConnection): void {
    this.retainCleanupFence(key, owner);
  }

  private async evictStaleGeneration(currentGeneration: string): Promise<void> {
    if (this.cleanupFences.size > 0) throw new Error('generation_blocked_by_cleanup_fence');
    for (const [key, owned] of [...this.recipeOwners.entries()]) {
      if (key.startsWith(`${currentGeneration}:`)) continue;
      await this.requireConfirmedCleanup(owned, key, `evict_cleanup_unconfirmed:${key}`);
      this.recipeOwners.delete(key);
    }
    for (const [sessionId, session] of [...this.sessions.entries()]) {
      if (session.recipeGeneration === currentGeneration) continue;
      this.terminalizeSessionOperations(sessionId);
      const ownerKey = `${session.recipeGeneration}:${sessionId}`;
      await this.requireConfirmedCleanup(session.owned, ownerKey, `evict_session_cleanup_unconfirmed:${ownerKey}`);
      this.sessions.delete(sessionId);
      this.recipeOwners.delete(ownerKey);
    }
  }
}

export function createEngine(): ProviderCallbacks {
  const engine = new AcpProviderEngine();
  return {
    call: (request) => engine.call(request),
    next: (operationId, maxEvents, maxBytes) => engine.next(operationId, maxEvents, maxBytes),
  };
}

export function createTestEngine(): AcpProviderEngine {
  return new AcpProviderEngine();
}
