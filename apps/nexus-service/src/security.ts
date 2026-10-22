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

export function checkOrigin(
  req: IncomingMessage,
  allowedOrigins: readonly string[],
): AuthFailure | null {
  if (req.method === 'OPTIONS') return null;
  const origin = req.headers.origin;
  if (!origin) return null;
  if (!allowedOrigins.includes(origin)) {
    return {
      ok: false,
      status: 403,
      code: 'forbidden',
      message: `Origin '${origin}' is not allowed for this Daemon API`,
      details: { resource: 'origin' },
    };
  }
  return null;
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
