import { randomUUID } from 'node:crypto';
import type {
  ProviderCall,
  ProviderEventBatch,
  ProviderReply,
  ValidatedProviderRecipe,
} from '@42ch/nexus-contracts';
import type { ProviderError, ProviderHostEvent } from './contracts.js';
import { MAX_BATCH_EVENTS, OperationDelivery } from './delivery.js';
import {
  closeOwnedConnection,
  createAcpSession,
  reapChild,
  spawnOwnedConnection,
  waitForChildExit,
  type OwnedConnection,
} from './process-owner.js';

type SessionState = {
  sessionId: string;
  providerId: string;
  recipeGeneration: string;
  cwd: string;
  owned: OwnedConnection;
};

type ActiveOperation = {
  sessionId: string;
  opId: string;
  delivery: OperationDelivery;
  terminalEnqueued: boolean;
};

function okReply(requestId: string, extra: Partial<ProviderReply> = {}): ProviderReply {
  return {
    request_id: requestId,
    ok: true,
    ...extra,
  };
}

function errReply(requestId: string, error: ProviderError): ProviderReply {
  return {
    request_id: requestId,
    ok: false,
    error,
  };
}

function sanitizeError(message: string): ProviderError {
  return {
    code: 'internal',
    message: message.replace(/\/[^\s]+/g, '[path]').slice(0, 240),
    details: {},
    http_status: 500,
  };
}

