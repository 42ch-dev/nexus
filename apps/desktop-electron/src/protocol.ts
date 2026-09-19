/**
 * Protocol + navigation policy for the Electron desktop shell (v1.192 P0-T1).
/**
 * The production desktop scheme `nexus:` is the one host scheme: standard,
 * secure, fetch, streaming; packaged resources only under the canonical
 * dist root; exact-origin CSP with main-validated origins only. The retired
 * proof scheme and its open-policy helpers were deleted with the proof
 * shell's runtime callsites in P0-T7.
 *
 * The `electron` value import is dynamic (see desktop-ipc.ts for the same
 * rationale): the pure path/origin/CSP policy below must stay importable in
 * plain node tests.
 */
import { createReadStream, existsSync, lstatSync, realpathSync, statSync } from 'node:fs';
import { join, normalize, sep } from 'node:path';
import { Readable } from 'node:stream';
import { DESKTOP_HOST, DESKTOP_SCHEME, isDesktopAppOrigin } from './desktop-contract.js';

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.ttf': 'font/ttf',
  '.ico': 'image/x-icon',
  '.map': 'application/json; charset=utf-8',
};

function isUnderRoot(candidate: string, root: string): boolean {
  return candidate === root || candidate.startsWith(root + sep);
}

export function resolveDistRoot(repoRoot: string): string {
  return join(repoRoot, 'apps', 'web', 'dist');
}

export function assertDistPresent(distRoot: string): void {
  const indexPath = join(distRoot, 'index.html');
  if (!existsSync(indexPath)) {
    throw new Error(
      `missing web artifact ${indexPath}. Build the unchanged apps/web/dist first: ` +
        '`pnpm --filter web build` (requires @42ch/nexus-contracts and @42ch/nexus-ui).',
    );
  }
}

function contentType(filePath: string): string {
  const ext = filePath.slice(filePath.lastIndexOf('.')).toLowerCase();
  return MIME[ext] ?? 'application/octet-stream';
}

/** NUL, C0 controls and DEL are rejected in URLs/origins. */
function hasControlChars(value: string): boolean {
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    if (code < 0x20 || code === 0x7f) return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Production desktop scheme `nexus://app` (frozen host contract)
// ---------------------------------------------------------------------------

/** Privileges for `nexus` — must be registered before app ready (P0-T7 main). */
export const DESKTOP_SCHEME_PRIVILEGES = {
  scheme: DESKTOP_SCHEME,
  privileges: {
    standard: true,
    secure: true,
    supportFetchAPI: true,
    corsEnabled: false,
    stream: true,
  },
} as const;

export function desktopIndexUrl(): string {
  return `${DESKTOP_SCHEME}://${DESKTOP_HOST}/index.html`;
}

/**
 * Register the `nexus` privileged scheme. MUST be called before app ready;
 * P0-T7's main awaits this before `app.whenReady()`.
 */
export async function registerDesktopSchemes(): Promise<void> {
  const { protocol } = await import('electron');
  protocol.registerSchemesAsPrivileged([{ ...DESKTOP_SCHEME_PRIVILEGES }]);
}

// ---------------------------------------------------------------------------
// CSP — exact origins inserted by main only, never renderer strings, never `*`
// ---------------------------------------------------------------------------

export interface DesktopCspPolicy {
  /** Selected service origin, e.g. http://127.0.0.1:8420 (exact origin only). */
  serviceOrigin: string;
  /** Explicit fingerprint-probe origin. */
  fingerprintProbeOrigin: string;
  /** Dev HMR on explicitly launched http://localhost:5173 / 127.0.0.1:5173 only. */
  dev?: boolean;
}

const DEV_CSP_ORIGINS = ['http://localhost:5173', 'http://127.0.0.1:5173'];
const DEV_CSP_WS_ORIGINS = ['ws://localhost:5173', 'ws://127.0.0.1:5173'];

/**
 * Validate an exact http(s) origin: scheme http/https, nonempty host, no
 * userinfo, no path/query/hash beyond '/', no wildcard, no control chars.
 */
export function assertDesktopServiceOrigin(origin: unknown): string {
  if (typeof origin !== 'string' || origin.length === 0) {
    throw new Error('CSP origin must be a non-empty string');
  }
  if (hasControlChars(origin) || origin !== origin.trim()) {
    throw new Error('CSP origin must not contain control characters or padding');
  }
  let parsed: URL;
  try {
    parsed = new URL(origin);
  } catch {
    throw new Error(`CSP origin is not a valid URL: ${origin}`);
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`CSP origin must be http(s): ${origin}`);
  }
  if (!parsed.hostname || parsed.hostname.includes('*')) {
    throw new Error(`CSP origin must have a concrete host: ${origin}`);
  }
  if (parsed.username !== '' || parsed.password !== '') {
    throw new Error(`CSP origin must not carry credentials: ${origin}`);
  }
  if (parsed.search !== '' || parsed.hash !== '') {
    throw new Error(`CSP origin must not carry query or fragment: ${origin}`);
  }
  const path = parsed.pathname;
  if (path !== '' && path !== '/') {
    throw new Error(`CSP origin must not carry a path: ${origin}`);
  }
  return parsed.origin;
}

