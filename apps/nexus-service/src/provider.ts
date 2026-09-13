import { randomUUID } from 'node:crypto';
import type {
  CancelOperationResponse,
  CreateSessionRequest,
  ExecuteOperationRequest,
  OperationResponse,
  ProviderCall,
  ProviderHostEvent,
  ProviderReply,
  SessionResponse,
  ShutdownSessionResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { PROVIDER_DEFAULT_DEADLINE_MS } from './config.js';
import { HttpError, mapNativeError, routeNotMigrated } from './errors.js';
import {
  ProviderRegistry,
  type ProviderOperationRecord,
  type ProviderSessionRecord,
} from './provider-registry.js';
import { OperationEventHub } from './sse.js';
import { hostQuery } from './world-kb.js';

export { ProviderRegistry } from './provider-registry.js';
export type { ProviderOperationRecord, ProviderSessionRecord } from './provider-registry.js';

const DSH_PROVIDER_ID = 'dsh-native';

function isTerminalHostEvent(event: ProviderHostEvent): boolean {
  return 'OpFinished' in event || 'OpFailed' in event || 'SessionStopped' in event;
}

function transcriptFromTerminal(event: ProviderHostEvent): string | null {
  if ('OpFinished' in event) return JSON.stringify(event.OpFinished);
  if ('OpFailed' in event) return JSON.stringify(event.OpFailed);
  return null;
}

function parseUuid(value: string, field: string): void {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value)) {
    throw new HttpError(400, 'invalid_input', `${field} must be a valid UUID`, { field });
  }
}

function rejectActorViewpoint(body: CreateSessionRequest, routeLabel: string): void {
  const hasActor = body.actor_ref !== undefined && body.actor_ref !== null;
  const hasViewpoint = body.viewpoint !== undefined && body.viewpoint !== null;
  if (hasActor !== hasViewpoint) {
    throw new HttpError(400, 'invalid_input', 'actor_ref and viewpoint must both be present or both absent');
  }
  if (hasActor) throw routeNotMigrated(routeLabel);
}

function ensureProviderReply(reply: ProviderReply): ProviderReply {
  if (!reply.ok) {
    if (reply.error) throw mapNativeError(reply.error);
    throw new HttpError(503, 'interrupted', 'provider call failed without error detail');
  }
  return reply;
}

async function providerCall(service: ServiceCore, request: ProviderCall): Promise<ProviderReply> {
  try {
    const reply = await service.core.providerCall(request);
    return ensureProviderReply(reply);
  } catch (error) {
    throw mapNativeError(error);
  }
}

export async function createProviderSession(service: ServiceCore, body: unknown): Promise<SessionResponse> {
  if (service.domainOnly) throw routeNotMigrated('POST /v1/daemon/agent-host/sessions');
  const req = body as CreateSessionRequest;
  if (!req || typeof req !== 'object' || typeof req.provider_id !== 'string' || req.provider_id.length === 0) {
    throw new HttpError(400, 'invalid_input', 'provider_id is required');
  }
  rejectActorViewpoint(req, 'POST /v1/daemon/agent-host/sessions (actor/viewpoint)');
  const payload: Record<string, unknown> = { provider_id: req.provider_id };
  if (req.cwd) payload.cwd = req.cwd;
  if (req.model) payload.model = req.model;
  if (req.mode) payload.mode = req.mode;
  const reply = await providerCall(service, {
    method: 'launch',
    request_id: randomUUID(),
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload,
  });
  const sessionId = reply.session_id;
  if (!sessionId) throw new HttpError(500, 'internal', 'provider launch returned no session_id');
  service.providerRegistry.registerSession({
    sessionId,
    providerId: req.provider_id,
    state: 'Ready',
    activeOpId: null,
    model: req.model,
  });
  return {
    session_id: sessionId,
    provider_id: req.provider_id,
    state: 'Ready',
    active_op_id: undefined,
    model: req.model,
  };
}

export async function shutdownProviderSession(service: ServiceCore, sessionId: string): Promise<ShutdownSessionResponse> {
  if (service.domainOnly) throw routeNotMigrated(`DELETE /v1/daemon/agent-host/sessions/${sessionId}`);
  parseUuid(sessionId, 'session_id');
  await assertKnownSession(service, sessionId);
  await providerCall(service, {
    method: 'shutdown',
    request_id: randomUUID(),
    session_id: sessionId,
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload: {},
  });
  service.providerRegistry.removeSession(sessionId);
  return { session_id: sessionId, status: 'shutdown' };
}

