/**
 * Desktop host contract (v1.192 P0-T1) — the ONE host contract between the
 * Electron main process, the preload bridge and the SPA.
 *
 * This module is deliberately free of any Electron or Node-only import so the
 * SPA (apps/web) can import **types only** from it without pulling a runtime
 * dependency on the host. All bounds, operation names, envelope shapes and
 * parsers live here; `desktop-ipc.ts` (main) and `preload.ts` (bridge) consume
 * them. Frozen by the plan's "IPC and trust boundary" table and
 * `.mstar/specs/desktop-shell.md` (Electron rewrite target, P0-T9).
 */

// ---------------------------------------------------------------------------
// Channels and bridge identity
// ---------------------------------------------------------------------------

export const DESKTOP_BRIDGE_VERSION = 1;

/**
 * Envelope wire version. Every request/response frame on the desktop invoke
 * channel carries `version: DESKTOP_ENVELOPE_VERSION`; main and preload both
 * reject any other value before any effect, so a future incompatible
 * envelope cannot be silently processed.
 */
export const DESKTOP_ENVELOPE_VERSION = 1;

/** Renderer → main invoke channel (typed operation envelope). */
export const DESKTOP_INVOKE_CHANNEL = 'nexus:desktop:invoke';
/** Main → renderer status event channel. */
export const DESKTOP_STATUS_CHANNEL = 'nexus:desktop:status-changed';
/** Preload → main synchronous trusted-runtime fetch (nonsecret metadata only). */
export const DESKTOP_RUNTIME_CHANNEL = 'nexus:desktop:runtime';

// ---------------------------------------------------------------------------
// Bounds (frozen)
// ---------------------------------------------------------------------------

export const MAX_REQUEST_BYTES = 1024 * 1024; // total serialized request AND response
export const MAX_STATUS_BYTES = 4 * 1024; // status event frame
export const MAX_DIAGNOSTIC_BYTES = 2 * 1024; // status detail tail
export const MAX_PATH_BYTES = 4096;
export const MAX_URL_BYTES = 8192;
export const MAX_CREATOR_ID_BYTES = 256;
export const MAX_AGENT_NAME_BYTES = 256;
export const MAX_LAUNCH_COMMAND_BYTES = 8192;
export const MAX_CONNECTION_JSON_BYTES = 64 * 1024;

export const MAX_ACTIVE_CALLS = 32;
export const MAX_QUEUED_CALLS = 16;
export const MAX_QUEUED_BYTES = MAX_REQUEST_BYTES;
export const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;
export const LIFECYCLE_RESTART_TIMEOUT_MS = 25_000;

const REQUEST_ID_RE = /^[A-Za-z0-9._-]{1,128}$/;

