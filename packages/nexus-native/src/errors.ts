import type { CoreError } from '@42ch/nexus-contracts';

const CORE_ERROR_CODES = new Set<string>([
  'uninitialized',
  'auth_required',
  'invalid_input',
  'forbidden',
  'not_found',
  'not_supported',
  'world_kb_conflict',
  'world_kb_validation',
  'outline_conflict',
  'outline_validation_failed',
  'strategy_candidate_conflict',
  'strategy_already_approved',
  'workspace_conflict',
  'writer_fenced',
  'owner_busy',
  'schema_mismatch',
  'busy',
  'closing',
  'interrupted',
  'policy_blocked',
  'service_unavailable',
  'compute_fuel_exhausted',
  'compute_wall_time_exceeded',
  'compute_memory_cap_exceeded',
  'compute_module_trapped',
  'compute_module_error',
  'internal',
]);

/** Parse a native rejection reason into the generated wire `CoreError` envelope. */
export function parseNativeCoreError(error: unknown): CoreError | null {
  // Typed rejections arrive as the envelope object itself, not a JSON string.
  if (
    error !== null &&
    typeof error === 'object' &&
    typeof (error as CoreError).code === 'string' &&
    CORE_ERROR_CODES.has((error as CoreError).code)
  ) {
    return error as CoreError;
  }
  const message = error instanceof Error ? error.message : String(error);
  try {
    const parsed = JSON.parse(message) as CoreError;
    if (
      parsed &&
      typeof parsed === 'object' &&
      typeof parsed.code === 'string' &&
      CORE_ERROR_CODES.has(parsed.code)
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
