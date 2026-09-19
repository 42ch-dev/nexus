/**
 * Exact-origin network auth policy (v1.192 P0-T5).
 *
 * Main session hooks inject the stored `X-API-Key` ONLY for the selected
 * top-level webContents' fetch/XHR to the exact saved active endpoint
 * origin AND the `/v1/daemon/` path prefix. The policy is pure and
 * host-agnostic; `attachDesktopNetworkHooks` is the thin Electron
 * `webRequest` wiring around it.
 *
 * Guarantees (parity row 18):
 * - renderer-supplied auth headers are stripped first;
 * - never injected on foreign hosts, redirects that leave the pinned
 *   origin, fingerprint probes, images, subresources or an inactive config;
 * - inactive endpoint ⇒ no auth anywhere.
 */

/** Auth authority for the active connection; null when inactive/keyless. */
export interface ActiveConnectionAuth {
  /** Exact pinned origin (scheme://host:port). */
  endpointOrigin: string;
  apiKey: string;
}

/** Minimal shape of a Chromium webRequest details object the policy needs. */
export interface RequestDetailsLike {
  url: string;
  resourceType: string;
  /** Presence of renderer-set auth on the outgoing request. */
  requestHeaders?: Record<string, string>;
}

export const AUTH_HEADER = 'x-api-key';
/** Auth is injected only for daemon API calls under this path prefix. */
export const AUTH_PATH_PREFIX = '/v1/daemon/';
/** Only fetch/XHR may carry the credential; never images, media, etc. */
const AUTH_RESOURCE_TYPES: Record<string, true> = { xhr: true, fetch: true };

export interface AuthDecision {
  /** The credential to inject, or null to send no auth. */
  inject: string | null;
}

/**
 * Exact-origin + path-prefix + resource-type decision. A request is
 * authenticated only when ALL of:
 *  - an active auth exists (inactive/keyless config ⇒ no injection);
 *  - the URL's exact origin equals the pinned endpoint origin;
 *  - the path starts with the daemon prefix;
 *  - the resource type is fetch/xhr.
 *
 * Redirects are decided per request: a redirect target that left the
 * pinned origin fails the origin check here, so the credential is dropped.
 */
export function decideAuthInjection(
  auth: ActiveConnectionAuth | null,
  details: RequestDetailsLike,
): AuthDecision {
  if (!auth) return { inject: null };
  if (!Object.hasOwn(AUTH_RESOURCE_TYPES, details.resourceType)) return { inject: null };
  let origin: string;
  let path: string;
  try {
    const url = new URL(details.url);
    origin = url.origin;
    path = url.pathname;
  } catch {
    return { inject: null };
  }
  if (origin !== auth.endpointOrigin) return { inject: null };
  if (!path.startsWith(AUTH_PATH_PREFIX)) return { inject: null };
  return { inject: auth.apiKey };
}

/**
 * Compute the outgoing request-headers map for a request, in Electron's
 * `webRequest` header-map shape (`Record<string, string>`): any
 * renderer-supplied `X-API-Key` is stripped first, then the main-owned
 * credential is set only when the policy decision allows it. Header names
 * are compared case-insensitively (HTTP tokens).
 */
export function applyAuthHeaders(
  auth: ActiveConnectionAuth | null,
  details: RequestDetailsLike,
): Record<string, string> {
  const headers: Record<string, string> = {};
  for (const [name, value] of Object.entries(details.requestHeaders ?? {})) {
    if (name.toLowerCase() !== AUTH_HEADER) headers[name] = value;
  }
  const { inject } = decideAuthInjection(auth, details);
  if (inject !== null) headers['X-API-Key'] = inject;
  return headers;
}

/**
 * Session-shaped dependency surface for Electron's `webRequest` hooks
 * (kept structural so tests can drive the real hook wiring with a fake
 * session and observe actual header isolation).
 */
export interface SessionLike {
  webRequest: {
    onBeforeSendHeaders(
      filter: { urls: string[] },
      listener: (details: RequestDetailsLike & { requestHeaders?: Record<string, string> }, callback: (response: { requestHeaders?: Record<string, string> }) => void) => void,
    ): void;
    onHeadersReceived?(
      filter: { urls: string[] },
      listener: (details: { url: string; statusLine?: string; responseHeaders?: Record<string, string[]> }, callback: (response: { cancel?: boolean; responseHeaders?: Record<string, string[]> }) => void) => void,
    ): void;
  };
}

/**
 * Wire the auth policy into a main session. `getActiveAuth` is re-read per
 * request so a connection change takes effect before any new request is
 * allowed out. Renderer auth headers are always stripped; injection follows
 * the exact-origin decision above.
 */
export function attachDesktopNetworkHooks(
  session: SessionLike,
  getActiveAuth: () => ActiveConnectionAuth | null,
): void {
  session.webRequest.onBeforeSendHeaders({ urls: ['http://*/*', 'https://*/*'] }, (details, callback) => {
    callback({ requestHeaders: applyAuthHeaders(getActiveAuth(), details) });
  });
}
