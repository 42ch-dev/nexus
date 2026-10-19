import { existsSync, realpathSync, type Stats } from 'node:fs';
import { open, realpath, stat } from 'node:fs/promises';
import type { ServerResponse } from 'node:http';
import { dirname, extname, isAbsolute, relative, resolve, sep } from 'node:path';
import type { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { HttpError } from './errors.js';

/**
 * Repo-root-anchored default build output (`apps/web/dist`). Three levels up
 * lands on the repo root from both `src/` (tsx) and `dist/` (compiled), the
 * same module-relative derivation `routes.ts` uses for its manifest.
 */
const BUILT_WEB_INDEX = new URL('../../../apps/web/dist/index.html', import.meta.url);

/**
 * Resolve the web-UI build directory.
 *
 * `null` force-disables serving (test/embedder hook — the discovery probe is
 * repo-anchored, so a no-dist boot would otherwise depend on checkout state);
 * `undefined` probes the repo-anchored build output and misses silently; an
 * explicit directory must actually hold `index.html`, so a mistyped
 * `--web-dist` fails the start instead of silently serving nothing.
 */
export function resolveWebDist(explicit: string | undefined | null): string | null {
  if (explicit === null) return null;
  if (explicit === undefined) {
    const index = fileURLToPath(BUILT_WEB_INDEX);
    return existsSync(index) ? dirname(index) : null;
  }
  const root = resolve(explicit);
  if (!existsSync(resolve(root, 'index.html'))) {
    throw new Error(`--web-dist requires ${root}/index.html to exist: ${root}`);
  }
  return root;
}

const CONTENT_TYPES: Record<string, string> = {
  html: 'text/html; charset=utf-8',
  js: 'text/javascript',
  mjs: 'text/javascript',
  css: 'text/css; charset=utf-8',
  map: 'application/json',
  svg: 'image/svg+xml',
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  ico: 'image/x-icon',
  json: 'application/json',
  txt: 'text/plain; charset=utf-8',
  wasm: 'application/wasm',
  woff: 'font/woff',
  woff2: 'font/woff2',
};

function notFound(): HttpError {
  return new HttpError(404, 'not_found', 'not found');
}

interface StaticHit {
  /** Containment-verified realpath: the bytes actually served (§5 step 4). */
  real: string;
  /** Whether the request-relative path sits under the `assets/` cache tier. */
  assets: boolean;
}

/**
 * Map one request path to the file it may serve, or `null` when the path is a
 * plain miss (ENOENT / directory) and the SPA fallback applies. Traversal
 * refusals throw so the caller's single error envelope reports them.
 */
async function resolveInsideRoot(
  root: string,
  rootReal: string,
  pathname: string,
): Promise<StaticHit | null> {
  let decoded: string;
  try {
    decoded = decodeURIComponent(pathname);
  } catch {
    throw notFound();
  }
  if (decoded.includes('\0') || decoded.includes('\\')) throw notFound();
  // The request path is rooted at `root`; a decoded `//…` is an absolute path.
  const relPath = decoded.startsWith('/') ? decoded.slice(1) : decoded;
  if (isAbsolute(relPath)) throw notFound();
  const candidate = resolve(root, relPath);
  const rel = relative(root, candidate);
  if (rel.startsWith('..') || isAbsolute(rel)) throw notFound();
  let stats: Stats;
  try {
    stats = await stat(candidate);
  } catch {
    return null;
  }
  if (!stats.isFile()) return null;
  let real: string;
  try {
    real = await realpath(candidate);
  } catch {
    return null;
  }
  if (real !== rootReal && !real.startsWith(rootReal + sep)) throw notFound();
  return { real, assets: relPath.split(sep)[0] === 'assets' };
}

export type StaticFileHandler = (
  res: ServerResponse,
  method: string,
  pathname: string,
) => Promise<void>;

/**
 * Read the first chunk off `stream` and push it back, so a filesystem failure
 * on an already-open file surfaces before any response byte is written.
 * Rejects on a read failure; resolves at EOF.
 */
function probeFirstChunk(stream: Readable): Promise<void> {
  return new Promise<void>((resolveProbe, rejectProbe) => {
    function cleanup() {
      stream.off('readable', onReadable);
      stream.off('end', onEnd);
      stream.off('error', onError);
    }
    function onReadable() {
      const chunk = stream.read();
      // Null below the probe's watermark just means the next read is pending;
      // 'readable' fires again.
      if (chunk === null) return;
      cleanup();
      stream.unshift(chunk);
      resolveProbe();
    }
    function onEnd() {
      cleanup();
      resolveProbe();
    }
    function onError(error: Error) {
      cleanup();
      rejectProbe(error);
    }
    stream.on('readable', onReadable);
    stream.once('end', onEnd);
    stream.once('error', onError);
  });
}

/**
 * Serve the built SPA from `root` with `index.html` (`fallback`) for any
 * unknown path. `pathname` is the raw request-target path: it is decoded here
 * exactly once, and never again.
 */
export function createStaticFileHandler(options: {
  root: string;
  fallback: string;
}): StaticFileHandler {
  const root = resolve(options.root);
  const fallback = resolve(options.fallback);
  // Computed once: symlink containment is judged against the real root, never
  // against a path that is itself a symlink.
  const rootReal = realpathSync(root);
  // The fallback is containment-checked once here too: a symlinked `index.html`
  // escaping the root must be refused like a direct hit, never served. Its
  // realpath is what gets read (same swap-window shrink as a direct hit).
  let safeFallback: string | null = null;
  try {
    const fallbackReal = realpathSync(fallback);
    if (fallbackReal === rootReal || fallbackReal.startsWith(rootReal + sep)) {
      safeFallback = fallbackReal;
    }
  } catch {
    // Unresolvable fallback: refuse at request time (404).
  }

  return async (res, method, pathname) => {
    // Set before any throw so the single error envelope carries it too.
    res.setHeader('X-Content-Type-Options', 'nosniff');
    if (method !== 'GET' && method !== 'HEAD') {
      throw new HttpError(405, 'method_not_allowed', 'method not allowed');
    }
    const hit = await resolveInsideRoot(root, rootReal, pathname);
    const file = hit ? hit.real : safeFallback;
    if (file === null) throw notFound();
    const cache = hit?.assets ? 'public, max-age=3600' : 'no-store';
    if (!hit) {
      // The fallback has no request-path probe of its own: a missing or
      // non-file `index.html` is a clean 404 miss, never a served body (§5).
      let stats: Stats;
      try {
        stats = await stat(file);
      } catch {
        throw notFound();
      }
      if (!stats.isFile()) throw notFound();
    }
    // Open before committing success headers: a read failure must land on the
    // shared `sendError` envelope without the asset's framing, or a keep-alive
    // socket would inherit a stale `Content-Length` (§6).
    const handle = await open(file, 'r');
    let size: number;
    try {
      // Frame the body from the opened descriptor, so the advertised
      // `Content-Length` and the streamed bytes describe one file object (§6).
      size = (await handle.stat()).size;
    } catch (error) {
      await handle.close();
      throw error;
    }
    // A HEAD answers from metadata alone; a GET probes its first read below, so
    // the framing and the body can never come from different file objects.
    const body = method === 'GET' ? handle.createReadStream() : null;
    if (body) {
      // Probe the first read before committing success headers: a failure on the
      // opened file must reach the shared `sendError` envelope, rather than being
      // swallowed onto an already-destroyed response (§6).
      try {
        await probeFirstChunk(body);
      } catch (error) {
        body.destroy();
        throw error;
      }
    }
    res.statusCode = 200;
    res.setHeader(
      'Content-Type',
      CONTENT_TYPES[extname(file).slice(1).toLowerCase()] ?? 'application/octet-stream',
    );
    res.setHeader('Cache-Control', cache);
    res.setHeader('Content-Length', String(size));
    if (body === null) {
      await handle.close();
      res.end();
      return;
    }
    // Streamed, never buffered whole: large assets stay off the shared loop.
    try {
      await pipeline(body, res);
    } catch {
      // A body that failed after the headers went out cannot be repaired: cut
      // the connection rather than append an envelope to a short, framed body (§6).
      res.destroy();
    }
  };
}
