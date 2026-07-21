/**
 * Client-side error model for the Nexus local Web UI.
 *
 * This is an **app-side** error abstraction — NOT a wire DTO duplicate. The
 * shared Daemon API `ErrorResponse` schema (landed by Track B / plan P0 in
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
 * Transport-failure sub-classification (V1.129 P0 — profile-create-reliability).
 *
 * `BrowserClient.request()` inspects the `fetch` throw + response and tags the
 * thrown {@link NexusClientError} with the matching kind so the dialog (and the
 * P1 app-wide sweep) can branch honestly per kind. The classifier is best-effort:
 * the browser deliberately hides precise certificate / network reasons from JS,
 * so `tls` falls open to `network` rather than over-claiming a cert rejection.
 *
 * The instance-field shape (vs a discriminated union of subclasses) was locked
 * by Seat 2 — see `.mstar/iterations/v1.129/specs/profile-create-reliability.md`
 * § Interfaces.
 *
 * Ordering of classification is also locked (spec § Classification algorithm):
 * `daemon_down` → `http_fallback` → `tls` (best-effort) → `timeout` → `network` → `unknown`.
 */
export type TransportErrorKind =
  /** TCP refuse, DNS failure, offline, or undifferentiated fetch throw. */
  | 'network'
  /**
   * Certificate failure (best-effort). Browsers hide the precise reason — when
   * the throw carries no detectable TLS signal, the classifier falls back to
   * `network` rather than over-claiming cert rejection.
   */
  | 'tls'
  /** Explicit abort / timeout (AbortError, AbortSignal.timeout). */
  | 'timeout'
  /**
   * Daemon returned `text/html` with HTTP 200 — release-mode SPA fallback for
   * an unrouted path. Treated as transport-class because the request never
   * reached the API layer.
   */
  | 'http_fallback'
  /** Local-mode daemon is not running (browser-tab client with empty `baseUrl`). */
  | 'daemon_down'
  /** No classifier matched; recovery copy stays generic. */
  | 'unknown';

/**
 * Error thrown by {@link NexusClient} implementations for non-2xx responses or
 * transport failures. Carries the HTTP status and, when the daemon provided a
 * parseable error body, the stable `code` + `message` from the shared
 * ErrorResponse shape.
 *
 * For transport-class failures (`status === 0`), `kind` carries the
 * {@link TransportErrorKind} sub-classification so callers can surface an
 * honest per-kind message + CTA instead of the legacy multi-cause blob.
 */
export class NexusClientError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: unknown;
  /**
   * Transport-failure sub-classification. Present iff `status === 0`
   * (transport unreachable). HTTP errors (`status >= 400`) leave this
   * `undefined`; their recovery UX is driven by `code` / `details` instead.
   */
  readonly kind?: TransportErrorKind;

  constructor(
    status: number,
    code: string,
    message: string,
    details?: unknown,
    kind?: TransportErrorKind,
  ) {
    super(message);
    this.name = 'NexusClientError';
    this.status = status;
    this.code = code;
    this.details = details;
    this.kind = kind;
  }

  /**
   * True when the daemon orchestration engine is not running (503 /
   * `service_unavailable` / message contains "engine not available").
   * List surfaces map this to honest unavailable/empty UX — not the generic
   * `common.error.title` crash framing.
   */
  static isOrchestrationEngineUnavailable(error: unknown): boolean {
    return (
      error instanceof NexusClientError
      && error.status === 503
      && (error.code === 'service_unavailable'
        || error.message.toLowerCase().includes('engine not available'))
    );
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

/** @see NexusClientError.isOrchestrationEngineUnavailable */
export function isOrchestrationEngineUnavailable(error: unknown): boolean {
  return NexusClientError.isOrchestrationEngineUnavailable(error);
}