/** NUL, C0 controls and DEL are rejected in every path/URL/id-bearing field. */
function hasControlChars(value: string): boolean {
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Operations (closed union — frozen host contract table)
// ---------------------------------------------------------------------------

export const DESKTOP_OPERATIONS = [
  'open_with',
  'reveal_in_finder',
  'open_external_url',
  'pick_directory',
  'get_workspace_root',
  'set_workspace_path',
  'switch_active_creator',
  'ensure_setup_bootstrap',
  'get_entrance',
  'set_entrance',
  'get_setup_completed',
  'set_setup_completed',
  'get_agent_profile',
  'set_agent_profile',
  'get_connection_config',
  'set_connection_config',
  'delete_connection_config',
  'get_daemon_status',
  'start_daemon',
  'stop_daemon',
  'restart_daemon',
  'reset_local_database',
  'toggle_maximize_window',
] as const;

export type DesktopOperation = (typeof DESKTOP_OPERATIONS)[number];

export type NullPayloadOperations =
  | 'get_workspace_root'
  | 'ensure_setup_bootstrap'
  | 'get_entrance'
  | 'get_setup_completed'
  | 'get_agent_profile'
  | 'get_connection_config'
  | 'delete_connection_config'
  | 'get_daemon_status'
  | 'start_daemon'
  | 'stop_daemon'
  | 'restart_daemon'
  | 'reset_local_database'
  | 'toggle_maximize_window';

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

export type EntranceId = 'developer' | 'content-creator';

export interface AgentProfile {
  name: string;
  launchCommand?: string;
}

export interface SetupBootstrapResult {
  creator_id: string;
  already_bootstrapped: boolean;
}

/** Public connection projection — NEVER contains the API key. */
export interface PublicConnectionConfig {
  endpointUrl: string;
  label?: string;
  active?: boolean;
  pinnedFingerprint?: string;
  hasApiKey: boolean;
}

/** Ephemeral credential update. Omitted key means `keep`. */
export type ConnectionCredentialUpdate =
  | { action: 'keep' }
  | { action: 'replace'; value: string };

export type DaemonState = 'starting' | 'running' | 'degraded' | 'stopped' | 'error';

/** Existing status shape {state, version?, port, detail?} — preserved. */
export interface DaemonStatus {
  state: DaemonState;
  version?: string;
  port: number;
  detail?: string;
}

/** Immutable nonsecret runtime metadata populated by main before preload. */
export interface DesktopRuntimeMetadata {
  localEndpoint: string;
}

// ---------------------------------------------------------------------------
// Per-operation payload / result maps (typed invoke surface)
// ---------------------------------------------------------------------------

export interface DesktopOperationPayload {
  open_with: { path: string };
  reveal_in_finder: { path: string };
  open_external_url: { url: string };
  pick_directory: { defaultPath: string };
  get_workspace_root: undefined;
  set_workspace_path: { path: string };
  switch_active_creator: { creatorId: string };
  ensure_setup_bootstrap: undefined;
  get_entrance: undefined;
  set_entrance: { value: EntranceId };
  get_setup_completed: undefined;
  set_setup_completed: { value: boolean };
  get_agent_profile: undefined;
  set_agent_profile: AgentProfile;
  get_connection_config: undefined;
  set_connection_config: {
    config: PublicConnectionConfig;
    credential: ConnectionCredentialUpdate;
  };
  delete_connection_config: undefined;
  get_daemon_status: undefined;
  start_daemon: undefined;
  stop_daemon: undefined;
  restart_daemon: undefined;
  reset_local_database: undefined;
  toggle_maximize_window: undefined;
}

export interface DesktopOperationResult {
  open_with: null;
  reveal_in_finder: null;
  open_external_url: null;
  pick_directory: string | null;
  get_workspace_root: string;
  set_workspace_path: null;
  switch_active_creator: string;
  ensure_setup_bootstrap: SetupBootstrapResult;
  get_entrance: EntranceId;
  set_entrance: null;
  get_setup_completed: boolean;
  set_setup_completed: null;
  get_agent_profile: AgentProfile | null;
  set_agent_profile: null;
  get_connection_config: PublicConnectionConfig | null;
  set_connection_config: PublicConnectionConfig;
  delete_connection_config: null;
  get_daemon_status: DaemonStatus;
  start_daemon: null;
  stop_daemon: null;
  restart_daemon: null;
  reset_local_database: null;
  toggle_maximize_window: null;
}

// ---------------------------------------------------------------------------
// Envelope
// ---------------------------------------------------------------------------

export interface DesktopRequest {
  version: typeof DESKTOP_ENVELOPE_VERSION;
  request_id: string;
  operation: DesktopOperation;
  payload?: unknown;
}

export interface DesktopSuccess {
  version: typeof DESKTOP_ENVELOPE_VERSION;
  request_id: string;
  ok: true;
  result: unknown;
}

export interface DesktopFailure {
  version: typeof DESKTOP_ENVELOPE_VERSION;
  request_id: string;
  ok: false;
  error: { code: string; message: string };
}

export type DesktopResponse = DesktopSuccess | DesktopFailure;

// ---------------------------------------------------------------------------
// Error helpers (proof error-envelope pattern, reused)
// ---------------------------------------------------------------------------

export function desktopError(code: string, message: string): Error {
  const err = new Error(message) as Error & { code: string };
  err.code = code;
  return err;
}

export function errorCode(err: unknown): string {
  if (
    err &&
    typeof err === 'object' &&
    'code' in err &&
    typeof (err as { code: unknown }).code === 'string'
  ) {
    return (err as { code: string }).code;
  }
  return 'internal';
}

export function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

/**
 * Success envelope: the COMPLETE serialized response frame (version,
 * request_id, ok, result) must fit the frozen 1 MiB bound — not just the
 * result body.
 */
export function desktopOk(request_id: string, result: unknown): DesktopSuccess {
  const response: DesktopSuccess = {
    version: DESKTOP_ENVELOPE_VERSION,
    request_id,
    ok: true,
    result,
  };
  if (jsonBytes(response) > MAX_REQUEST_BYTES) {
    throw desktopError('internal', 'response exceeds bounded encode limit');
  }
  return response;
}

/** Machine-readable error codes are short identifiers; longer ones collapse. */
const MAX_ERROR_CODE_CHARS = 64;

/**
 * Failure envelope: never throws (it IS the failure path). Error code and
 * message are bounded so the COMPLETE serialized frame fits the frozen 1 MiB
 * response bound; an over-long message is trimmed and, as a hard guard, any
 * still-oversized frame collapses to a minimal internal failure.
 */
export function desktopErr(request_id: string, code: string, message: string): DesktopFailure {
  const boundedCode =
    typeof code === 'string' && code.length > 0 && code.length <= MAX_ERROR_CODE_CHARS
      ? code
      : 'internal';
  const build = (msg: string): DesktopFailure => ({
    version: DESKTOP_ENVELOPE_VERSION,
    request_id,
    ok: false,
    error: { code: boundedCode, message: msg },
  });
  let boundedMessage = typeof message === 'string' ? message : String(message);
  // Trim the message until the complete frame fits; JSON escaping overhead is
  // covered by measuring the built envelope, not just the raw string bytes.
  let response = build(boundedMessage);
  while (jsonBytes(response) > MAX_REQUEST_BYTES && boundedMessage.length > 0) {
    boundedMessage = boundedMessage.slice(0, Math.floor(boundedMessage.length / 2));
    response = build(boundedMessage);
  }
  if (jsonBytes(response) > MAX_REQUEST_BYTES) {
    response = build('desktop operation failed');
  }
  return response;
}

export function isDesktopResponse(value: unknown): value is DesktopResponse {
  if (!value || typeof value !== 'object') return false;
  if (!('version' in value) || !('request_id' in value) || !('ok' in value)) return false;
  return (
    value.version === DESKTOP_ENVELOPE_VERSION &&
    typeof value.request_id === 'string' &&
    typeof value.ok === 'boolean'
  );
}

// ---------------------------------------------------------------------------
// Encoding / validation primitives (TextEncoder — host-agnostic)
// ---------------------------------------------------------------------------

const encoder = new TextEncoder();

export function jsonBytes(value: unknown): number {
  return encoder.encode(JSON.stringify(value)).length;
}

function assertNoControlChars(value: string, what: string): void {
  if (hasControlChars(value)) {
    throw desktopError('invalid_input', `${what} must not contain control characters`);
  }
}

function assertByteLength(value: string, max: number, what: string): void {
  if (encoder.encode(value).length > max) {
    throw desktopError('input_too_large', `${what} exceeds ${max} bytes`);
  }
}

function boundedString(
  body: Record<string, unknown>,
  key: string,
  max: number,
  what: string,
): string {
  const value = body[key];
  if (typeof value !== 'string' || value.length === 0) {
    throw desktopError('invalid_input', `${what} must be a non-empty string`);
  }
  assertNoControlChars(value, what);
  assertByteLength(value, max, what);
  return value;
}

function optionalBoundedString(
  body: Record<string, unknown>,
  key: string,
  max: number,
  what: string,
): string | undefined {
  const value = body[key];
  if (value === undefined) return undefined;
  if (typeof value !== 'string') {
    throw desktopError('invalid_input', `${what} must be a string`);
  }
  assertNoControlChars(value, what);
  assertByteLength(value, max, what);
  return value;
}

/** Closed object shape: exact key set, no extras, no arrays, no null. */
function exactObject(value: unknown, keys: readonly string[], what: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw desktopError('invalid_input', `${what} must be an object`);
  }
  const body = value as Record<string, unknown>;
  for (const key of Object.keys(body)) {
    if (!keys.includes(key)) {
      throw desktopError('invalid_input', `${what} has unknown field: ${key}`);
    }
  }
  return body;
}