/**
 * Build the frozen desktop CSP. Posture (plan §"Scheme, CSP, navigation and
 * network"): default-src 'self'; script-src 'self'; style-src 'self'
 * 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:;
 * connect-src 'self' <service> <probe> [dev exact origins + HMR ws];
 * object-src 'none'; base-uri 'none'; frame-src 'none'; frame-ancestors
 * 'none'; form-action 'none'. No unsafe-eval, no wildcards.
 */
export function buildDesktopCsp(policy: DesktopCspPolicy): string {
  const serviceOrigin = assertDesktopServiceOrigin(policy.serviceOrigin);
  const probeOrigin = assertDesktopServiceOrigin(policy.fingerprintProbeOrigin);
  const connect = ["'self'", serviceOrigin, probeOrigin];
  if (policy.dev === true) {
    connect.push(...DEV_CSP_ORIGINS, ...DEV_CSP_WS_ORIGINS);
  }
  return [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob: https:",
    "font-src 'self' data:",
    `connect-src ${connect.join(' ')}`,
    "object-src 'none'",
    "base-uri 'none'",
    "frame-src 'none'",
    "frame-ancestors 'none'",
    "form-action 'none'",
  ].join('; ');
}

// ---------------------------------------------------------------------------
// Asset path policy — packaged resources only, traversal + symlink safe
// ---------------------------------------------------------------------------

/**
 * True when the URL path attempts directory traversal (any `..` component
 * after decoding and normalization). Used to refuse even the SPA HTML
 * fallback for traversal requests.
 */
export function desktopPathHasTraversal(urlPath: string): boolean {
  let decoded: string;
  try {
    decoded = decodeURIComponent(urlPath.split('?')[0]?.split('#')[0] ?? '');
  } catch {
    return true;
  }
  if (decoded.includes('\0')) return true;
  // Inspect the RAW decoded components: normalize() would lexically resolve
  // `..` segments (e.g. "a/../../b" → "../b") and hide the traversal attempt.
  const relative = decoded.replace(/^\/+/, '');
  return relative.split('/').includes('..');
}

/**
 * Resolve a `nexus://app/<path>` asset against the canonical dist root.
 * Decode once → component-prefix containment → realpath under the canonical
 * root (symlink escape rejected) → regular file only. Returns the canonical
 * file path or null (unknown asset / traversal / escape).
 */
export function resolveDesktopAssetPath(distRoot: string, urlPath: string): string | null {
  if (desktopPathHasTraversal(urlPath)) {
    return null;
  }
  const decoded = decodeURIComponent(urlPath.split('?')[0]?.split('#')[0] ?? '');
  const relative = decoded.replace(/^\/+/, '');
  if (relative === '') return null;
  const candidate = normalize(join(distRoot, relative));
  if (!isUnderRoot(candidate, normalize(distRoot))) {
    return null;
  }
  let canonicalCandidate: string;
  let root: string;
  try {
    canonicalCandidate = realpathSync.native(candidate);
    root = realpathSync.native(distRoot);
  } catch {
    return null;
  }
  if (!isUnderRoot(canonicalCandidate, root)) {
    return null;
  }
  let stat;
  try {
    stat = lstatSync(canonicalCandidate);
  } catch {
    return null;
  }
  if (!stat.isFile()) {
    return null;
  }
  return canonicalCandidate;
}

