import { randomUUID } from 'node:crypto';
import type { ContentBlock, SessionConfigOption } from '@agentclientprotocol/sdk';
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
  configOptions: SessionConfigOption[];
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

class OperationError extends Error {
  constructor(readonly code: 'invalid_input' | 'not_supported' | 'interrupted', message: string) {
    super(message);
  }
}

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new OperationError('invalid_input', 'expected_object');
  }
  return value as Record<string, unknown>;
}

function string(value: unknown): string {
  if (typeof value !== 'string' || value.length === 0) {
    throw new OperationError('invalid_input', 'expected_nonempty_string');
  }
  return value;
}

/** Decode the existing Rust HostOperation representation, not a second DTO. */
function parseOperation(payload: Record<string, unknown>, controlId: string) {
  if (Object.keys(payload).length !== 1) {
    throw new OperationError('invalid_input', 'expected_one_host_operation');
  }
  if ('SetModel' in payload) {
    return { kind: 'model' as const, opId: controlId, value: string(object(payload.SetModel).model) };
  }
  if ('SetMode' in payload) {
    return { kind: 'mode' as const, opId: controlId, value: string(object(payload.SetMode).mode) };
  }
  if (!('Prompt' in payload)) {
    throw new OperationError('not_supported', 'unsupported_operation');
  }
  const prompt = object(payload.Prompt);
  const opId = string(prompt.op_id);
  if (prompt.permission_scope != null) {
    const scope = object(prompt.permission_scope);
    for (const key of ['allow_read', 'allow_write', 'allow_destructive']) {
      if (typeof scope[key] !== 'boolean') {
        throw new OperationError('invalid_input', 'invalid_permission_scope');
      }
    }
    // The callback's existing host permission policy denies every request.
    // Intersection with any valid narrowing scope remains deny-all; never
    // turn a scope's true bits into an approval or discard a false bit.
  }
  if (!Array.isArray(prompt.content) || prompt.content.length === 0) {
    throw new OperationError('invalid_input', 'empty_prompt');
  }
  const content: ContentBlock[] = prompt.content.map((value: unknown): ContentBlock => {
    const block = object(value);
    if (Object.keys(block).length !== 1) {
      throw new OperationError('invalid_input', 'invalid_content_block');
    }
    if ('Text' in block) {
      const text = object(block.Text).text;
      if (typeof text !== 'string') throw new OperationError('invalid_input', 'invalid_text');
      return { type: 'text', text };
    }
    if ('ResourceLink' in block) {
      const link = object(block.ResourceLink);
      const uri = string(link.uri);
      return { type: 'resource_link', uri, name: link.name == null ? uri : string(link.name) };
    }
    throw new OperationError('not_supported', 'unsupported_content_block');
  });
  if (new TextEncoder().encode(JSON.stringify(content)).length > MAX_PROMPT_INPUT_BYTES) {
    throw new OperationError('invalid_input', 'prompt_too_large');
  }
  return { kind: 'prompt' as const, opId, content };
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

/** Default ceiling when a caller supplies no deadline. */
const DEFAULT_DEADLINE_MS = 5_000;

/**
 * The ONE absolute deadline for a cancel/shutdown handler, derived from the
 * caller's `deadline_ms`.
 *
 * Every phase of the handler — protocol cancel, prompt settlement, forced
 * teardown, TERM/KILL reap — draws from this single instant. No phase may grant
 * itself a fresh default, or the handler would outlive the deadline it was given.
 */
function handlerDeadlineMs(deadlineMs: number | null | undefined): number {
  const budget = typeof deadlineMs === 'number' && deadlineMs > 0 ? deadlineMs : DEFAULT_DEADLINE_MS;
  return Date.now() + budget;
}

/**
 * Budget left before `deadline`.
 *
 * Never negative and never a fresh default: a spent deadline yields `0`, which
 * the reap reads as "cannot confirm" and fences rather than signalling blind.
 */
function remainingBudgetMs(deadline: number): number {
  return Math.max(0, deadline - Date.now());
}

/**
 * Bound `work` by the remaining budget.
 *
 * The caller's `deadline_ms` is authoritative: whatever happens, a cancel or
 * shutdown must reach the owned-process reap inside it rather than awaiting a
 * prompt that never settles. Returns `false` when the work did not finish in
 * time — the caller then proceeds to cleanup rather than hanging.
 */
async function boundedByDeadline(
  budgetMs: number,
  work: Promise<unknown>,
): Promise<boolean> {
  let timer: unknown;
  try {
    const timeout = new Promise<'timeout'>((resolve) => {
      timer = setTimeout(() => resolve('timeout'), budgetMs);
    });
    const settled = await Promise.race([work.then(() => 'done' as const), timeout]);
    return settled === 'done';
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

export class AcpProviderEngine {
  private sessions = new Map<string, SessionState>();
  private operations = new Map<string, ActiveOperation>();
  private recipeOwners = new Map<string, OwnedConnection>();
  private cleanupFences = new Map<string, OwnedConnection>();
  private livePayloadOrder: string[] = [];
  private launchingProviders = new Set<string>();
  private launchingSessions = new Set<string>();

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
      if (error instanceof OperationError) {
        return errReply(request.request_id, {
          code: error.code, message: error.message, details: {},
          http_status: error.code === 'interrupted' ? 503 : 400,
        });
      }
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

  private hasCleanupFenceForOwner(owned: OwnedConnection): boolean {
    const identity = this.ownerIdentityKey(owned);
    for (const existing of this.cleanupFences.values()) {
      if (this.ownerIdentityKey(existing) === identity) return true;
    }
    return false;
  }

  private retainCleanupFence(key: string, owned: OwnedConnection): void {
    if (this.hasCleanupFenceForOwner(owned)) return;
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

  /**
   * Confirm the owned child is gone, spending only `budgetMs`.
   *
   * `budgetMs` is the caller's REMAINING handler budget — passing `undefined`
   * means "this is a standalone settlement attempt" (a later retry or an eviction),
   * which is the only case allowed to use the default reap budget.
   */
  private async requireConfirmedCleanup(
    owned: OwnedConnection,
    fenceKey: string,
    failureMessage: string,
    budgetMs?: number,
  ): Promise<ReapResult> {
    const reap = await cleanupOwnedConnection(owned, budgetMs);
    if (!reap.confirmed) {
      this.retainCleanupFence(fenceKey, owned);
      throw new CleanupUnconfirmedError(failureMessage);
    }
    // The child is gone, so every fence for this owner is settled — leaving a
    // stale one behind would block a later reopen for no reason.
    this.clearFencesForOwner(owned);
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

  private routeSessionUpdate(sessionId: string, acpSessionId: string, update: unknown): void {
    // ACP IDs are connection-local; different providers may return identical
    // IDs. The connection closure, not a global ACP-ID scan, owns the update.
    const session = this.sessions.get(sessionId);
    if (!session || session.owned.acpSessionId !== acpSessionId) return;
    const opId = session.activeOperationId;
    if (!opId) return;
    const op = this.operations.get(opId);
    if (!op?.delivery) return;
    const mapped = mapSessionUpdate(sessionId, opId, update);
    if (mapped) op.delivery.enqueue(mapped);
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
    if (this.launchingProviders.has(recipe.provider_id)) throw new Error('session_busy');
    const sessionId = request.session_id ?? randomUUID();
    if (this.sessions.has(sessionId) || this.launchingSessions.has(sessionId)) {
      throw new OperationError('invalid_input', 'session_id_already_owned');
    }
    this.launchingProviders.add(recipe.provider_id);
    this.launchingSessions.add(sessionId);
    const ownerKey = `${recipe.recipe_generation}:${sessionId}`;
    let owned: OwnedConnection | null = null;
    try {
      await this.evictStaleGeneration(recipe.provider_id, recipe.recipe_generation);
      owned = await this.connectForRecipe(recipe, sessionId);
      const created = await owned.connection.newSession({ cwd: recipe.cwd, mcpServers: [] });
      owned.acpSessionId = created.sessionId;
      const session: SessionState = {
        sessionId,
        providerId: recipe.provider_id,
        recipeGeneration: recipe.recipe_generation,
        cwd: recipe.cwd,
        owned,
        activeOperationId: null,
        configOptions: created.configOptions ?? [],
      };
      // LaunchSpec's retained model/mode settings use the same discovered
      // control paths as subsequent operations, on this exact ACP session.
      if (request.payload.model != null) await this.setModel(session, string(request.payload.model));
      if (request.payload.mode != null) await this.setMode(session, string(request.payload.mode));
      this.sessions.set(sessionId, session);
      return okReply(request.request_id, { session_id: sessionId });
    } catch (error) {
      return await this.finalizeLaunchFailure(
        error,
        owned,
        ownerKey,
        request.request_id,
      );
    } finally {
      this.launchingProviders.delete(recipe.provider_id);
      this.launchingSessions.delete(sessionId);
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
    const operation = parseOperation(request.payload, request.operation_id ?? randomUUID());
    const opId = operation.opId;
    if (this.operations.has(opId)) throw new OperationError('invalid_input', 'operation_id_already_owned');
    if (operation.kind === 'model' && !session.configOptions.some((option) => option.category === 'model')) {
      throw new OperationError('not_supported', 'no_model_config_option');
    }
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
    const task = operation.kind === 'prompt'
      ? this.runPrompt(session, active, operation.content)
      : this.runControl(session, active, operation);
    active.promptTask = task;
    const release = () => {
      if (session.activeOperationId === opId) session.activeOperationId = null;
    };
    void task.then(release, release);
    if (operation.kind !== 'prompt') {
      // A control RPC rejection is not a successful operation. Keep its
      // owned task visible to shutdown while awaiting the real acknowledgement.
      await task;
    }
    return okReply(request.request_id, { operation_id: opId, session_id: sessionId });
  }

  private async setMode(session: SessionState, mode: string): Promise<void> {
    try {
      await session.owned.connection.setSessionMode({
        sessionId: string(session.owned.acpSessionId), modeId: mode,
      });
    } catch {
      throw new OperationError('not_supported', 'set_mode_failed');
    }
  }

  private async setModel(session: SessionState, model: string): Promise<void> {
    const option = session.configOptions.find((config) => config.category === 'model');
    if (!option) throw new OperationError('not_supported', 'no_model_config_option');
    try {
      const response = await session.owned.connection.setSessionConfigOption({
        sessionId: string(session.owned.acpSessionId), configId: option.id, value: model,
      });
      session.configOptions = response.configOptions;
    } catch {
      throw new OperationError('not_supported', 'set_model_failed');
    }
  }

  private async runControl(
    session: SessionState,
    op: ActiveOperation,
    operation: { kind: 'model' | 'mode'; value: string },
  ): Promise<void> {
    try {
      if (operation.kind === 'model') await this.setModel(session, operation.value);
      else await this.setMode(session, operation.value);
      if (op.cancelled) throw new OperationError('interrupted', 'control_cancelled');
      op.delivery?.trySetTerminal({ kind: 'finished', reason: 'end_turn' });
    } catch (error) {
      if (!op.cancelled) {
        op.delivery?.trySetTerminal({
          kind: 'failed', category: 'capability_unsupported', message: `set_${operation.kind}_failed`,
        });
      }
      throw error;
    }
  }

  private async runPrompt(
    session: SessionState,
    op: ActiveOperation,
    content: ContentBlock[],
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
        prompt: content,
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
    // ONE absolute deadline for this entire handler: the protocol cancel, the
    // prompt join, and the TERM/KILL reap all draw from it. No phase may take a
    // fresh default, or the handler would outlive the deadline it was given.
    const deadline = handlerDeadlineMs(request.deadline_ms);
    op.cancelled = true;
    // A retained terminal does not prove the previous reap succeeded. Retrying
    // cancel must settle this owner's fence before acknowledging cleanup.
    if (!op.delivery?.trySetTerminal({ kind: 'finished', reason: 'cancelled' }) &&
        !this.hasCleanupFenceForOwner(session.owned)) {
      return okReply(request.request_id, { operation_id: operationId, session_id: op.sessionId });
    }
    // Narrowing does not survive the closure boundary, so capture it here.
    const acpSessionId = session.owned.acpSessionId;
    await boundedByDeadline(
      remainingBudgetMs(deadline),
      (async () => {
        try {
          await session.owned.connection.cancel({ sessionId: acpSessionId });
        } catch {
          // best effort; the reap below is what must be confirmed
        }
        if (op.promptTask) await op.promptTask.catch(() => undefined);
      })(),
    );
    if (session.activeOperationId === operationId) session.activeOperationId = null;
    const ownerKey = `${session.recipeGeneration}:${session.sessionId}`;
    try {
      // Whatever the cancel phase left is the ONLY budget the reap may spend.
      await this.requireConfirmedCleanup(
        session.owned,
        ownerKey,
        'cancel_child_not_reaped',
        remainingBudgetMs(deadline),
      );
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
    // ONE absolute deadline for the whole handler (see `handleCancel`).
    const deadline = handlerDeadlineMs(request.deadline_ms);
    if (session.activeOperationId) {
      const active = this.operations.get(session.activeOperationId);
      if (active) {
        active.cancelled = true;
        active.delivery?.trySetTerminal({ kind: 'finished', reason: 'cancelled' });
        const acpSessionId = session.owned.acpSessionId;
        await boundedByDeadline(
          remainingBudgetMs(deadline),
          (async () => {
            if (acpSessionId) {
              try {
                await session.owned.connection.cancel({ sessionId: acpSessionId });
              } catch {
                // best effort; the owned reap below is what must be confirmed
              }
            }
            if (active.promptTask) await active.promptTask.catch(() => undefined);
          })(),
        );
      }
      session.activeOperationId = null;
    }
    const ownerKey = `${session.recipeGeneration}:${sessionId}`;
    try {
      await this.requireConfirmedCleanup(
        session.owned,
        ownerKey,
        'shutdown_child_not_reaped',
        remainingBudgetMs(deadline),
      );
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
      this.routeSessionUpdate(sessionId, String(params.sessionId), params.update);
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

  /** @internal tests: adopt a session with an owned connection. */
  adoptSession(
    sessionId: string,
    owned: OwnedConnection,
    generation = 'gen-test',
  ): void {
    this.sessions.set(sessionId, {
      sessionId,
      providerId: 'mock-acp',
      recipeGeneration: generation,
      // The ambient process shim has no `cwd`; the test hook only needs a
      // placeholder since no launch path reads it here.
      cwd: '',
      owned,
      activeOperationId: null,
      configOptions: [],
    });
    this.recipeOwners.set(`${generation}:${sessionId}`, owned);
  }

  /** @internal tests: register an in-flight operation for a session. */
  adoptOperation(
    sessionId: string,
    opId: string,
    promptTask: Promise<void> | null,
    delivery: OperationDelivery | null = null,
  ): void {
    this.operations.set(opId, {
      sessionId,
      opId,
      delivery,
      promptTask,
      cancelled: false,
      terminalDelivered: false,
      inspectUrl: inspectUrlFor(opId),
    });
    const session = this.sessions.get(sessionId);
    if (session) session.activeOperationId = opId;
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

  private async evictStaleGeneration(providerId: string, currentGeneration: string): Promise<void> {
    if (this.cleanupFences.size > 0) throw new Error('generation_blocked_by_cleanup_fence');
    for (const [sessionId, session] of [...this.sessions.entries()]) {
      if (session.providerId !== providerId || session.recipeGeneration === currentGeneration) continue;
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
