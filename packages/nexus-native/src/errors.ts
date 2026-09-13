import type { CoreError } from '@42ch/nexus-contracts';

const CORE_ERROR_CODES = new Set<string>([
  'uninitialized',
  'auth_required',
  'invalid_input',
  'forbidden',
  'not_found',
  'world_kb_conflict',
  'world_kb_validation',
  'writer_fenced',
  'owner_busy',
  'schema_mismatch',
  'busy',
  'closing',
  'interrupted',
  'internal',
]);

/** Parse a native rejection reason into the generated wire `CoreError` envelope. */
export function parseNativeCoreError(error: unknown): CoreError | null {
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
    // not JSON
  }
  return null;
}

export function isNativeCoreErrorCode(error: unknown, code: CoreError['code']): boolean {
  const wire = parseNativeCoreError(error);
  return wire?.code === code;
}