/**
 * SPA route fallback applies to HTML navigations only, never API/assets.
 * A bare wildcard Accept value (star/star) is NOT a navigation signal — it
 * is the default fetch/asset Accept value — so it must not trigger the
 * index.html fallback for unknown API/asset paths; only an explicit
 * text/html media type qualifies.
 */
export function isDesktopHtmlNavigation(acceptHeader: string | null | undefined): boolean {
  if (!acceptHeader) return false;
  return acceptHeader.split(',').some((part) => {
    const media = part.trim().split(';')[0]?.trim().toLowerCase();
    return media === 'text/html';
  });
}

export interface DesktopNavigationOptions {
  dev?: boolean;
}

/** Deny navigation anywhere outside the active app origin (dev origins opt-in). */
export function allowDesktopNavigation(
  url: string,
  options?: DesktopNavigationOptions,
): boolean {
  if (url !== url.trim()) return false;
  return isDesktopAppOrigin(url, options);
}

// ---------------------------------------------------------------------------
// External URL policy — the single predicate lives in the shared host
// contract (`isAllowedDesktopExternalUrl` in desktop-contract.ts); it is
// re-exported here so existing protocol consumers keep one import site. The
// implementation is NOT duplicated: raw-string C0/DEL + whitespace + size
// checks run before WHATWG parsing, then http/https, nonempty host, no
// userinfo.
// ---------------------------------------------------------------------------

export { isAllowedDesktopExternalUrl } from './desktop-contract.js';

// ---------------------------------------------------------------------------
// Desktop scheme registration
// ---------------------------------------------------------------------------

/**
 * Pure resolution chain of the `nexus` protocol handler: exact asset first;
 * SPA route fallback to index.html ONLY for an explicit HTML navigation on a
 * non-traversal path; null maps to 404 in the handler. Extracted so the
 * handler-level fallback behavior is testable without launching Electron.
 */
export function resolveDesktopNavigationPath(
  distRoot: string,
  pathname: string,
  acceptHeader: string | null | undefined,
): string | null {
  const direct = resolveDesktopAssetPath(distRoot, pathname);
  if (direct) return direct;
  if (!isDesktopHtmlNavigation(acceptHeader) || desktopPathHasTraversal(pathname)) {
    return null;
  }
  return resolveDesktopAssetPath(distRoot, '/index.html');
}

/**
 * Register the `nexus` protocol handler. Packaged resources only — callers
 * pass the resolved dist root (packaged: Resources/web-dist; dev: built
 * apps/web/dist). No repository fallback here; the caller decides.
 *
 * Unknown asset → 404. SPA route paths fall back to index.html ONLY for HTML
 * navigation (Accept text/html), never for API/assets/traversal.
 */
export async function registerDesktopProtocol(
  distRoot: string,
  policy: DesktopCspPolicy,
): Promise<void> {
  const { protocol } = await import('electron');
  assertDistPresent(distRoot);
  const root = realpathSync.native(distRoot);
  const csp = buildDesktopCsp(policy);

  protocol.handle(DESKTOP_SCHEME, (request) => {
    const url = new URL(request.url);
    if (url.hostname !== DESKTOP_HOST) {
      return new Response('forbidden', { status: 403 });
    }
    let pathname = url.pathname;
    if (pathname.endsWith('/')) pathname += 'index.html';
    if (pathname === '/' || pathname === '') pathname = '/index.html';

    const filePath = resolveDesktopNavigationPath(root, pathname, request.headers.get('accept'));
    if (!filePath) {
      return new Response('not found', { status: 404 });
    }
    const stat = statSync(filePath);
    if (!stat.isFile()) {
      return new Response('not found', { status: 404 });
    }
    const body = Readable.toWeb(createReadStream(filePath)) as ReadableStream;
    return new Response(body, {
      status: 200,
      headers: {
        'Content-Type': contentType(filePath),
        'Content-Security-Policy': csp,
        'X-Content-Type-Options': 'nosniff',
        'Cache-Control': 'no-store',
      },
    });
  });
}
