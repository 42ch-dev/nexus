import { randomUUID } from 'node:crypto';
import type {
  CancelOperationResponse,
  CreateSessionRequest,
  OperationResponse,
  ProviderCall,
  ProviderReply,
  SessionResponse,
  ShutdownSessionResponse,
} from '@42ch/nexus-contracts';
import type { ServiceCore } from './lifecycle.js';
import { MAX_ACTIVE_PROVIDER_OPERATIONS, PROVIDER_DEFAULT_DEADLINE_MS } from './config.js';
import { HttpError, mapNativeError, routeNotMigrated } from './errors.js';
import {
  isTerminalOperationStatus,
  type ProviderOperationRecord,
  type ProviderSessionRecord,
} from './provider-registry.js';
import { OperationEventHub } from './sse.js';
import { hostQuery } from './world-kb.js';

export { ProviderRegistry } from './provider-registry.js';
export type { ProviderOperationRecord, ProviderSessionRecord } from './provider-registry.js';

const DSH_PROVIDER_ID = 'dsh-native';

/** Reject any key not permitted by the generated request schema. */
function rejectUnknownKeys(value: Record<string, unknown>, allowed: readonly string[], label: string): void {
  for (const key of Object.keys(value)) {
    if (!allowed.includes(key)) {
      throw new HttpError(400, 'invalid_input', `${label} has unknown field: ${key}`, { field: key });
    }
  }
}

function requirePlainObject(value: unknown, label: string): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new HttpError(400, 'invalid_input', `${label} must be a JSON object`);
  }
  return value as Record<string, unknown>;
}

function optionalString(value: unknown, field: string): string | undefined {
  // Generated optional scalars are `type: string`: only an absent (undefined)
  // property is legal. `null` is a type violation, not absence.
  if (value === undefined) return undefined;
  if (typeof value !== 'string') {
    throw new HttpError(400, 'invalid_input', `${field} must be a string`, { field });
  }
  return value;
}