function validatePathPayload(payload: unknown, key: 'path'): { path: string } {
  const body = exactObject(payload, [key], 'payload');
  return { path: boundedString(body, key, MAX_PATH_BYTES, key) };
}

function validateNullPayload(operation: DesktopOperation, payload: unknown): void {
  if (payload !== undefined && payload !== null) {
    throw desktopError('invalid_input', `${operation} takes no payload`);
  }
}

function validatePublicConnectionConfig(raw: unknown): PublicConnectionConfig {
  const body = exactObject(
    raw,
    ['endpointUrl', 'label', 'active', 'pinnedFingerprint', 'hasApiKey'],
    'connection config',
  );
  if (typeof body.hasApiKey !== 'boolean') {
    throw desktopError('invalid_input', 'hasApiKey must be a boolean');
  }
  const config: PublicConnectionConfig = {
    endpointUrl: boundedString(body, 'endpointUrl', MAX_URL_BYTES, 'endpointUrl'),
    hasApiKey: body.hasApiKey,
  };
  const label = optionalBoundedString(body, 'label', 256, 'label');
  if (label !== undefined) config.label = label;
  if (body.active !== undefined) {
    if (typeof body.active !== 'boolean') {
      throw desktopError('invalid_input', 'active must be a boolean');
    }
    config.active = body.active;
  }
  const fingerprint = optionalBoundedString(body, 'pinnedFingerprint', 256, 'pinnedFingerprint');
  if (fingerprint !== undefined) config.pinnedFingerprint = fingerprint;
  return config;
}

