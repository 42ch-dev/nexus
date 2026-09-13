/**
 * Bounded IPC contract for the P3 Electron proof shell.
 * Messages: request_id + operation + owned payload (architecture §6–7, plan P3-T2).
 */

export const MAX_REQUEST_BYTES = 1024 * 1024;
export const MAX_BATCH_BYTES = 256 * 1024;
export const MAX_PULL_EVENTS = 16;
export const MAX_ACTIVE_CALLS = 32;
export const MAX_PENDING_CALLS = 16;
export const MAX_PENDING_BYTES = MAX_REQUEST_BYTES;
export const CLOSE_JOIN_MS = 5000;
export const INTERRUPT_NOTIFY_MS = 5000;
export const DEFAULT_REQUEST_TIMEOUT_MS = 30_000;

export const ALLOWED_OPERATIONS = [
  'compatibility',
  'open',
  'graph',
  'patch',
  'provider',
  'pull',
  'close',
] as const;

export type ProofOperation = (typeof ALLOWED_OPERATIONS)[number];

export type LifecyclePhase =
  | 'idle'
  | 'starting'
  | 'open'
  | 'closing'
  | 'closed'
  | 'interrupted';

export interface IpcRequest {
  request_id: string;
  operation: ProofOperation;
  payload?: unknown;
}

export interface IpcSuccess {
  request_id: string;
  ok: true;
  result: unknown;
}

export interface IpcFailure {
  request_id: string;
  ok: false;
  error: {
    code: string;
    message: string;
  };
}

export type IpcResponse = IpcSuccess | IpcFailure;

export interface UtilityConfig {
  user_home: string;
  access: 'read_only' | 'direct_writer' | 'engine_owner';
  allow_uninitialized: boolean;
}

export interface LifecycleStatus {
  phase: LifecyclePhase;
  owner_alive: boolean;
  cleanup_confirmed: boolean | null;
  pending_operations: string[];
  reason: string | null;
  last_close_report: unknown | null;
}

export interface PreloadProofStep {
  step: string;
  payload?: Record<string, unknown>;
}

const REQUEST_ID_RE = /^[a-zA-Z0-9._-]{1,128}$/;

export function assertOperation(value: unknown): ProofOperation {
  if (typeof value !== 'string' || !ALLOWED_OPERATIONS.includes(value as ProofOperation)) {
    throw ipcError('invalid_input', `unsupported operation: ${String(value)}`);
  }
  return value as ProofOperation;
}

export function parseIpcRequest(raw: unknown): IpcRequest {
  if (!raw || typeof raw !== 'object') {
    throw ipcError('invalid_input', 'request must be an object');
  }
  const body = raw as Record<string, unknown>;
  const request_id = body.request_id;
  if (typeof request_id !== 'string' || !REQUEST_ID_RE.test(request_id)) {
    throw ipcError('invalid_input', 'request_id must be a bounded identifier');
  }
  const operation = assertOperation(body.operation);
  const payload = body.payload;
  if (payload !== undefined) {
    const bytes = Buffer.byteLength(JSON.stringify(payload), 'utf8');
    if (bytes > MAX_REQUEST_BYTES) {
      throw ipcError('input_too_large', `payload exceeds ${MAX_REQUEST_BYTES} bytes`);
    }
  }
  return { request_id, operation, payload };
}

export function ipcOk(request_id: string, result: unknown): IpcSuccess {
  const encoded = Buffer.byteLength(JSON.stringify(result ?? null), 'utf8');
  if (encoded > MAX_REQUEST_BYTES) {
    throw ipcError('internal', 'response exceeds bounded encode limit');
  }
  return { request_id, ok: true, result };
}

export function ipcErr(request_id: string, code: string, message: string): IpcFailure {
  return {
    request_id,
    ok: false,
    error: { code, message },
  };
}

export function ipcError(code: string, message: string): Error {
  const err = new Error(message);
  (err as Error & { code: string }).code = code;
  return err;
}

export function errorCode(err: unknown): string {
  if (err && typeof err === 'object' && 'code' in err && typeof (err as { code: unknown }).code === 'string') {
    return (err as { code: string }).code;
  }
  return 'internal';
}

export function errorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}

export function estimatePayloadBytes(payload: unknown): number {
  if (payload === undefined) return 0;
  return Buffer.byteLength(JSON.stringify(payload), 'utf8');
}

export function isIpcResponse(value: unknown): value is IpcResponse {
  if (!value || typeof value !== 'object') return false;
  const body = value as Partial<IpcResponse>;
  return typeof body.request_id === 'string' && typeof body.ok === 'boolean';
}

export function clampPullBounds(
  maxEventsRaw: unknown,
  maxBytesRaw: unknown,
): { maxEvents: number; maxBytes: number } {
  const maxEvents = maxEventsRaw === undefined ? MAX_PULL_EVENTS : Number(maxEventsRaw);
  const maxBytes = maxBytesRaw === undefined ? MAX_BATCH_BYTES : Number(maxBytesRaw);
  if (
    !Number.isFinite(maxEvents) ||
    !Number.isInteger(maxEvents) ||
    maxEvents < 0 ||
    !Number.isFinite(maxBytes) ||
    !Number.isInteger(maxBytes) ||
    maxBytes < 0
  ) {
    throw ipcError('invalid_input', 'max_events and max_bytes must be finite non-negative integers');
  }
  return {
    maxEvents: Math.min(maxEvents, MAX_PULL_EVENTS),
    maxBytes: Math.min(maxBytes, MAX_BATCH_BYTES),
  };
}

/** Renderer-facing proof steps; each maps to a whitelisted utility operation. */
export const PROOF_STEPS = [
  'compatibility',
  'open',
  'graph',
  'patch',
  'provider_probe',
  'provider_pull',
  'close',
  'lifecycle_status',
] as const;

export type ProofStepName = (typeof PROOF_STEPS)[number];

export function assertProofStep(value: unknown): ProofStepName {
  if (typeof value !== 'string' || !PROOF_STEPS.includes(value as ProofStepName)) {
    throw ipcError('invalid_input', `unsupported proof step: ${String(value)}`);
  }
  return value as ProofStepName;
}
