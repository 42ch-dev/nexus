import { protocol } from 'electron';
import { createReadStream, existsSync, lstatSync, realpathSync, statSync } from 'node:fs';
import { join, normalize, sep } from 'node:path';
import { Readable } from 'node:stream';

export const PROOF_SCHEME = 'nexus-proof';
export const PROOF_HOST = 'app';

const CSP = [
  "default-src 'self' nexus-proof:",
  "script-src 'self' nexus-proof:",
  "style-src 'self' 'unsafe-inline' nexus-proof:",
  "img-src 'self' data: blob: nexus-proof:",
  "font-src 'self' data: nexus-proof:",
  "connect-src 'self' nexus-proof: http://127.0.0.1:* http://localhost:* ws://127.0.0.1:* ws://localhost:*",
  "object-src 'none'",
  "base-uri 'none'",
  "frame-ancestors 'none'",
  "form-action 'none'",
].join('; ');

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

let canonicalDistRoot: string | null = null;

export function proofIndexUrl(): string {
  return `${PROOF_SCHEME}://${PROOF_HOST}/index.html`;
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

function canonicalRoot(distRoot: string): string {
  canonicalDistRoot ??= realpathSync.native(distRoot);
  return canonicalDistRoot;
}

function isUnderRoot(candidate: string, root: string): boolean {
  return candidate === root || candidate.startsWith(root + sep);
}

function resolveSafePath(distRoot: string, urlPath: string): string | null {
  let decoded: string;
  try {
    decoded = decodeURIComponent(urlPath.split('?')[0]?.split('#')[0] ?? '');
  } catch {
    return null;
  }
  const relative = decoded.replace(/^\/+/, '');
  if (relative.includes('..')) {
    return null;
  }
  const candidate = normalize(join(distRoot, relative));
  const root = normalize(distRoot + sep);
  if (!candidate.startsWith(root)) {
    return null;
  }
  if (!existsSync(candidate)) {
    return null;
  }
  let canonicalCandidate: string;
  try {
    canonicalCandidate = realpathSync.native(candidate);
  } catch {
    return null;
  }
  if (!isUnderRoot(canonicalCandidate, canonicalRoot(distRoot))) {
    return null;
  }
  const stat = lstatSync(candidate);
  if (!stat.isFile()) {
    return null;
  }
  return canonicalCandidate;
}

function contentType(filePath: string): string {
  const ext = filePath.slice(filePath.lastIndexOf('.')).toLowerCase();
  return MIME[ext] ?? 'application/octet-stream';
}

export function registerProofProtocol(distRoot: string): void {
  assertDistPresent(distRoot);
  canonicalDistRoot = realpathSync.native(distRoot);

  protocol.handle(PROOF_SCHEME, (request) => {
    const url = new URL(request.url);
    if (url.hostname !== PROOF_HOST) {
      return new Response('forbidden', { status: 403 });
    }
    let pathname = url.pathname;
    if (pathname.endsWith('/')) pathname += 'index.html';
    if (pathname === '/' || pathname === '') pathname = '/index.html';
    const filePath = resolveSafePath(distRoot, pathname);
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
        'Content-Security-Policy': CSP,
        'X-Content-Type-Options': 'nosniff',
        'Cache-Control': 'no-store',
      },
    });
  });
}

export function isProofOrigin(url: string): boolean {
  return allowNavigation(url);
}

export function allowNavigation(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === `${PROOF_SCHEME}:` && parsed.hostname === PROOF_HOST;
  } catch {
    return false;
  }
}

export function isAllowedExternalUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== 'https:') return false;
    const host = parsed.hostname.toLowerCase();
    return host === 'github.com' || host.endsWith('.github.com') || host === 'nexus42.invalid';
  } catch {
    return false;
  }
}