function validateCredentialUpdate(raw: unknown): ConnectionCredentialUpdate {
  const body = exactObject(raw, ['action', 'value'], 'credential update');
  if (body.action === 'keep') {
    if (body.value !== undefined) {
      throw desktopError('invalid_input', 'keep credential must not carry a value');
    }
    return { action: 'keep' };
  }
  if (body.action === 'replace') {
    if (typeof body.value !== 'string') {
      throw desktopError('invalid_input', 'replace credential requires a string value');
    }
    assertNoControlChars(body.value, 'credential value');
    assertByteLength(body.value, MAX_CONNECTION_JSON_BYTES, 'credential value');
    return { action: 'replace', value: body.value };
  }
  throw desktopError('invalid_input', `unsupported credential action: ${String(body.action)}`);
}

type PayloadValidator = (operation: DesktopOperation, payload: unknown) => unknown;

const NULL_PAYLOAD_OPS: Record<NullPayloadOperations, true> = {
  get_workspace_root: true,
  ensure_setup_bootstrap: true,
  get_entrance: true,
  get_setup_completed: true,
  get_agent_profile: true,
  get_connection_config: true,
  delete_connection_config: true,
  get_daemon_status: true,
  start_daemon: true,
  stop_daemon: true,
  restart_daemon: true,
  reset_local_database: true,
  toggle_maximize_window: true,
};

const OPERATION_VALIDATORS: Record<DesktopOperation, PayloadValidator> = {
  open_with: (_op, payload) => validatePathPayload(payload, 'path'),
  reveal_in_finder: (_op, payload) => validatePathPayload(payload, 'path'),
  open_external_url: (_op, payload) => {
    const body = exactObject(payload, ['url'], 'payload');
    return { url: boundedString(body, 'url', MAX_URL_BYTES, 'url') };
  },
  pick_directory: (_op, payload) => {
    const body = exactObject(payload, ['defaultPath'], 'payload');
    return { defaultPath: boundedString(body, 'defaultPath', MAX_PATH_BYTES, 'defaultPath') };
  },
  get_workspace_root: (op, payload) => validateNullPayload(op, payload),
  set_workspace_path: (_op, payload) => validatePathPayload(payload, 'path'),
  switch_active_creator: (_op, payload) => {
    const body = exactObject(payload, ['creatorId'], 'payload');
    return { creatorId: boundedString(body, 'creatorId', MAX_CREATOR_ID_BYTES, 'creatorId') };
  },
  ensure_setup_bootstrap: (op, payload) => validateNullPayload(op, payload),
  get_entrance: (op, payload) => validateNullPayload(op, payload),
  set_entrance: (_op, payload) => {
    const body = exactObject(payload, ['value'], 'payload');
    if (body.value !== 'developer' && body.value !== 'content-creator') {
      throw desktopError('invalid_input', 'value must be developer or content-creator');
    }
    return { value: body.value };
  },
  get_setup_completed: (op, payload) => validateNullPayload(op, payload),
  set_setup_completed: (_op, payload) => {
    const body = exactObject(payload, ['value'], 'payload');
    if (typeof body.value !== 'boolean') {
      throw desktopError('invalid_input', 'value must be a boolean');
    }
    return { value: body.value };
  },
  get_agent_profile: (op, payload) => validateNullPayload(op, payload),
  set_agent_profile: (_op, payload) => {
    const body = exactObject(payload, ['name', 'launchCommand'], 'agent profile');
    const profile: AgentProfile = {
      name: boundedString(body, 'name', MAX_AGENT_NAME_BYTES, 'name'),
    };
    const launchCommand = optionalBoundedString(
      body,
      'launchCommand',
      MAX_LAUNCH_COMMAND_BYTES,
      'launchCommand',
    );
    if (launchCommand !== undefined) profile.launchCommand = launchCommand;
    return profile;
  },
  get_connection_config: (op, payload) => validateNullPayload(op, payload),
  set_connection_config: (_op, payload) => {
    const body = exactObject(payload, ['config', 'credential'], 'payload');
    return {
      config: validatePublicConnectionConfig(body.config),
      credential: validateCredentialUpdate(body.credential),
    };
  },
  delete_connection_config: (op, payload) => validateNullPayload(op, payload),
  get_daemon_status: (op, payload) => validateNullPayload(op, payload),
  start_daemon: (op, payload) => validateNullPayload(op, payload),
  stop_daemon: (op, payload) => validateNullPayload(op, payload),
  restart_daemon: (op, payload) => validateNullPayload(op, payload),
  reset_local_database: (op, payload) => validateNullPayload(op, payload),
  toggle_maximize_window: (op, payload) => validateNullPayload(op, payload),
};

