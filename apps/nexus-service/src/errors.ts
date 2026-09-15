import { parseNativeCoreError } from '@42ch/nexus-native';
import type { CoreError } from '@42ch/nexus-contracts';

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
  world_kb_conflict: 409,
  world_kb_validation: 422,
  writer_fenced: 409,
  owner_busy: 409,
  schema_mismatch: 409,
  busy: 503,
  closing: 503,
  interrupted: 503,
  internal: 500,
};

const PUBLIC_INTERNAL_MESSAGE = 'Internal server error';

export function statusForCode(code: string): number {
  if (code in STATUS_BY_CODE) {
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
    // `internal` is the sanitization boundary: the native detail (and any
    // path/SQL/config it carries in `details`) never reaches the client.
    if (wire.code === 'internal') {
      return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
    }
    const details =
      wire.details && typeof wire.details === 'object'
        ? (wire.details as Record<string, unknown>)
        : undefined;
    return new HttpError(
      wire.http_status ?? statusForCode(wire.code),
      wire.code,
      wire.message,
      details,
    );
  }

  const message = error instanceof Error ? error.message : String(error);
  // `@42ch/nexus-native` validates every value before it crosses the wire and
  // rejects with `invalid <field>: <reason>` — a client error, not a fault.
  if (message.startsWith('invalid ')) {
    return new HttpError(400, 'invalid_input', message);
  }
  return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
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