function hasOwn(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function expectString(value: unknown, field: string): string {
  if (typeof value !== 'string') {
    throw new HttpError(400, 'invalid_input', `${field} must be a string`, { field });
  }
  return value;
}

function expectPattern(value: string, pattern: RegExp, field: string): void {
  if (!pattern.test(value)) {
    throw new HttpError(400, 'invalid_input', `${field} has an invalid format`, { field });
  }
}

/**
 * Validate an `actor_ref` against the generated closed sum. A malformed value is
 * `invalid_input`; only a *valid* Actor ref becomes `not_migrated`.
 */
function validateActorRef(value: unknown): boolean {
  const ref = requirePlainObject(value, 'actor_ref');
  const kind = ref.actor_kind;
  if (kind === 'creator') {
    rejectUnknownKeys(ref, ['actor_kind', 'creator_id'], 'actor_ref');
    expectPattern(expectString(ref.creator_id, 'actor_ref.creator_id'), /^ctr_[a-zA-Z0-9]+$/, 'actor_ref.creator_id');
    return true;
  }
  if (kind === 'character') {
    rejectUnknownKeys(ref, ['actor_kind', 'character_id'], 'actor_ref');
    expectPattern(expectString(ref.character_id, 'actor_ref.character_id'), /^chr_[0-9a-f]{32}$/, 'actor_ref.character_id');
    return true;
  }
  throw new HttpError(400, 'invalid_input', 'actor_ref.actor_kind must be creator or character', { field: 'actor_ref.actor_kind' });
}

/** Validate a `viewpoint` shape against the generated schema. */
function validateViewpoint(value: unknown): boolean {
  const vp = requirePlainObject(value, 'viewpoint');
  rejectUnknownKeys(vp, ['world_id', 'binding_id', 'branch_id', 'event_id'], 'viewpoint');
  expectPattern(expectString(vp.world_id, 'viewpoint.world_id'), /^wld_[a-zA-Z0-9]+$/, 'viewpoint.world_id');
  // An own property must be a valid string; only absence is legal, never null.
  if (hasOwn(vp, 'binding_id')) {
    expectPattern(expectString(vp.binding_id, 'viewpoint.binding_id'), /^awb_[0-9a-f]{32}$/, 'viewpoint.binding_id');
  }
  if (hasOwn(vp, 'branch_id')) {
    expectPattern(expectString(vp.branch_id, 'viewpoint.branch_id'), /^fbk_[a-zA-Z0-9]+$/, 'viewpoint.branch_id');
  }
  if (hasOwn(vp, 'event_id')) {
    expectPattern(expectString(vp.event_id, 'viewpoint.event_id'), /^evt_[a-zA-Z0-9]+$/, 'viewpoint.event_id');
  }
  return true;
}

/**
 * Enforce the generated `CreateSessionRequest` shape before any provider effect:
 * plain object only, required/typed optional fields, a valid actor/viewpoint
 * pair, and no unknown keys. Malformed values are `invalid_input` (400); a
 * well-formed but unsupported Actor-mode request is `not_migrated` (501).
 */
function validateCreateSessionRequest(body: unknown): CreateSessionRequest {
  const req = requirePlainObject(body, 'session create body');
  rejectUnknownKeys(req, ['provider_id', 'cwd', 'model', 'mode', 'actor_ref', 'viewpoint'], 'session create body');
  if (typeof req.provider_id !== 'string' || req.provider_id.length === 0) {
    throw new HttpError(400, 'invalid_input', 'provider_id is required', { field: 'provider_id' });
  }
  const cwd = optionalString(req.cwd, 'cwd');
  const model = optionalString(req.model, 'model');
  const mode = optionalString(req.mode, 'mode');
  // The actor/viewpoint pair is detected by own-property presence, so an explicit
  // `null` is treated as present-and-invalid rather than absent.
  const hasActor = hasOwn(req, 'actor_ref');
  const hasViewpoint = hasOwn(req, 'viewpoint');
  if (hasActor !== hasViewpoint) {
    throw new HttpError(400, 'invalid_input', 'actor_ref and viewpoint must both be present or both absent');
  }
  if (hasActor) {
    // Validate shape first: a malformed ref is invalid_input, not not_migrated.
    // `requirePlainObject` rejects null with 400.
    validateActorRef(req.actor_ref);
    validateViewpoint(req.viewpoint);
    throw routeNotMigrated('POST /v1/daemon/agent-host/sessions (actor/viewpoint)');
  }
  const validated: CreateSessionRequest = { provider_id: req.provider_id };
  if (cwd !== undefined) validated.cwd = cwd;
  if (model !== undefined) validated.model = model;
  if (mode !== undefined) validated.mode = mode;
  return validated;
}

/** The only operation branch the service supports is a validated prompt. */
interface ValidatedPrompt {
  kind: 'prompt';
  content: string;
  remember?: boolean;
}

/**
 * Enforce the generated `ExecuteOperationRequest` oneOf before any provider
 * effect. A malformed branch is `invalid_input` (400); a well-formed
 * `set_model`/`set_mode` branch is `not_migrated` (501).
 */
function validateExecuteOperationRequest(body: unknown): ValidatedPrompt {
  const req = requirePlainObject(body, 'operation request body');
  const kind = req.kind;
  if (typeof kind !== 'string') {
    throw new HttpError(400, 'invalid_input', 'kind is required', { field: 'kind' });
  }
  if (kind === 'prompt') {
    rejectUnknownKeys(req, ['kind', 'content', 'remember'], 'prompt request');
    expectString(req.content, 'content');
    if (req.remember !== undefined && typeof req.remember !== 'boolean') {
      throw new HttpError(400, 'invalid_input', 'remember must be a boolean', { field: 'remember' });
    }
    const prompt: ValidatedPrompt = { kind: 'prompt', content: req.content as string };
    if (req.remember !== undefined) prompt.remember = req.remember;
    return prompt;
  }
  if (kind === 'set_model') {
    rejectUnknownKeys(req, ['kind', 'model'], 'set_model request');
    expectString(req.model, 'model');
    throw routeNotMigrated('operation kind set_model');
  }
  if (kind === 'set_mode') {
    rejectUnknownKeys(req, ['kind', 'mode'], 'set_mode request');
    expectString(req.mode, 'mode');
    throw routeNotMigrated('operation kind set_mode');
  }
  throw new HttpError(400, 'invalid_input', `unknown operation kind: ${kind}`, { field: 'kind' });
}

function parseUuid(value: string, field: string): void {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value)) {
    throw new HttpError(400, 'invalid_input', `${field} must be a valid UUID`, { field });
  }
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
  const req = validateCreateSessionRequest(body);
  const payload: Record<string, unknown> = { provider_id: req.provider_id };
  if (req.cwd !== undefined) payload.cwd = req.cwd;
  if (req.model !== undefined) payload.model = req.model;
  if (req.mode !== undefined) payload.mode = req.mode;
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
  const req = validateExecuteOperationRequest(body);
  // Transport admission: cap live operations *before* the provider effect so a
  // stalled/hung population cannot grow without bound (architecture §7).
  if (service.providerRegistry.activeOperationCount() >= MAX_ACTIVE_PROVIDER_OPERATIONS) {
    throw new HttpError(503, 'busy', 'too many active provider operations');
  }
  const executePayload: Record<string, unknown> = { kind: 'prompt', content: req.content };
  if (req.remember !== undefined) executePayload.remember = req.remember;
  const reply = await providerCall(service, {
    method: 'execute',
    request_id: randomUUID(),
    session_id: sessionId,
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload: executePayload,
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
  service.providerRegistry.ensureHub(operationId, () => new OperationEventHub(operationId, sessionId));
  return { operation_id: operationId, session_id: sessionId, status: 'started' };
}

export async function cancelProviderOperation(service: ServiceCore, operationId: string): Promise<CancelOperationResponse> {
  if (service.domainOnly) throw routeNotMigrated(`POST /v1/daemon/agent-host/operations/${operationId}`);
  parseUuid(operationId, 'operation_id');
  const op = await assertKnownOperation(service, operationId);
  if (op.providerId === DSH_PROVIDER_ID) {
    throw new HttpError(501, 'route_not_migrated', 'DSH provider does not support cancellation');
  }
  // Terminal truth may arrive as a hydrated native status (no local event yet),
  // so a null `terminalEvent` alone must never authorise a cancel dispatch.
  if (op.terminalEvent || isTerminalOperationStatus(op.status)) {
    throw new HttpError(409, 'busy', 'operation already terminal', { resource: `operation:${operationId}` });
  }
  await providerCall(service, {
    method: 'cancel',
    request_id: randomUUID(),
    operation_id: operationId,
    deadline_ms: PROVIDER_DEFAULT_DEADLINE_MS,
    payload: {},
  });
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
