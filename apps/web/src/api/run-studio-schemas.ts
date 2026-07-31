/**
 * Structural guards narrowing the generated `ModuleDetail.schemas.*` open
 * index signatures (V1.147 P1, qc1 W-003 fix) onto the `@42ch/nexus-ui`
 * primitive contracts.
 *
 * The generated `ModuleDetail` manifest is the wire SSOT — these helpers are
 * typed-guard functions on the wire shape, not a second source of truth. If a
 * future manifest bump changes the schema fragment shape, the guard rejects
 * it (`null`) instead of silently rendering an empty or wrong form.
 */
import type { InvocationSchema } from '@42ch/nexus-ui';

/**
 * Narrow a raw `ModuleDetail.schemas.invocation` value to the structural
 * `InvocationSchema` the Run form primitive consumes.
 *
 * Accepts a plain object whose `properties` is a plain object (when present)
 * and whose `required` is an array of strings (when present); `type` must be
 * `"object"` or absent. Anything else returns `null` (caller renders the
 * "Can't run this module" state).
 */
export function toInvocationSchema(raw: unknown): InvocationSchema | null {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return null;
  }
  const record = raw as Record<string, unknown>;

  const type = record.type;
  if (type !== undefined && type !== 'object') {
    return null;
  }

  const properties = record.properties;
  if (
    properties !== undefined &&
    (typeof properties !== 'object' || properties === null || Array.isArray(properties))
  ) {
    return null;
  }

  const required = record.required;
  if (
    required !== undefined &&
    (!Array.isArray(required) || required.some((entry) => typeof entry !== 'string'))
  ) {
    return null;
  }

  // Narrowed copy: only the structural fields the primitive reads survive.
  return {
    ...(type !== undefined ? { type } : {}),
    ...(properties !== undefined ? { properties: properties as InvocationSchema['properties'] } : {}),
    ...(required !== undefined ? { required: required as string[] } : {}),
  };
}
