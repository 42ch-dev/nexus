/**
 * The single wire-validation seam for `@42ch/nexus-native`.
 *
 * Message shapes are bound to the generated contract types (`satisfies
 * ShapeOf<T>`), so a contract field added or renamed fails this build instead
 * of silently drifting. Validation never rewrites the caller's value: omission
 * stays omission (unless the schema marks the property required) and `null`
 * stays `null` where the schema allows it.
 */
import type {
  CoreChangesRequest,
  CoreHostQuery,
  NativeOpenOptions,
  ProviderCall,
  WorldKbPatchEntityRequest,
} from '@42ch/nexus-contracts';

export const MAX_SAFE_INTEGER = 9007199254740991;

/** Fixed N-API contract value declared by `native-compatibility.schema.json`. */
export const REQUIRED_NAPI_MINIMUM = 8;

type FieldKind = 'string' | 'boolean' | 'integer' | 'object' | 'array';

interface FieldSpec {
  readonly kind: FieldKind;
  readonly min?: number;
  readonly max?: number;
  readonly enum?: readonly string[];
  readonly pattern?: RegExp;
  readonly minLength?: number;
  readonly maxLength?: number;
  readonly nullable?: boolean;
  /** Nested object schema (`kind: 'object'` only). */
  readonly shape?: Shape;
  /** Item spec for `kind: 'array'`. */
  readonly items?: FieldSpec;
}

export interface Shape {
  readonly fields: Readonly<Record<string, FieldSpec>>;
  /** JSON Schema `required`: every listed property must be present. */
  readonly required: readonly string[];
  /** `additionalProperties: false` when omitted. */
  readonly open?: boolean;
  /** JSON Schema `minProperties` on this object. */
  readonly minProperties?: number;
}

/**
 * Binds a shape to a generated contract type: every key of `T` needs a spec
 * (add/rename drift fails this build) and `required` must name only keys of `T`.
 */
type ShapeOf<T> = Omit<Shape, 'fields' | 'required'> & {
  readonly fields: { readonly [K in keyof T]-?: FieldSpec };
  readonly required: readonly (keyof T & string)[];
};

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
      if (spec.minLength !== undefined && value.length < spec.minLength) {
        throw new Error(`invalid ${path}: shorter than the minimum length of ${spec.minLength}`);
      }
      if (spec.maxLength !== undefined && value.length > spec.maxLength) {
        throw new Error(`invalid ${path}: longer than the maximum length of ${spec.maxLength}`);
      }
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
    case 'object': {
      if (!isPlainObject(value)) throw new Error(`invalid ${path}: expected an object`);
      if (spec.shape) assertShape(value, spec.shape, path);
      return;
    }
    case 'array': {
      if (!Array.isArray(value)) throw new Error(`invalid ${path}: expected an array`);
      if (spec.items) {
        value.forEach((entry, index) => checkField(entry, spec.items as FieldSpec, `${path}[${index}]`));
      }
      return;
    }
    default:
      throw new Error(`invalid ${path}: unsupported field kind`);
  }
}

/** Enforce required fields, reject unknown/malformed fields, recurse into nested shapes. */
export function assertShape(value: unknown, shape: Shape, path: string): void {
  if (!isPlainObject(value)) throw new Error(`invalid ${path}: expected an object`);
  const present = Object.keys(value).filter((key) => value[key] !== undefined);
  if (shape.minProperties !== undefined && present.length < shape.minProperties) {
    throw new Error(`invalid ${path}: at least ${shape.minProperties} property is required`);
  }
  for (const key of shape.required) {
    if (value[key] === undefined) {
      throw new Error(`invalid ${path}.${key}: required field is missing`);
    }
  }
  for (const key of present) {
    const spec = shape.fields[key];
    if (!spec) {
      if (!shape.open) throw new Error(`invalid ${path}.${key}: unknown field`);
      continue;
    }
    checkField(value[key], spec, `${path}.${key}`);
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
  required: ['user_home', 'access'],
} satisfies ShapeOf<NativeOpenOptions>;

export const CORE_CHANGES_REQUEST_SHAPE = {
  fields: {
    after_sequence: { kind: 'string', pattern: /^[0-9]+$/ },
    limit: { kind: 'integer', min: 1, max: 256 },
  },
  required: ['after_sequence'],
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
  required: ['query'],
} satisfies ShapeOf<CoreHostQuery>;

/**
 * `payload` is the method-specific request body. `provider-call.schema.json`
 * constrains it to an object; the per-method bodies are the existing
 * `daemon-api/agent-host` schemas, which the Rust port parses.
 */
export const PROVIDER_CALL_SHAPE = {
  fields: {
    method: { kind: 'string', enum: ['probe', 'launch', 'execute', 'cancel', 'shutdown'] },
    request_id: { kind: 'string' },
    session_id: { kind: 'string', nullable: true },
    operation_id: { kind: 'string', nullable: true },
    deadline_ms: { kind: 'integer', min: 0 },
    payload: { kind: 'object' },
  },
  required: ['method', 'request_id', 'deadline_ms', 'payload'],
} satisfies ShapeOf<ProviderCall>;

/** `world-kb-entity-patch.schema.json` — `additionalProperties: false`, at least one field. */
export const WORLD_KB_ENTITY_PATCH_SHAPE: Shape = {
  fields: {
    title: { kind: 'string', minLength: 1, maxLength: 200 },
    body: { kind: 'object' },
    aliases: { kind: 'array', items: { kind: 'string' } },
    block_type: {
      kind: 'string',
      enum: [
        'character',
        'ability',
        'scene',
        'organization',
        'item',
        'conflict',
        'info_point',
        'event',
        'species',
        'faction',
        'magic_system',
        'technology',
        'deity',
        'level',
        'economy_tier',
        'dialogue',
        'beat',
        'act',
        'era',
      ],
    },
    modules: { kind: 'object' },
  },
  required: [],
  minProperties: 1,
};

export const WORLD_KB_PATCH_ENTITY_SHAPE = {
  fields: {
    entity_id: { kind: 'string' },
    expected_version: { kind: 'integer', min: 0 },
    patch: { kind: 'object', shape: WORLD_KB_ENTITY_PATCH_SHAPE },
  },
  required: ['entity_id', 'expected_version', 'patch'],
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