export function assertDesktopOperation(value: unknown): DesktopOperation {
  if (
    typeof value !== 'string' ||
    !DESKTOP_OPERATIONS.includes(value as DesktopOperation)
  ) {
    throw desktopError('invalid_input', `unsupported operation: ${String(value)}`);
  }
  return value as DesktopOperation;
}

/**
 * Parse and validate a raw renderer invoke envelope. Rejects unknown fields,
 * unknown operations, oversized frames and malformed payloads BEFORE any
 * effect runs. Returned payload is the normalized, closed-shape value.
 */
export function parseDesktopRequest(raw: unknown): DesktopRequest {
  if (jsonBytes(raw ?? null) > MAX_REQUEST_BYTES) {
    throw desktopError('input_too_large', `request exceeds ${MAX_REQUEST_BYTES} bytes`);
  }
  const body = exactObject(raw, ['version', 'request_id', 'operation', 'payload'], 'request');
  if (body.version !== DESKTOP_ENVELOPE_VERSION) {
    throw desktopError(
      'invalid_input',
      `unsupported envelope version: ${String(body.version)}`,
    );
  }
  const request_id = body.request_id;
  if (typeof request_id !== 'string' || !REQUEST_ID_RE.test(request_id)) {
    throw desktopError('invalid_input', 'request_id must be a bounded identifier');
  }
  const operation = assertDesktopOperation(body.operation);
  const validated = OPERATION_VALIDATORS[operation](operation, body.payload);
  const request: DesktopRequest = { version: DESKTOP_ENVELOPE_VERSION, request_id, operation };
  if (validated !== undefined && validated !== null) request.payload = validated;
  return request;
}

export function isNullPayloadOperation(operation: DesktopOperation): boolean {
  return operation in NULL_PAYLOAD_OPS;
}

/**
 * Host-agnostic preload/contract parity manifest. The sandboxed preload
 * mirrors the contract by compilation constraint (CommonJS preload + ESM
 * contract share one dist/); tests assert the COMPILED preload's mirror
 * values against this manifest, so a drift on either side fails the parity
 * lock. Every value here is the canonical definition.
 */
export const DESKTOP_BRIDGE_MANIFEST = {
  version: DESKTOP_BRIDGE_VERSION,
  envelopeVersion: DESKTOP_ENVELOPE_VERSION,
  invokeChannel: DESKTOP_INVOKE_CHANNEL,
  statusChannel: DESKTOP_STATUS_CHANNEL,
  runtimeChannel: DESKTOP_RUNTIME_CHANNEL,
  maxStatusBytes: MAX_STATUS_BYTES,
  maxDiagnosticBytes: MAX_DIAGNOSTIC_BYTES,
  maxUrlBytes: MAX_URL_BYTES,
  operations: DESKTOP_OPERATIONS,
} as const;

// ---------------------------------------------------------------------------
// Status frame + runtime metadata bounds (preload and main both enforce)
// ---------------------------------------------------------------------------