function parseRecipe(payload: Record<string, unknown>): ValidatedProviderRecipe {
  const recipe = (payload.recipe ?? payload) as ValidatedProviderRecipe;
  if (!recipe?.executable || !recipe.cwd || !recipe.recipe_generation || !recipe.provider_id) {
    throw new Error('invalid_recipe');
  }
  return recipe;
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
      const message = error instanceof Error ? error.message : 'provider_failed';
      return errReply(request.request_id, sanitizeError(message));
    }
  }

  async next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch> {
    const op = this.operations.get(operationId);
    if (!op) {
      return { operation_id: operationId, events: [], has_more: false };
    }
    if (!op.delivery.tryBeginPull()) {
      throw new Error('pull already in flight');
    }
    try {
      const batch = op.delivery.pull(maxEvents, maxBytes);
      if (!batch.has_more && op.delivery.isTerminal && !op.terminalEnqueued) {
        op.terminalEnqueued = true;
        const terminalEvents = op.delivery.drainTerminalEvents(op.sessionId, op.opId);
        const merged = [...batch.events, ...terminalEvents];
        const capEvents = Math.min(maxEvents, MAX_BATCH_EVENTS);
        return {
          operation_id: operationId,
          events: merged.slice(0, capEvents),
          has_more: merged.length > capEvents,
          gap: batch.gap,
        };
      }
      return batch;
    } finally {
      op.delivery.endPull();
    }
  }

  private routeSessionUpdate(acpSessionId: string, update: unknown): void {
    for (const session of this.sessions.values()) {
      if (session.owned.acpSessionId !== acpSessionId) continue;
      for (const op of this.operations.values()) {
        if (op.sessionId !== session.sessionId) continue;
        const mapped = mapSessionUpdate(session.sessionId, op.opId, update);
        if (mapped) op.delivery.enqueue(mapped);
      }
    }
  }

  private async handleProbe(request: ProviderCall): Promise<ProviderReply> {
    const recipe = parseRecipe(request.payload);
    const owned = await spawnOwnedConnection(recipe, () => undefined);
    await closeOwnedConnection(owned);
    const reaped = await waitForChildExit(owned.child, 4_000);
    if (!reaped) {
      return errReply(request.request_id, {
        code: 'interrupted',
        message: 'probe_child_not_reaped',
        details: {},
        http_status: 503,
      });
    }
    const start = Date.now();
    return okReply(request.request_id, {
      health: {
        provider_id: recipe.provider_id,
        available: true,
        latency_ms: Date.now() - start,
        message: null,
      },
    });
  }

  private async handleLaunch(request: ProviderCall): Promise<ProviderReply> {
    const recipe = parseRecipe(request.payload);
    const cwd = String(request.payload.cwd ?? recipe.cwd);
    await this.evictStaleGeneration(recipe.recipe_generation);
    const sessionId = request.session_id ?? randomUUID();
    const owned = await this.connectForRecipe(recipe, sessionId);
    await createAcpSession(owned, cwd);
    this.sessions.set(sessionId, {
      sessionId,
      providerId: recipe.provider_id,
      recipeGeneration: recipe.recipe_generation,
      cwd,
      owned,
    });
    return okReply(request.request_id, {
      session_id: sessionId,
    });
  }

  private async handleExecute(request: ProviderCall): Promise<ProviderReply> {
    const sessionId = request.session_id;
    if (!sessionId) throw new Error('execute_requires_session');
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error('session_not_found');
    const payload = request.payload as { kind?: string; content?: string };
    if (payload.kind !== 'prompt') throw new Error('unsupported_operation');
    const opId = randomUUID();
    const delivery = new OperationDelivery(opId);
    const active: ActiveOperation = { sessionId, opId, delivery, terminalEnqueued: false };
    this.operations.set(opId, active);
    delivery.enqueue({ OpStarted: { session_id: sessionId, op_id: opId } });

    void this.runPrompt(session, opId, payload.content ?? '', delivery);
    return okReply(request.request_id, { operation_id: opId, session_id: sessionId });
  }

  private async runPrompt(
    session: SessionState,
    opId: string,
    content: string,
    delivery: OperationDelivery,
  ): Promise<void> {
    const acpSessionId = session.owned.acpSessionId;
    if (!acpSessionId) {
      delivery.setTerminal({ kind: 'failed', category: 'provider_error', message: 'missing_acp_session' });
      return;
    }
    try {
      const response = await session.owned.connection.prompt({
        sessionId: acpSessionId,
        prompt: [{ type: 'text', text: content }],
      });
      const reason = String(response.stopReason ?? 'end_turn') as
        | 'end_turn'
        | 'cancelled'
        | 'max_tokens'
        | 'max_turn_requests'
        | 'refusal';
      delivery.setTerminal({ kind: 'finished', reason });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'prompt_failed';
      if (message.includes('frame_too_large')) {
        delivery.setTerminal({ kind: 'failed', category: 'provider_error', message: 'delivery_overflow' });
      } else if (message.toLowerCase().includes('eof') || message.includes('closed')) {
        delivery.setTerminal({ kind: 'failed', category: 'provider_error', message: 'provider_eof' });
      } else {
        delivery.setTerminal({ kind: 'failed', category: 'provider_error', message: 'prompt_failed' });
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
    await session.owned.connection.cancel({ sessionId: session.owned.acpSessionId });
    op.delivery.setTerminal({ kind: 'finished', reason: 'cancelled' });
    return okReply(request.request_id, { operation_id: operationId, session_id: op.sessionId });
  }

  private async handleShutdown(request: ProviderCall): Promise<ProviderReply> {
    const sessionId = request.session_id;
    if (!sessionId) throw new Error('shutdown_requires_session');
    const session = this.sessions.get(sessionId);
    if (!session) throw new Error('session_not_found');
    await closeOwnedConnection(session.owned);
    await reapChild(session.owned.child);
    this.sessions.delete(sessionId);
    this.recipeOwners.delete(`${session.recipeGeneration}:${sessionId}`);
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

  private async evictStaleGeneration(currentGeneration: string): Promise<void> {
    for (const [key, owned] of this.recipeOwners.entries()) {
      if (key.startsWith(`${currentGeneration}:`)) continue;
      await closeOwnedConnection(owned);
      await reapChild(owned.child);
      this.recipeOwners.delete(key);
    }
    for (const [sessionId, session] of this.sessions.entries()) {
      if (session.recipeGeneration !== currentGeneration) {
        await closeOwnedConnection(session.owned);
        await reapChild(session.owned.child);
        this.sessions.delete(sessionId);
      }
    }
  }
}

export function createEngine(): {
  call(request: ProviderCall): Promise<ProviderReply>;
  next(operationId: string, maxEvents: number, maxBytes: number): Promise<ProviderEventBatch>;
} {
  const engine = new AcpProviderEngine();
  return {
    call: (request) => engine.call(request),
    next: (operationId, maxEvents, maxBytes) => engine.next(operationId, maxEvents, maxBytes),
  };
}
