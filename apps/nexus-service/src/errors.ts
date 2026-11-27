import { parseNativeCoreError } from '@42ch/nexus-native';
import type { CoreError } from '@42ch/nexus-contracts';

import applyRefusalClasses from './apply-refusal-classes.json' with { type: 'json' };

export interface ApiErrorBody {
  success: false;
  error: {
    code: string;
    message: string;
    details?: Record<string, unknown>;
    request_id?: string;
  };
}

export class HttpError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details?: Record<string, unknown>;

  constructor(status: number, code: string, message: string, details?: Record<string, unknown>) {
    super(message);
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

const STATUS_BY_CODE: Record<CoreError['code'], number> = {
  uninitialized: 409,
  auth_required: 401,
  invalid_input: 400,
  forbidden: 403,
  not_found: 404,
  not_supported: 501,
  world_kb_conflict: 409,
  world_kb_validation: 422,
  writer_fenced: 409,
  owner_busy: 409,
  schema_mismatch: 409,
  busy: 503,
  closing: 503,
  interrupted: 503,
  policy_blocked: 403,
  service_unavailable: 503,
  compute_fuel_exhausted: 422,
  compute_wall_time_exceeded: 422,
  compute_memory_cap_exceeded: 422,
  compute_module_trapped: 422,
  compute_module_error: 422,
  outline_conflict: 409,
  outline_validation_failed: 422,
  strategy_candidate_conflict: 409,
  strategy_already_approved: 409,
  workspace_conflict: 409,
  internal: 500,
};

const PUBLIC_INTERNAL_MESSAGE = 'Internal server error';

/**
 * 409 follows one rule, not a bare list: the request is well-formed and the approval it names
 * exists, but the state that approval recorded has moved, so retrying this exact request cannot
 * succeed — it is already the current apply, or its frozen bytes no longer hash to the identity
 * the approval recorded. The client reloads the applied status or re-approves. Every other refusal
 * needs a *different* request (another approval, a user preset, a schedule, an approval whose
 * document builds), so it keeps the core's client-error status; that includes
 * `document_unbuildable`, whose remedy is a new approval, not a retry.
 *
 * The rule is stated once, as `ApplyRefusal::class` in `crates/nexus-core/src/apply.rs`; the
 * checked-in table below is its projection, and that crate's
 * `the_service_classification_table_covers_every_variant` fails when a variant and its row
 * disagree.
 */
const APPLY_CONFLICT_REASONS: ReadonlySet<string> = new Set(
  Object.entries(applyRefusalClasses as Record<string, string>)
    .filter(([, refusalClass]) => refusalClass === 'conflict')
    .map(([reason]) => reason),
);

/** The envelope code of a conflict-shaped apply refusal, or `undefined` for a client error. */
function applyConflictCode(details: Record<string, unknown> | undefined): string | undefined {
  if (details?.code !== 'apply_refused') return undefined;
  const reason = String(details.reason);
  return APPLY_CONFLICT_REASONS.has(reason) ? reason : undefined;
}

export function statusForCode(code: string): number {
  if (Object.hasOwn(STATUS_BY_CODE, code)) {
    return STATUS_BY_CODE[code as CoreError['code']];
  }
  if (code === 'route_not_migrated') return 501;
  if (code === 'input_too_large') return 413;
  return 500;
}

export function routeNotMigrated(path: string): HttpError {
  return new HttpError(
    501,
    'route_not_migrated',
    `Route is not migrated in the standalone service profile: ${path}`,
  );
}

/**
 * Map a native rejection to the daemon-compatible HTTP error envelope.
 *
 * Every migrated native code arrives as a generated `CoreError` JSON envelope
 * (structured conflict/validation details included); the only remaining
 * non-wire rejections are this service's own pre-wire validation errors and
 * unmapped internal faults, which are never leaked to the client.
 */
export function mapNativeError(error: unknown): HttpError {
  if (error instanceof HttpError) return error;

  const wire = parseNativeCoreError(error);
  if (wire) {
    if (wire.code === 'internal') {
      return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
    }
    const details =
      wire.details && typeof wire.details === 'object'
        ? (wire.details as Record<string, unknown>)
        : undefined;
    const conflictCode = applyConflictCode(details);
    return new HttpError(
      conflictCode ? 409 : wire.http_status ?? statusForCode(wire.code),
      conflictCode ?? wire.code,
      wire.message,
      details,
    );
  }

  const message = error instanceof Error ? error.message : String(error);
  if (message.startsWith('invalid ')) {
    return new HttpError(400, 'invalid_input', message);
  }
  return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
}

/**
 * The typed `details.category` the native producer publishes when the embedded
 * Host authority is not attached.
 *
 * Its public code/status stay `invalid_input`/400 — an unattached authority is
 * truthful degraded readiness, not a client fault — so this category, never the
 * human message, is the stable discriminator for the absent-host state.
 */
export const HOST_NOT_STARTED_CATEGORY = 'host_not_started';

/**
 * Whether a native rejection reports the absent embedded Host authority.
 *
 * Reads the typed envelope (`invalid_input` plus `details.category`): a reworded
 * producer message cannot change the verdict, and every other `invalid_input` —
 * or any `internal` — stays a hard failure instead of falling into an absence
 * fallback. A missing resource is NOT absence and keeps its own deliberate
 * `not_found` handling at each call site. Accepts a raw native rejection or an
 * already mapped `HttpError`.
 */
export function isAbsentHostError(error: unknown): boolean {
  const mapped = mapNativeError(error);
  return mapped.code === 'invalid_input' && mapped.details?.category === HOST_NOT_STARTED_CATEGORY;
}

export function toErrorBody(error: HttpError, requestId: string): ApiErrorBody {
  const message = error.code === 'internal' ? PUBLIC_INTERNAL_MESSAGE : error.message;
  return {
    success: false,
    error: {
      code: error.code,
      message,
      ...(error.details ? { details: error.details } : {}),
      request_id: requestId,
    },
  };
}

export function walkJsonSafeIntegers(value: unknown, path = 'body'): void {
  if (value === null || value === undefined) return;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new HttpError(400, 'invalid_input', `numeric_range: ${path} is not finite`);
    }
    if (Number.isInteger(value) && !Number.isSafeInteger(value)) {
      throw new HttpError(400, 'invalid_input', `numeric_range: ${path} is not a safe integer`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => walkJsonSafeIntegers(entry, `${path}[${index}]`));
    return;
  }
  if (typeof value === 'object') {
    for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
      walkJsonSafeIntegers(entry, `${path}.${key}`);
    }
  }
}

export function stringifyJsonSafe(value: unknown): string {
  walkJsonSafeIntegers(value);
  return JSON.stringify(value, (_key, current) => {
    if (typeof current === 'bigint') {
      throw new HttpError(400, 'invalid_input', 'bigint values are not allowed on the wire');
    }
    return current;
  });
}
