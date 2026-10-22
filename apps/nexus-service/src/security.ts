import { timingSafeEqual } from 'node:crypto';
import type { IncomingMessage } from 'node:http';
import { isLoopbackBindHost } from './bind.js';
import type { AuthSessionStore } from './auth-session.js';
import type { ResolvedServiceConfig } from './config.js';

export type AuthMode = 'keyed_all' | 'keyless_localhost';

export interface AuthDecision {
  ok: true;
  scheme: 'api_key' | 'loopback_bypass' | 'session_cookie';
}

export interface AuthFailure {
  ok: false;
  status: number;
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

export type AuthResult = AuthDecision | AuthFailure;

export function authMode(config: ResolvedServiceConfig): AuthMode {
  return config.apiKey ? 'keyed_all' : 'keyless_localhost';
}

export function constantTimeEqual(a: string, b: string): boolean {
  const left = Buffer.from(a);
  const right = Buffer.from(b);
  if (left.length !== right.length) return false;
  return timingSafeEqual(left, right);
}

export function isLoopbackAddress(remoteAddress: string | undefined): boolean {
  if (!remoteAddress) return true;
  const normalized = remoteAddress.replace(/^::ffff:/, '');
  return isLoopbackBindHost(normalized);
}

/**
 * `authenticated` states that the request already proved access (API key,
 * session cookie or one-time bootstrap token). Only then does a non-loopback
 * request skip the TLS requirement — plain-HTTP LAN dev (access spec §3.1).
 */
export function validateBindPolicy(
  config: ResolvedServiceConfig,
  remoteAddress: string | undefined,
  authenticated = false,
): AuthFailure | null {
  const loopback = isLoopbackAddress(remoteAddress);
  if (!loopback && !config.allowRemote) {
    return {
      ok: false,
      status: 403,
      code: 'forbidden',
      message: 'non-loopback connections require --allow-remote',
    };
  }
  if (!loopback && (!config.tlsCert || !config.tlsKey) && !authenticated) {
    return {
      ok: false,
      status: 403,
      code: 'forbidden',
      message: 'remote access requires --tls-cert and --tls-key or an authenticated request',
    };
  }
  return null;
}

/**
 * Origin admission for browser requests.
 *
 * The allowlist covers the desktop shell and the Vite dev origins. The service
 * also serves the SPA itself (`--web-dist`), and that document's requests are
 * same-origin: their `Origin` is exactly the Host the browser connected to — a
 * LAN address the allowlist cannot know ahead of time. Such a request is
 * admitted; every other origin is still refused.
 */
export function checkOrigin(
  req: IncomingMessage,
  allowedOrigins: readonly string[],
  protocol: 'http' | 'https',
): AuthFailure | null {
  if (req.method === 'OPTIONS') return null;
  const origin = req.headers.origin;
  if (!origin) return null;
  if (allowedOrigins.includes(origin)) return null;
  if (isSameOrigin(origin, req.headers.host, protocol)) return null;
  return {
    ok: false,
    status: 403,
    code: 'forbidden',
    message: `Origin '${origin}' is not allowed for this Daemon API`,
    details: { resource: 'origin' },
  };
}

/** Whether `origin` is exactly the served document's own origin (`protocol://Host`). */
function isSameOrigin(origin: string, host: string | undefined, protocol: string): boolean {
  if (!host) return false;
  let parsed: URL;
  try {
    parsed = new URL(origin);
  } catch {
    // Opaque origins (`null`, a non-URL value) are never same-origin.
    return false;
  }
  return parsed.protocol === `${protocol}:` && parsed.host.toLowerCase() === host.toLowerCase();
}

/**
 * Keyed admission order (access spec §3.1): a present `X-API-Key` is validated
 * exactly as before and wins over the cookie; with no header, a valid
 * `nexus_session` cookie admits. Keyless loopback mode is unchanged.
 */
export function checkApiKey(
  req: IncomingMessage,
  config: ResolvedServiceConfig,
  remoteAddress: string | undefined,
  sessions?: AuthSessionStore,
): AuthResult {
  if (authMode(config) === 'keyed_all') {
    const provided = req.headers['x-api-key'];
    const key = Array.isArray(provided) ? provided[0] : provided;
    if (key && key.length > 0) {
      if (!constantTimeEqual(key, config.apiKey!)) {
        return {
          ok: false,
          status: 401,
          code: 'auth_required',
          message: 'Authentication required',
        };
      }
      return validateBindPolicy(config, remoteAddress, true) ?? { ok: true, scheme: 'api_key' };
    }
    if (sessions?.hasValidSession(req)) {
      return (
        validateBindPolicy(config, remoteAddress, true) ?? { ok: true, scheme: 'session_cookie' }
      );
    }
    const bindFailure = validateBindPolicy(config, remoteAddress);
    if (bindFailure) return bindFailure;
    return {
      ok: false,
      status: 401,
      code: 'auth_required',
      message: 'Authentication required',
    };
  }

  const bindFailure = validateBindPolicy(config, remoteAddress);
  if (bindFailure) return bindFailure;

  if (!isLoopbackAddress(remoteAddress)) {
    return {
      ok: false,
      status: 403,
      code: 'forbidden',
      message: 'non-loopback connections require an API key',
      details: { resource: 'daemon-api' },
    };
  }
  return { ok: true, scheme: 'loopback_bypass' };
}

export function corsAllowMethods(): string {
  return 'GET, POST, PUT, PATCH, DELETE, OPTIONS';
}

export function corsAllowHeaders(): string {
  return 'Content-Type, X-API-Key, X-Request-Id, Authorization';
}
