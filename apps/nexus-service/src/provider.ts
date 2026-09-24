import { randomUUID } from 'node:crypto';
import type {
  CancelOperationResponse,
  CharacterOperationResult,
  CreateSessionRequest,
  ExecuteOperationRequest,
  ActorRef,
  SessionViewpoint,
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
import { hostQuery, withPrincipal } from './world-kb.js';

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
 * Validate an `actor_ref` against the generated closed sum and return the
 * admitted generated value. A malformed value is `invalid_input`; a valid one
 * is dispatched to the native Host authority unchanged.
 */
function validateActorRef(value: unknown): ActorRef {
  const ref = requirePlainObject(value, 'actor_ref');
  const kind = ref.actor_kind;
  if (kind === 'creator') {
    rejectUnknownKeys(ref, ['actor_kind', 'creator_id'], 'actor_ref');
    expectPattern(expectString(ref.creator_id, 'actor_ref.creator_id'), /^ctr_[a-zA-Z0-9]+$/, 'actor_ref.creator_id');
    return { actor_kind: 'creator', creator_id: ref.creator_id as string };
  }
  if (kind === 'character') {
    rejectUnknownKeys(ref, ['actor_kind', 'character_id'], 'actor_ref');
    expectPattern(expectString(ref.character_id, 'actor_ref.character_id'), /^chr_[0-9a-f]{32}$/, 'actor_ref.character_id');
    return { actor_kind: 'character', character_id: ref.character_id as string };
  }
  throw new HttpError(400, 'invalid_input', 'actor_ref.actor_kind must be creator or character', { field: 'actor_ref.actor_kind' });
}

/** Validate a `viewpoint` shape and return the admitted generated value. */
function validateViewpoint(value: unknown): SessionViewpoint {
  const vp = requirePlainObject(value, 'viewpoint');
  rejectUnknownKeys(vp, ['world_id', 'binding_id', 'branch_id', 'event_id'], 'viewpoint');
  expectPattern(expectString(vp.world_id, 'viewpoint.world_id'), /^wld_[a-zA-Z0-9]+$/, 'viewpoint.world_id');
  const viewpoint: SessionViewpoint = { world_id: vp.world_id as string };
  // An own property must be a valid string; only absence is legal, never null.
  if (hasOwn(vp, 'binding_id')) {
    expectPattern(expectString(vp.binding_id, 'viewpoint.binding_id'), /^awb_[0-9a-f]{32}$/, 'viewpoint.binding_id');
    viewpoint.binding_id = vp.binding_id as string;
  }
  if (hasOwn(vp, 'branch_id')) {
    expectPattern(expectString(vp.branch_id, 'viewpoint.branch_id'), /^fbk_[a-zA-Z0-9]+$/, 'viewpoint.branch_id');
    viewpoint.branch_id = vp.branch_id as string;
  }
  if (hasOwn(vp, 'event_id')) {
    expectPattern(expectString(vp.event_id, 'viewpoint.event_id'), /^evt_[a-zA-Z0-9]+$/, 'viewpoint.event_id');
    viewpoint.event_id = vp.event_id as string;
  }
  return viewpoint;
}

/**
 * Enforce the generated `CreateSessionRequest` shape before any Host/provider
 * effect: plain object only, required/typed optional fields, a valid
 * actor/viewpoint pair, and no unknown keys. Malformed values are
 * `invalid_input` (400). A valid pair is returned as the admitted generated
 * request, because both Actor modes are served by the native Host authority —
 * admission, binding and World ownership are decided there, never here.
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
  const validated: CreateSessionRequest = { provider_id: req.provider_id };
  if (cwd !== undefined) validated.cwd = cwd;
  if (model !== undefined) validated.model = model;
  if (mode !== undefined) validated.mode = mode;
  if (hasActor) {
    // Validate shape first: a malformed ref is invalid_input. `requirePlainObject`
    // rejects null with 400, and only the admitted generated values are kept.
    validated.actor_ref = validateActorRef(req.actor_ref);
    validated.viewpoint = validateViewpoint(req.viewpoint);
  }
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

/** A missing session/operation, or no attached Host authority, is absence — not a fault. */
function isAbsentHostError(error: unknown): boolean {
  const mapped = mapNativeError(error);
  return (
    mapped.code === 'not_found' ||
    (mapped.code === 'invalid_input' && mapped.message === 'host not started')
  );
}

/**
 * Fresh native truth for one session, or `null` when the Host authority has no
 * row for it (unknown id, or no attached authority at all). Every session
 * command resolves its placement here first: an Actor session is recognized
 * from the authority's own row, never from a request payload or a cache guess.
 */
async function tryNativeSession(
  service: ServiceCore,
  sessionId: string,
): Promise<SessionResponse | null> {
  try {
    const response = await hostQuery(service, { query: 'get_session', session_id: sessionId });
    return response.session ?? null;
  } catch (error) {
    if (isAbsentHostError(error)) return null;
    throw mapNativeError(error);
  }
}

/** One resolved session: the mirror record plus the native Actor pair, if any. */
interface SessionPlacement {
  record: ProviderSessionRecord;
  /** Native truth reports a stored Actor pair, so the authority owns the session. */
  actorBacked: boolean;
}

function recordFromSession(session: SessionResponse): ProviderSessionRecord {
  const record: ProviderSessionRecord = {
    sessionId: session.session_id,
    providerId: session.provider_id,
    state: session.state,
    activeOpId: session.active_op_id ?? null,
    model: session.model,
  };
  if (session.actor_ref !== undefined) record.actorRef = session.actor_ref;
  if (session.viewpoint !== undefined) record.viewpoint = session.viewpoint;
  return record;
}

/**
 * Resolve one session's placement before any effect. Fresh native truth decides
 * Actor versus provider-only; the mirror is consulted only when the authority
 * has no row at all, and even then its Actor marker was copied from native
 * truth when the session was created or hydrated.
 */
async function resolveSessionPlacement(
  service: ServiceCore,
  sessionId: string,
): Promise<SessionPlacement> {
  const native = await tryNativeSession(service, sessionId);
  if (native) {
    const record = recordFromSession(native);
    service.providerRegistry.registerSession(record);
    return { record, actorBacked: native.actor_ref !== undefined };
  }
  const cached = service.providerRegistry.sessionRecord(sessionId);
  if (cached) return { record: cached, actorBacked: cached.actorRef !== undefined };
  throw new HttpError(404, 'not_found', `session ${sessionId} not found`, {
    resource: `session:${sessionId}`,
  });
}

/**
 * The authority's own generic operation row for one id, or `null` when it serves
 * none. The mirror is never consulted: it is not authoritative for an Actor
 * operation, and its cache must not resurrect one the authority has aged out
 * (technical contract §5: an expired Actor observation is absent, never
 * re-created as provider-only state).
 */
async function nativeOperationRow(
  service: ServiceCore,
  operationId: string,
): Promise<{ operation_id: string; session_id: string; status: string } | null> {
  try {
    const response = await hostQuery(service, { query: 'get_operation', operation_id: operationId });
    return response.operation ?? null;
  } catch (error) {
    const mapped = mapNativeError(error);
    if (mapped.code !== 'not_found') throw mapped;
    return null;
  }
}

/**
 * Whether the core authority owns this id as an Actor operation.
 *
 * Ownership is resolved from the mirror's own Actor mark — copied from native
 * admission, or from the authority's own read — and, when the mirror holds no
 * row, from fresh native identity: the operation's row names its session and
 * that session's native echo says whether the authority owns it as an Actor
 * session. The absence of a Character result is NEVER the discriminator: by
 * contract (§5) a Creator Actor prompt has no Character result and is still an
 * Actor operation. `false` is therefore a positive statement — the id is proven
 * provider-only — and never an inference from an Actor read that came back
 * empty.
 */
async function isActorOwnedOperation(service: ServiceCore, operationId: string): Promise<boolean> {
  const cached = service.providerRegistry.operationRecord(operationId);
  if (cached) return cached.actorBacked === true;
  if (await lookupCharacterOperation(service, operationId)) return true;
  const owner = (await nativeOperationRow(service, operationId))?.session_id;
  if (!owner) return false;
  const session = await tryNativeSession(service, owner);
  return session?.actor_ref !== undefined;
}

/**
 * Age the mirror's Actor arm before another Actor operation is admitted.
 *
 * The authority settles an Actor run on its own drain, with or without an HTTP
 * subscriber, and owns both the detailed outcome and the bounded observation
 * (technical contract §5). The mirror therefore cannot wait for a subscriber's
 * pull to learn that a run is over: it asks the authority what it already knows
 * and retires every Actor record the authority no longer serves. Running on each
 * admission, the sweep keeps the Actor arm from outliving the core's own
 * retention by more than the operation being admitted — with the existing caps
 * and with no change to the no-subscriber settlement the authority already has.
 */
async function ageSettledActorOperations(service: ServiceCore): Promise<void> {
  for (const record of service.providerRegistry.actorBackedOperations()) {
    const character = await lookupCharacterOperation(service, record.operationId);
    if (character) {
      // A Character operation's own outcome is its liveness.
      if (character.run_status === 'running') continue;
    } else if (await nativeOperationRow(service, record.operationId)) {
      // A Creator Actor operation keeps the generic observation: its own
      // authority row is what makes it live.
      continue;
    }
    service.providerRegistry.retireActorOperation(record.operationId);
  }
}

/**
 * `GET /v1/daemon/agent-host/operations/{operation_id}` Character arm: the
 * authority's own `CharacterOperationResult`, or `null` when the id is not a
 * live/retained core-indexed Character operation (the caller then keeps the
 * generic provider-only/recovered-journal answer).
 */
export async function lookupCharacterOperation(
  service: ServiceCore,
  operationId: string,
): Promise<CharacterOperationResult | null> {
  parseUuid(operationId, 'operation_id');
  try {
    return await withPrincipal(service, (principal) =>
      service.core.hostCharacterOperation(principal, operationId),
    );
  } catch (error) {
    if (isAbsentHostError(error)) return null;
    throw mapNativeError(error);
  }
}

export async function createProviderSession(service: ServiceCore, body: unknown): Promise<SessionResponse> {
  if (service.domainOnly) throw routeNotMigrated('POST /v1/daemon/agent-host/sessions');
  const req = validateCreateSessionRequest(body);
  // Actor mode is served by the ONE core Host authority: it admits the stored
  // Actor/binding/World, owns the session identity and echoes the pair. The
  // provider-only lane below never sees an Actor request, so a failed admission
  // can never degrade into a legacy provider session.
  if (req.actor_ref !== undefined) {
    const session = await withPrincipal(service, (principal) =>
      service.core.hostCreateSession(principal, req),
    );
    service.providerRegistry.registerSession(recordFromSession(session));
    return session;
  }
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
  const placement = await resolveSessionPlacement(service, sessionId);
  if (placement.actorBacked) {
    // The authority owns the release: it cancels the session's live Actor work
    // through the same manager and confirms the session shutdown before it
    // reports success, then retires the Actor reuse state.
    const reply = await withPrincipal(service, (principal) =>
      service.core.hostShutdownSession(principal, sessionId),
    );
    service.providerRegistry.removeSession(sessionId);
    return reply;
  }
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
  const placement = await resolveSessionPlacement(service, sessionId);
  const req = validateExecuteOperationRequest(body);
  if (placement.actorBacked) {
    // Contract §1/§5: this Host has no complete run-capture writer, so a
    // Character `remember:true` is refused BEFORE any effect — no reservation,
    // no provider work, no pending/captured result. Absent/false captures
    // nothing and executes normally with the authority's disabled capture.
    if (req.remember === true) {
      throw routeNotMigrated('character prompt remember is not supported by this host');
    }
    const request: ExecuteOperationRequest = { kind: 'prompt', content: req.content };
    if (req.remember !== undefined) request.remember = req.remember;
    const reply = await withPrincipal(service, (principal) =>
      service.core.hostExecuteOperation(principal, sessionId, request),
    );
    // The mirror keeps delivery bookkeeping only (the hub and the bounded
    // terminal retention); terminal truth stays the authority's
    // `character_operation` read, never a guess from the mirror. Age the arm
    // first: the authority settles Actor runs with or without a subscriber, so
    // this admission is where the mirror learns which of its Actor records the
    // authority no longer serves.
    await ageSettledActorOperations(service);
    service.providerRegistry.registerOperation({
      operationId: reply.operation_id,
      sessionId,
      providerId: placement.record.providerId,
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
    service.providerRegistry.ensureHub(
      reply.operation_id,
      () => new OperationEventHub(reply.operation_id, sessionId),
    );
    return reply;
  }
  // Transport admission: cap live operations *before* the provider effect so a
  // stalled/hung population cannot grow without bound (architecture §7).
  if (service.providerRegistry.activeOperationCount() >= MAX_ACTIVE_PROVIDER_OPERATIONS) {
    throw new HttpError(503, 'busy', 'too many active provider operations');
  }
  // The provider protocol accepts the Rust HostOperation wire shape, not the
  // public HTTP request. A legacy session cannot authorize memory capture.
  if (req.remember === true) {
    throw new HttpError(422, 'invalid_input', 'remember requires an admitted Character session');
  }
  const executePayload = {
    Prompt: {
      op_id: randomUUID(),
      content: [{ Text: { text: req.content } }],
      permission_scope: null,
    },
  };
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
    providerId: placement.record.providerId,
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
  // Every Actor kind is cancelled by the AUTHORITY, decided from the Actor
  // mark or from fresh native identity BEFORE any provider mutation: this host's
  // core retains no cancellable row for a Creator Actor operation and none for
  // an aged-out id, so its own refusal is the answer. A core refusal is never
  // retried as a raw provider call — the provider-only lane is not an authority
  // bypass (technical contract §2/§4).
  if (await isActorOwnedOperation(service, operationId)) {
    const reply = await withPrincipal(service, (principal) =>
      service.core.hostCancelOperation(principal, operationId),
    );
    service.providerRegistry.settleOperationStatus(operationId, 'cancelled');
    return reply;
  }
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
  // Settle the cached record so the accepted cancel is observable and no longer
  // charges the live-operation cap; native truth (journal + EnvState) matches.
  service.providerRegistry.settleOperationStatus(operationId, 'cancelled');
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
  const response: SessionResponse = {
    session_id: cached.sessionId,
    provider_id: cached.providerId,
    state: cached.state,
    active_op_id: cached.activeOpId ?? undefined,
    model: cached.model,
  };
  // The mirror is a cache of native truth: an Actor session keeps its echoed
  // pair (copied from the authority when it was created or hydrated) so a cold
  // or restart-level read never downgrades it to a provider-only session.
  if (cached.actorRef !== undefined) response.actor_ref = cached.actorRef;
  if (cached.viewpoint !== undefined) response.viewpoint = cached.viewpoint;
  return response;
}

export async function lookupProviderOperation(
  service: ServiceCore,
  operationId: string,
): Promise<{ operation_id: string; session_id: string; status: string } | null> {
  parseUuid(operationId, 'operation_id');
  const native = await nativeOperationRow(service, operationId);
  if (native) return native;
  const cached = service.providerRegistry.operationRecord(operationId);
  // The mirror speaks only for provider-only operations. An Actor operation's
  // truth is the authority's own, so with no native row it is absent — never a
  // stale mirror row served as a generic provider-only observation.
  if (!cached || cached.actorBacked === true) return null;
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
