import { randomBytes } from 'node:crypto';
import type { IncomingMessage } from 'node:http';

/**
 * Browser access bootstrap (access spec §3.1 / compass D20).
 *
 * The service mints one single-use token per boot and prints it inside the
 * `GET /__bootstrap?token=` URL. Consuming it hands the browser a bounded
 * `HttpOnly` session cookie, so later API calls need no `X-API-Key`.
 *
 * Everything here is per-boot, in-memory state: revocation is a service
 * restart, and no token or session ever reaches disk.
 */
export const TOKEN_TTL_MS = 300_000;
export const SESSION_TTL_MS = 28_800_000;
export const SESSION_COOKIE = 'nexus_session';

/** 256-bit base64url identifier (tokens and session ids alike). */
function randomId(): string {
  return randomBytes(32).toString('base64url');
}

function readCookie(header: string | undefined, name: string): string | null {
  if (!header) return null;
  for (const part of header.split(';')) {
    const trimmed = part.trim();
    const separator = trimmed.indexOf('=');
    if (separator === -1) continue;
    if (trimmed.slice(0, separator) === name) return trimmed.slice(separator + 1);
  }
  return null;
}

/** `Set-Cookie` value for a freshly issued session. */
export function sessionCookie(sessionId: string, secure: boolean): string {
  const maxAgeSeconds = SESSION_TTL_MS / 1000;
  return `${SESSION_COOKIE}=${sessionId}; HttpOnly; SameSite=Lax; Path=/; Max-Age=${maxAgeSeconds}${
    secure ? '; Secure' : ''
  }`;
}

export class AuthSessionStore {
  /** Unconsumed tokens, mapped to their expiry instant. */
  private readonly tokens = new Map<string, number>();
  /** Consumed tokens, retained until their own expiry so replay is reported as replay. */
  private readonly consumedTokens = new Map<string, number>();
  private readonly sessions = new Map<string, number>();

  constructor(private readonly now: () => number = Date.now) {}

  mintToken(): string {
    this.sweep();
    const token = randomId();
    this.tokens.set(token, this.now() + TOKEN_TTL_MS);
    return token;
  }

  /** Validates and deletes the token; a second use of the same token returns false. */
  consumeToken(token: string): boolean {
    this.sweep();
    const expiresAt = this.tokens.get(token);
    if (expiresAt === undefined || expiresAt <= this.now()) return false;
    this.tokens.delete(token);
    this.consumedTokens.set(token, expiresAt);
    return true;
  }

  /** True when the token was already consumed and has not yet expired. */
  wasConsumed(token: string): boolean {
    const expiresAt = this.consumedTokens.get(token);
    return expiresAt !== undefined && expiresAt > this.now();
  }

  issueSession(): string {
    this.sweep();
    const sessionId = randomId();
    this.sessions.set(sessionId, this.now() + SESSION_TTL_MS);
    return sessionId;
  }

  hasValidSession(req: IncomingMessage): boolean {
    const sessionId = readCookie(req.headers.cookie, SESSION_COOKIE);
    if (!sessionId) return false;
    const expiresAt = this.sessions.get(sessionId);
    if (expiresAt === undefined) return false;
    if (expiresAt <= this.now()) {
      this.sessions.delete(sessionId);
      return false;
    }
    return true;
  }

  /** Lazy expiry sweep: bounded maps, no timer. */
  private sweep(): void {
    const now = this.now();
    for (const [token, expiresAt] of this.tokens) {
      if (expiresAt <= now) this.tokens.delete(token);
    }
    for (const [token, expiresAt] of this.consumedTokens) {
      if (expiresAt <= now) this.consumedTokens.delete(token);
    }
    for (const [sessionId, expiresAt] of this.sessions) {
      if (expiresAt <= now) this.sessions.delete(sessionId);
    }
  }
}
