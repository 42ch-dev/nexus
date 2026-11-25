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
  return code in CORE_ERROR_CODES;
}

/** A client-supplied status is only honored when it is a valid HTTP status. */
function isValidHttpStatus(status: unknown): status is number {
  return typeof status === 'number' && Number.isInteger(status) && status >= 100 && status <= 599;
}

/** Parse a native rejection reason into the generated wire `CoreError` envelope. */
export function parseNativeCoreError(error: unknown): CoreError | null {
  // Typed rejections arrive as the envelope object itself, not a JSON string.
  // The object is provider-controlled, so validate the envelope fields rather
  // than trusting an allowlisted `code` alone.
  if (error !== null && typeof error === 'object') {
    const candidate = error as CoreError;
    if (
      typeof candidate.code === 'string' &&
      isCoreErrorCode(candidate.code) &&
      typeof candidate.message === 'string'
    ) {
      if (candidate.http_status === undefined || isValidHttpStatus(candidate.http_status)) {
        return candidate;
      }
      // An out-of-range/non-integer http_status would be rejected by Node when
      // assigned to res.statusCode; ignore it so the mapper falls back to the
      // code's mapped status instead of re-masking the error as 500.
      const { http_status: _ignored, ...envelope } = candidate;
      return envelope;
    }
  }
  const message = error instanceof Error ? error.message : String(error);
  try {
    const parsed = JSON.parse(message) as CoreError;
    if (
      parsed &&
      typeof parsed === 'object' &&
      typeof parsed.code === 'string' &&
      isCoreErrorCode(parsed.code)
    ) {
      return parsed;
    }
  } catch {
  }
  return null;
}

export function isNativeCoreErrorCode(error: unknown, code: CoreError['code']): boolean {
  const wire = parseNativeCoreError(error);
  return wire?.code === code;
}
