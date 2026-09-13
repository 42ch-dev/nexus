/**
 * The single wire-validation seam for `@42ch/nexus-native`.
 *
 * Message shapes are bound to the generated contract types (`satisfies
 * ShapeOf<T>`), so a contract field added or renamed fails this build instead
 * of silently drifting. Validation never rewrites the caller's value: omission
 * stays omission and `null` stays `null`.
 */
import type {
  CoreChangesRequest,
  CoreHostQuery,
  NativeOpenOptions,
  ProviderCall,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';

export const MAX_SAFE_INTEGER = 9007199254740991;

type FieldKind = 'string' | 'boolean' | 'integer' | 'any';

interface FieldSpec {
  readonly kind: FieldKind;
  readonly min?: number;
  readonly max?: number;
  readonly enum?: readonly string[];
  readonly pattern?: RegExp;
  readonly nullable?: boolean;
}

export interface Shape {
  readonly fields: Readonly<Record<string, FieldSpec>>;
  /** `true` mirrors a schema `additionalProperties: true` payload. */
  readonly open?: boolean;
}

type ShapeOf<T> = { readonly fields: { readonly [K in keyof T]-?: FieldSpec } };

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function checkField(value: unknown, spec: FieldSpec, path: string): void {
  if (value === null) {
    if (spec.nullable) return;
    throw new Error(`invalid ${path}: null is not allowed`);
  }
  switch (spec.kind) {
    case 'string': {
      if (typeof value !== 'string') throw new Error(`invalid ${path}: expected a string`);
      if (spec.pattern && !spec.pattern.test(value)) {
        throw new Error(`invalid ${path}: value does not match the required pattern`);
      }
      if (spec.enum && !spec.enum.includes(value)) {
        throw new Error(`invalid ${path}: value is not one of the allowed values`);
      }
      return;
    }
    case 'boolean': {
      if (typeof value !== 'boolean') throw new Error(`invalid ${path}: expected a boolean`);
      return;
    }
    case 'integer': {
      if (!Number.isSafeInteger(value)) {
        throw new Error(`invalid ${path}: expected an exactly representable integer`);
      }
      const asNumber = value as number;
      if (spec.min !== undefined && asNumber < spec.min) {
        throw new Error(`invalid ${path}: below the minimum of ${spec.min}`);
      }
      if (spec.max !== undefined && asNumber > spec.max) {
        throw new Error(`invalid ${path}: above the maximum of ${spec.max}`);
      }
      return;
    }
    default:
      return;
  }
}

/** Reject unknown/malformed fields; `undefined` keeps the schema's omission semantics. */
export function assertShape(value: unknown, shape: Shape, path: string): void {
  if (!isPlainObject(value)) throw new Error(`invalid ${path}: expected an object`);
  for (const key of Object.keys(value)) {
    const spec = shape.fields[key];
    if (!spec) {
      if (!shape.open) throw new Error(`invalid ${path}.${key}: unknown field`);
      continue;
    }
    const field = value[key];
    if (field === undefined) continue;
    checkField(field, spec, `${path}.${key}`);
  }
}

/** Numeric policy: no integer-valued Number outside the exactly representable range. */
export function assertSafeNumbers(value: unknown, path: string): void {
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new Error(`invalid ${path}: non-finite number`);
    if (Number.isInteger(value) && !Number.isSafeInteger(value)) {
      throw new Error(`invalid ${path}: integer outside the exactly representable range`);
    }
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => assertSafeNumbers(entry, `${path}[${index}]`));
    return;
  }
  if (isPlainObject(value)) {
    for (const [key, entry] of Object.entries(value)) {
      assertSafeNumbers(entry, `${path}.${key}`);
    }
  }
}

export const NATIVE_OPEN_OPTIONS_SHAPE = {
  fields: {
    user_home: { kind: 'string' },
    access: { kind: 'string', enum: ['read_only', 'direct_writer', 'engine_owner'] },
    allow_uninitialized: { kind: 'boolean' },
  },
} satisfies ShapeOf<NativeOpenOptions>;

export const CORE_CHANGES_REQUEST_SHAPE = {
  fields: {
    after_sequence: { kind: 'string', pattern: /^[0-9]+$/ },
    limit: { kind: 'integer', min: 1, max: 256 },
  },
} satisfies ShapeOf<CoreChangesRequest>;

export const CORE_HOST_QUERY_SHAPE = {
  fields: {
    query: {
      kind: 'string',
      enum: ['health', 'catalog', 'list_sessions', 'get_session', 'get_operation'],
    },
    session_id: { kind: 'string' },
    operation_id: { kind: 'string' },
    limit: { kind: 'integer', min: 1 },
    cursor: { kind: 'string' },
    format: { kind: 'string', enum: ['catalog', 'scan'] },
  },
} satisfies ShapeOf<CoreHostQuery>;

export const PROVIDER_CALL_SHAPE = {
  fields: {
    method: { kind: 'string', enum: ['probe', 'launch', 'execute', 'cancel', 'shutdown'] },
    request_id: { kind: 'string' },
    session_id: { kind: 'string', nullable: true },
    operation_id: { kind: 'string', nullable: true },
    deadline_ms: { kind: 'integer', min: 0 },
    payload: { kind: 'any' },
  },
} satisfies ShapeOf<ProviderCall>;

export const WORLD_KB_PATCH_ENTITY_SHAPE = {
  fields: {
    entity_id: { kind: 'string' },
    expected_version: { kind: 'integer', min: 0 },
    patch: { kind: 'any' },
  },
} satisfies ShapeOf<WorldKbPatchEntityRequest>;

/** Validate then serialize — the one path used for every value crossing the wire. */
export function stringifyWire(value: unknown, shape?: Shape, label = 'payload'): string {
  if (shape) assertShape(value, shape, label);
  assertSafeNumbers(value, label);
  return JSON.stringify(value);
}

export function encodeWireBuffer(value: unknown, shape?: Shape, label = 'payload'): Uint8Array {
  return new TextEncoder().encode(stringifyWire(value, shape, label));
}

export function assertSafeInteger(value: number | null | undefined, field: string): number | null {
  if (value == null) return null;
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`invalid ${field}: expected a non-negative exactly representable integer`);
  }
  return value;
}