export function assertDesktopStatusFrame(raw: unknown): DaemonStatus {
  const body = exactObject(raw, ['state', 'version', 'port', 'detail'], 'status frame');
  if (typeof body.port !== 'number' || !Number.isInteger(body.port) || body.port < 0 || body.port > 65535) {
    throw desktopError('invalid_input', 'status port must be an integer in 0..65535');
  }
  const state = boundedString(body, 'state', 32, 'state');
  if (
    state !== 'starting' &&
    state !== 'running' &&
    state !== 'degraded' &&
    state !== 'stopped' &&
    state !== 'error'
  ) {
    throw desktopError('invalid_input', `unknown status state: ${state}`);
  }
  const status: DaemonStatus = {
    state,
    port: body.port,
  };
  const version = optionalBoundedString(body, 'version', 64, 'version');
  if (version !== undefined) status.version = version;
  const detail = optionalBoundedString(body, 'detail', MAX_DIAGNOSTIC_BYTES, 'detail');
  if (detail !== undefined) status.detail = detail;
  if (jsonBytes(status) > MAX_STATUS_BYTES) {
    throw desktopError('input_too_large', `status frame exceeds ${MAX_STATUS_BYTES} bytes`);
  }
  return status;
}

export function parseDesktopRuntimeMetadata(raw: unknown): DesktopRuntimeMetadata {
  const body = exactObject(raw, ['localEndpoint'], 'desktop runtime');
  const localEndpoint = boundedString(body, 'localEndpoint', MAX_URL_BYTES, 'localEndpoint');
  let parsed: URL;
  try {
    parsed = new URL(localEndpoint);
  } catch {
    throw desktopError('invalid_input', 'localEndpoint must be an absolute URL');
  }
  if ((parsed.protocol !== 'http:' && parsed.protocol !== 'https:') || !parsed.host) {
    throw desktopError('invalid_input', 'localEndpoint must be an http(s) URL with a host');
  }
  return { localEndpoint };
}

// ---------------------------------------------------------------------------
// External URL policy — the ONE main-owned predicate (parity row 25), shared
// by the protocol/navigation layer (P0-T1) and the `open_external_url`
// action (P0-T3). Checked on the RAW input string before any parsing:
// WHATWG URL normalization strips/remaps some C0 controls, so a post-parse
// check alone would accept control-bearing input. http/https with a
// nonempty host only; no userinfo, no surrounding whitespace, no controls.
// ---------------------------------------------------------------------------

export function isAllowedDesktopExternalUrl(url: unknown): boolean {
  if (typeof url !== 'string' || url.length === 0) return false;
  if (jsonBytes(url) > MAX_URL_BYTES) return false;
  if (hasControlChars(url)) return false;
  if (url !== url.trim()) return false;
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') return false;
  if (!parsed.hostname) return false;
  if (parsed.username !== '' || parsed.password !== '') return false;
  return true;
}

// ---------------------------------------------------------------------------
// App origin (nexus://app) — shared by desktop-ipc sender checks and
// protocol navigation policy. Dev HMR origins are opt-in, never default.
// ---------------------------------------------------------------------------

export const DESKTOP_SCHEME = 'nexus';
export const DESKTOP_HOST = 'app';
export const DESKTOP_ORIGIN = 'nexus://app';

export interface DesktopOriginOptions {
  /** Explicitly launched dev HMR on localhost:5173 only. */
  dev?: boolean;
}

/**
 * Exact app-origin match: scheme nexus, host app, no credentials, no port,
 * no path beyond '/'. Anything else (file:, https:, other host, userinfo) is
 * rejected.
 */
export function isDesktopAppOrigin(url: string, options?: DesktopOriginOptions): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.username !== '' || parsed.password !== '') return false;
  if (parsed.protocol === `${DESKTOP_SCHEME}:`) {
    return parsed.hostname === DESKTOP_HOST && (parsed.port === '' || parsed.port === '0');
  }
  if (options?.dev === true && parsed.protocol === 'http:') {
    const host = parsed.hostname.toLowerCase();
    return (host === 'localhost' || host === '127.0.0.1') && parsed.port === '5173';
  }
  return false;
}

// ---------------------------------------------------------------------------
// Preload bridge surface (window.nexusDesktop, version 1)
// ---------------------------------------------------------------------------

export interface DesktopBridge {
  version: typeof DESKTOP_BRIDGE_VERSION;
  runtime: DesktopRuntimeMetadata;
  invoke: {
    <O extends DesktopOperation>(
      operation: O,
      ...args: DesktopOperationPayload[O] extends undefined
        ? []
        : [payload: DesktopOperationPayload[O]]
    ): Promise<DesktopOperationResult[O]>;
  };
  /** Returns an unsubscribe function; never passes Electron event objects. */
  onStatusChanged(listener: (status: DaemonStatus) => void): () => void;
}

declare global {
  interface Window {
    nexusDesktop?: DesktopBridge;
  }
}
