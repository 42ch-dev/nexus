/**
 * Client-side error model for the Nexus local Web UI.
 *
 * This is an **app-side** error abstraction — NOT a wire DTO duplicate. The
 * shared Local API `ErrorResponse` schema (landed by Track B / plan P0 in
 * V1.64, residual F-E1) models the daemon's **inner** error detail as
 * `{ code, message, details? }`. The daemon runtime **wraps** that detail in
 * an envelope `{ success: false, error: ErrorResponse }` (see
 * `ApiErrorResponse` in `crates/nexus-daemon-runtime/src/api/errors.rs`).
 * `BrowserClient.fromBody` unwraps `body.error` defensively so the stable
 * `code` + actionable `message` reach the UI instead of a generic
 * `http_<status>` fallback.
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
    // The daemon runtime wraps the canonical ErrorResponse under `error`:
    //   { success: false, error: { code, message, details?, request_id? } }
    // (see `ApiErrorResponse` in
    // crates/nexus-daemon-runtime/src/api/errors.rs). Some orchestration
    // handlers still emit ad-hoc (StatusCode, String) bodies
    // (R-V164-FE1-ORCH deferral); the top-level fallback keeps those working.
    const parsed = (body ?? {}) as Partial<NexusErrorBody>;
    const inner =
      (parsed as { error?: Partial<NexusErrorBody> }).error ?? parsed;
    return new NexusClientError(
      status,
      inner.code ?? parsed.code ?? `http_${status}`,
      inner.message ?? parsed.message ?? `Request failed with status ${status}`,
      inner.details ?? parsed.details,
    );
  }
}
