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

const STATUS_BY_CODE: Record<string, number> = {
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
  route_not_migrated: 501,
  input_too_large: 413,
};

const PUBLIC_INTERNAL_MESSAGE = 'Internal server error';

export function statusForCode(code: string): number {
  return STATUS_BY_CODE[code] ?? 500;
}

export function routeNotMigrated(path: string): HttpError {
  return new HttpError(
    501,
    'route_not_migrated',
    `Route is not migrated in the standalone service profile: ${path}`,
  );
}

function tryParseCoreError(message: string): CoreError | null {
  try {
    const parsed = JSON.parse(message) as CoreError;
    if (parsed && typeof parsed === 'object' && typeof parsed.code === 'string') {
      return parsed;
    }
  } catch {
    // fall through
  }
  return null;
}

function mapDisplayMessage(message: string): HttpError {
  if (message === 'workspace not initialized') {
    return new HttpError(409, 'uninitialized', 'Workspace not initialized');
  }
  if (message === 'authentication required') {
    return new HttpError(401, 'auth_required', 'Authentication required');
  }
  if (message.startsWith('forbidden: ')) {
    return new HttpError(403, 'forbidden', message, { resource: message.slice('forbidden: '.length) });
  }
  if (message.startsWith('not found: ')) {
    return new HttpError(404, 'not_found', message, { resource: message.slice('not found: '.length) });
  }
  const sessionNotFound = /^session (.+) not found$/.exec(message);
  if (sessionNotFound) {
    return new HttpError(404, 'not_found', message, { resource: `session:${sessionNotFound[1]}` });
  }
  const operationInactive = /^operation (.+) is not active$/.exec(message);
  if (operationInactive) {
    return new HttpError(404, 'not_found', message, { resource: `operation:${operationInactive[1]}` });
  }
  const invalid = /^invalid input: ([^—]+) — (.+)$/.exec(message);
  if (invalid) {
    return new HttpError(400, 'invalid_input', message, {
      field: invalid[1].trim(),
      reason: invalid[2].trim(),
    });
  }
  if (message.startsWith('session_id:') || message.startsWith('operation_id:')) {
    return new HttpError(400, 'invalid_input', message);
  }
  if (message.startsWith('invalid ')) {
    return new HttpError(400, 'invalid_input', message);
  }
  if (message === 'world kb conflict') {
    return new HttpError(409, 'world_kb_conflict', 'World KB conflict');
  }
  if (message === 'world kb validation failed') {
    return new HttpError(422, 'world_kb_validation', 'World KB validation failed');
  }
  if (message === 'writer owner busy') {
    return new HttpError(409, 'owner_busy', 'Writer owner busy');
  }
  if (message === 'writer fenced') {
    return new HttpError(409, 'writer_fenced', 'Writer fenced');
  }
  if (message === 'schema mismatch') {
    return new HttpError(409, 'schema_mismatch', 'Schema mismatch');
  }
  if (message === 'busy') {
    return new HttpError(503, 'busy', 'Service busy');
  }
  if (message === 'closing') {
    return new HttpError(503, 'closing', 'Service is closing');
  }
  if (message === 'interrupted') {
    return new HttpError(503, 'interrupted', 'Operation interrupted');
  }
  if (message === 'provider port unavailable') {
    return new HttpError(503, 'busy', 'Provider port unavailable');
  }
  if (message.startsWith('internal: ') || message.startsWith('config_load:') || message.startsWith('database_error:')) {
    return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
  }
  return new HttpError(500, 'internal', PUBLIC_INTERNAL_MESSAGE);
}

export function mapNativeError(error: unknown): HttpError {
  if (error instanceof HttpError) return error;
  const message = error instanceof Error ? error.message : String(error);
  const wire = tryParseCoreError(message);
  if (wire) {
    const status = wire.http_status ?? statusForCode(wire.code);
    const details =
      wire.details && typeof wire.details === 'object'
        ? (wire.details as Record<string, unknown>)
        : undefined;
    const publicMessage = wire.code === 'internal' ? PUBLIC_INTERNAL_MESSAGE : wire.message;
    return new HttpError(status, wire.code, publicMessage, details);
  }
  return mapDisplayMessage(message);
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
