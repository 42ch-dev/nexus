import type { CoreError } from '@42ch/nexus-contracts';

// Compile-time coupled to the generated union: a new CoreError['code'] fails
// typecheck here until it is added.
const CORE_ERROR_CODES: Record<CoreError['code'], true> = {
  uninitialized: true,
  auth_required: true,
  invalid_input: true,
  forbidden: true,
  not_found: true,
  not_supported: true,
  world_kb_conflict: true,
  world_kb_validation: true,
  outline_conflict: true,
  outline_validation_failed: true,
  strategy_candidate_conflict: true,
  strategy_already_approved: true,
  workspace_conflict: true,
  writer_fenced: true,
  owner_busy: true,
  schema_mismatch: true,
  busy: true,
  closing: true,
  interrupted: true,
  policy_blocked: true,
  service_unavailable: true,
  compute_fuel_exhausted: true,
  compute_wall_time_exceeded: true,
  compute_memory_cap_exceeded: true,
  compute_module_trapped: true,
  compute_module_error: true,
  internal: true,
};

function isCoreErrorCode(code: string): code is CoreError['code'] {
  // Own-property check: inherited names (toString, constructor, __proto__, …)
  // must not pass the whitelist.
  return Object.hasOwn(CORE_ERROR_CODES, code);
}

/** An error reply only carries a client-error or server-error status. */
function isValidHttpStatus(status: unknown): status is number {
  return typeof status === 'number' && Number.isInteger(status) && status >= 400 && status <= 599;
}

/**
 * Validate a provider-controlled envelope. Requires an allowlisted code and a
 * string message; an invalid http_status (outside 400-599, non-integer, or
 * non-numeric) is stripped so the mapper falls back to the code's mapped
 * status instead of emitting a bogus or success status.
 */
function validateEnvelope(candidate: unknown): CoreError | null {
  if (candidate === null || typeof candidate !== 'object') return null;
  const envelope = candidate as CoreError;
  if (
    typeof envelope.code !== 'string' ||
    !isCoreErrorCode(envelope.code) ||
    typeof envelope.message !== 'string'
  ) {
    return null;
  }
  if (envelope.http_status === undefined || isValidHttpStatus(envelope.http_status)) {
    return envelope;
  }
  // Rebuild explicitly: object rest copies only own enumerable properties, so
  // an Error-shaped rejection would lose its non-enumerable `message`.
  const sanitized: CoreError = { code: envelope.code, message: envelope.message };
  if (envelope.details !== undefined) sanitized.details = envelope.details;
  return sanitized;
}

/** Parse a native rejection reason into the generated wire `CoreError` envelope. */
export function parseNativeCoreError(error: unknown): CoreError | null {
  // Typed rejections arrive as the envelope object itself, not a JSON string.
  const fromObject = validateEnvelope(error);
  if (fromObject) return fromObject;
  const message = error instanceof Error ? error.message : String(error);
  try {
    return validateEnvelope(JSON.parse(message));
  } catch {
    return null;
  }
}

export function isNativeCoreErrorCode(error: unknown, code: CoreError['code']): boolean {
  const wire = parseNativeCoreError(error);
  return wire?.code === code;
}