export async function executeProviderOperation(service: ServiceCore, sessionId: string, body: unknown): Promise<OperationResponse> {
  if (service.domainOnly) throw routeNotMigrated(`POST /v1/daemon/agent-host/sessions/${sessionId}/operations`);
  parseUuid(sessionId, 'session_id');
  const session = await assertKnownSession(service, sessionId);
  const req = body as ExecuteOperationRequest;
  if (!req || typeof req !== 'object' || typeof req.kind !== 'string') {
    throw new HttpError(400, 'invalid_input', 'operation request body is required');
  }
  if (req.kind !== 'prompt') throw routeNotMigrated(`operation kind ${req.kind}`);
  if (typeof req.content !== 'string') throw new HttpError(400, 'invalid_input', 'prompt content is required');
  const reply = await providerCall(service, {
    method: 'execute',
    request_id: randomUUID(),
    session_id: sessionId,
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload: { kind: 'prompt', content: req.content },
  });
  const operationId = reply.operation_id;
  if (!operationId) throw new HttpError(500, 'internal', 'provider execute returned no operation_id');
  service.providerRegistry.registerOperation({
    operationId,
    sessionId,
    providerId: session.providerId,
    status: 'started',
    terminalEvent: null,
    terminalTranscript: null,
  });
  service.providerRegistry.ensureHub(operationId, sessionId, () => new OperationEventHub(operationId, sessionId));
  return { operation_id: operationId, session_id: sessionId, status: 'started' };
}

export async function cancelProviderOperation(service: ServiceCore, operationId: string): Promise<CancelOperationResponse> {
  if (service.domainOnly) throw routeNotMigrated(`POST /v1/daemon/agent-host/operations/${operationId}`);
  parseUuid(operationId, 'operation_id');
  const op = await assertKnownOperation(service, operationId);
  if (op.providerId === DSH_PROVIDER_ID) {
    throw new HttpError(501, 'route_not_migrated', 'DSH provider does not support cancellation');
  }
  await providerCall(service, {
    method: 'cancel',
    request_id: randomUUID(),
    operation_id: operationId,
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload: {},
  });
  service.providerRegistry.clearSessionOperation(op.sessionId);
  return { operation_id: operationId, status: 'cancelled' };
}

export async function lookupProviderSession(service: ServiceCore, sessionId: string): Promise<SessionResponse | null> {
  parseUuid(sessionId, 'session_id');
  try {
    const response = await hostQuery(service, { query: 'get_session', session_id: sessionId });
    if (response.session) return response.session;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code !== 'not_found') throw mapped;
  }
  const cached = service.providerRegistry.sessionRecord(sessionId);
  if (!cached) return null;
  return {
    session_id: cached.sessionId,
    provider_id: cached.providerId,
    state: cached.state,
    active_op_id: cached.activeOpId ?? undefined,
    model: cached.model,
  };
}

export async function lookupProviderOperation(
  service: ServiceCore,
  operationId: string,
): Promise<{ operation_id: string; session_id: string; status: string } | null> {
  parseUuid(operationId, 'operation_id');
  try {
    const response = await hostQuery(service, { query: 'get_operation', operation_id: operationId });
    if (response.operation) {
      return {
        operation_id: response.operation.operation_id,
        session_id: response.operation.session_id,
        status: response.operation.status,
      };
    }
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code !== 'not_found') throw mapped;
  }
  const cached = service.providerRegistry.operationRecord(operationId);
  if (!cached) return null;
  return { operation_id: cached.operationId, session_id: cached.sessionId, status: cached.status };
}

async function assertKnownSession(service: ServiceCore, sessionId: string): Promise<ProviderSessionRecord> {
  const cached = service.providerRegistry.sessionRecord(sessionId);
  if (cached) return cached;
  const lookedUp = await lookupProviderSession(service, sessionId);
  if (!lookedUp) {
    throw new HttpError(404, 'not_found', `session ${sessionId} not found`, { resource: `session:${sessionId}` });
  }
  const record: ProviderSessionRecord = {
    sessionId: lookedUp.session_id,
    providerId: lookedUp.provider_id,
    state: lookedUp.state,
    activeOpId: lookedUp.active_op_id ?? null,
    model: lookedUp.model,
  };
  service.providerRegistry.registerSession(record);
  return record;
}

async function assertKnownOperation(service: ServiceCore, operationId: string): Promise<ProviderOperationRecord> {
  const cached = service.providerRegistry.operationRecord(operationId);
  if (cached) return cached;
  const lookedUp = await lookupProviderOperation(service, operationId);
  if (!lookedUp) {
    throw new HttpError(404, 'not_found', `operation ${operationId} not found`, { resource: `operation:${operationId}` });
  }
  const session = await assertKnownSession(service, lookedUp.session_id);
  const record: ProviderOperationRecord = {
    operationId,
    sessionId: lookedUp.session_id,
    providerId: session.providerId,
    status: lookedUp.status,
    terminalEvent: null,
    terminalTranscript: null,
  };
  service.providerRegistry.registerOperation(record);
  return record;
}
