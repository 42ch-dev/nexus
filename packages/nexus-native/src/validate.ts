export const MAX_SAFE_INTEGER = 9007199254740991;

export function assertSafeInteger(value: number | null | undefined, field: string): number | null {
  if (value == null) return null;
  if (!Number.isFinite(value) || !Number.isInteger(value) || value < 0 || value > MAX_SAFE_INTEGER) {
    throw new Error(`invalid ${field}: must be a safe non-negative integer`);
  }
  return value;
}
