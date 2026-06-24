/**
 * Client-side error model for the Nexus local Web UI.
 *
 * This is an **app-side** error abstraction — NOT a wire DTO duplicate. The
 * shared Local API `ErrorResponse` schema (residual F-E1, landed by Track B /
 * plan P0 in V1.64) defines the daemon's error wire shape as
 * `{ code, message, details? }`. `BrowserClient` parses the response body
 * defensively into this type; once F-E1's generated `ErrorResponse` type is
 * available on the merged integration branch, the parsing can be tightened to
 * validate against it (see apps/web/AGENTS.md §Pending contracts alignment).
 */
export interface NexusErrorBody {
  /** Stable machine-readable error code (e.g. `not_found`, `validation_failed`). */
  code: string;
  /** Human-readable message, surfaced in toasts. */
  message: string;
  /** Optional structured details. */
  details?: unknown;
}

/**
 * Error thrown by {@link NexusClient} implementations for non-2xx responses or
 * transport failures. Carries the HTTP status and, when the daemon provided a
 * parseable error body, the stable `code` + `message` from the shared
 * ErrorResponse shape.
 */
export class NexusClientError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: unknown;

  constructor(
    status: number,
    code: string,
    message: string,
    details?: unknown,
  ) {
    super(message);
    this.name = 'NexusClientError';
    this.status = status;
    this.code = code;
    this.details = details;
  }

  static fromBody(status: number, body: unknown): NexusClientError {
    const parsed = (body ?? {}) as Partial<NexusErrorBody>;
    return new NexusClientError(
      status,
      parsed.code ?? `http_${status}`,
      parsed.message ?? `Request failed with status ${status}`,
      parsed.details,
    );
  }
}
