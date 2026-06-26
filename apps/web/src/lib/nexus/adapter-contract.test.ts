/**
 * NexusClient adapter-contract enforcement.
 *
 * Three concerns live here:
 *
 * 1. **Contract guard (architectural invariant).** web-ui.md §5 + apps/web
 *    AGENTS.md: every screen/component/query must depend on the `NexusClient`
 *    *interface* — never on `fetch`/`invoke` directly. That boundary is what
 *    keeps the V1.66 Tauri desktop shell a one-impl swap instead of a rewrite.
 *    The guard scans every non-test source module outside the adapter
 *    implementations and fails if any calls the global `fetch` (the browser
 *    transport). The adapter impls (`browser-client.ts`, `tauri-client.ts`,
 *    `desktop-capabilities.ts`) are the only modules permitted to touch transport
 *    primitives (`fetch` / `window.__TAURI__`).
 *
 * 2. **TauriClient transport parity (V1.66 §5 #1).** `TauriClient` is thin-over-
 *    `BrowserClient`: the 21 data methods reuse the identical HTTP transport to
 *    the resolved desktop loopback origin. This pins that contract — every data
 *    method hits the same `/v1/local/*` path as `BrowserClient`, just against
 *    `http://127.0.0.1:<port>`.
 *
 * 3. **Adapter contract enforcement.** The success/envelope/network paths are
 *    covered in browser-client.test.ts; here we pin the *contract* edges those
 *    tests do not: the `fetchImpl` injection seam, query serialization through
 *    the public API, and the 204 No-Content path. Together they make the
 *    adapter a real boundary, not a convention.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { TauriClient, resolveDesktopPort } from '@/lib/nexus/tauri-client';
import { useHandlers } from '@/test/msw-server';
import { createWorkCreated, healthOk, worksList } from '@/test/handlers';

// ── 1. Contract guard ───────────────────────────────────────────────────────

/**
 * Modules allowed to use transport primitives. The browser adapter owns
 * `fetch`; the Tauri adapter + desktop-capabilities own `window.__TAURI__` +
 * the loopback `fetch`. Everything else must go through the `NexusClient` /
 * `DesktopCapabilities` interfaces.
 */
const ADAPTER_IMPLS = new Set([
  '/src/lib/nexus/browser-client.ts',
  '/src/lib/nexus/tauri-client.ts',
  '/src/lib/nexus/desktop-capabilities.ts',
]);

/** Raw sources for every source module (tests excluded by the glob pattern). */
const sources = import.meta.glob('/src/**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** Test modules and the test harness directory are not production transport. */
const isTestArtifact = (path: string): boolean =>
  /\.(test|spec)\.(ts|tsx)$/.test(path) || path.startsWith('/src/test/');

/**
 * Detect a call to the *global* `fetch`. The negative lookbehind rejects
 * `.refetch(`, `prefetch(`, `useFetch(`, and property access on unrelated
 * objects — only a bare `fetch(` (optionally with whitespace) matches.
 */
const DIRECT_FETCH_CALL = /(?<![\w$])fetch\s*\(/;

describe('NexusClient adapter contract guard', () => {
  it('no screen/component/query module calls the global fetch directly', () => {
    const offenders: string[] = [];
    for (const [path, source] of Object.entries(sources)) {
      if (ADAPTER_IMPLS.has(path)) continue;
      if (isTestArtifact(path)) continue;
      if (DIRECT_FETCH_CALL.test(source)) offenders.push(path);
    }
    expect(offenders).toEqual([]);
  });
});

// ── 2. TauriClient transport parity (V1.66 §5 #1) ───────────────────────────

describe('TauriClient transport parity (thin-over-BrowserClient)', () => {
  it('resolves the desktop port per §5 #3 (explicit → NEXUS_DAEMON_PORT → 8420)', () => {
    expect(resolveDesktopPort()).toBe(8420);
    expect(resolveDesktopPort(9000)).toBe(9000);
    expect(resolveDesktopPort('invalid')).toBe(8420);
  });

  it('fixes the transport origin to the resolved desktop loopback port', async () => {
    const captured: { url?: string } = {};
    const fetchImpl: typeof fetch = (input) => {
      captured.url = String(input);
      return Promise.resolve(
        new Response(JSON.stringify({ status: 'ok', version: 'desk' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    };

    const client = new TauriClient({ port: 8421, fetchImpl });
    expect(client.port).toBe(8421);
    await client.health();
    // The desktop origin is the resolved loopback — NOT same-origin (the
    // browser-tab BrowserClient uses relative `/v1/local/*`).
    expect(captured.url).toBe('http://127.0.0.1:8421/v1/local/runtime/health');
  });

  it('delegates every NexusClient data method to the same /v1/local/* path as BrowserClient', async () => {
    // Capture every request URL TauriClient issues; assert each maps to the
    // identical Local API path the browser transport uses. This is the §5 #1
    // "reuse the identical HTTP transport" invariant, pinned method-by-method.
    const seen = new Set<string>();
    const fetchImpl: typeof fetch = (input, init) => {
      const url = new URL(String(input));
      seen.add(`${init?.method ?? 'GET'} ${url.pathname}`);
      return Promise.resolve(
        new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    };
    const client = new TauriClient({ fetchImpl });
    const workId = 'w1';
    // Exercise all 21 NexusClient methods (health + 20 data).
    await client.health();
    await client.listWorks();
    await client.getWork(workId);
    await client.createWork({ title: '', long_term_goal: '', initial_idea: '' });
    await client.patchWork(workId, { status: 'draft' });
    await client.listSessions();
    await client.getSession('s1');
    await client.listSchedules();
    await client.inspectSchedule('sch1');
    await client.listCapabilities();
    await client.listFindings(workId);
    await client.listPresets();
    await client.scaffoldPreset({ name: 'foo' });
    await client.validatePreset({ path: '/p.yaml' });
    await client.reloadPreset('foo');
    await client.listChapters(workId);
    await client.getChapter(workId, 1);
    await client.getChapterOutline(workId, 1);
    await client.putChapterOutline(workId, 1, { content: '' });
    await client.patchChapter(workId, 1, { slug: 'ch' });
    await client.getChapterBody(workId, 1);

    // Every method must have hit a /v1/local/* path (transport parity with the
    // browser client). If a method silently no-op'd or threw — as the V1.65
    // stub did — its path would be missing and this set would be smaller.
    const paths = [...seen].sort();
    expect(paths.every((p) => p.includes('/v1/local/'))).toBe(true);
    expect(seen.size).toBe(21);
    // Spot-check the chapter surface (the Q5 action target).
    expect(seen).toContain('GET /v1/local/works/w1/chapters/1/body');
    expect(seen).toContain('GET /v1/local/works/w1/chapters/1/outline');
  });
});

// ── 3. Adapter contract enforcement ─────────────────────────────────────────

describe('BrowserClient adapter contract', () => {
  it('delegates transport to the injected fetchImpl (diagnostics seam)', async () => {
    // A container object dodges TS control-flow narrowing of closure-assigned
    // locals: property reads are not narrowed, so the captured values are
    // visible after the awaited call.
    const captured: { url?: string; init?: RequestInit } = {};
    const fetchImpl: typeof fetch = (input, init) => {
      captured.url = String(input);
      captured.init = init;
      return Promise.resolve(
        new Response(JSON.stringify({ status: 'ok', version: 'diag' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
      );
    };

    const client = new BrowserClient({ fetchImpl });
    const health = await client.health();
    expect(health).toEqual({ status: 'ok', version: 'diag' });
    expect(captured.url).toBe('/v1/local/runtime/health');
    expect(captured.init?.method).toBe('GET');
    expect((captured.init?.headers as Record<string, string>)?.Accept).toBe('application/json');
  });

  it('serializes list query params into the request URL (toQueryString)', async () => {
    let requestedUrl: string | null = null;
    useHandlers(
      http.get('/v1/local/works', ({ request }) => {
        requestedUrl = request.url;
        return HttpResponse.json({ items: [], pagination: { limit: 5, has_more: false } });
      }),
    );

    const client = new BrowserClient();
    await client.listWorks({ limit: 5, cursor: 'cur-1' });
    const url = new URL(requestedUrl!);
    expect(url.searchParams.get('limit')).toBe('5');
    expect(url.searchParams.get('cursor')).toBe('cur-1');
  });

  it('omits empty query values (empty string drops the param, no `?` emitted)', async () => {
    const captured: { url?: string } = {};
    useHandlers(
      http.get('/v1/local/works', ({ request }) => {
        captured.url = request.url;
        return HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } });
      }),
    );

    const client = new BrowserClient();
    // `cursor: ''` is allowed by ListWorksQuery; toQueryString drops it, so the
    // request URL must carry no query string.
    await client.listWorks({ cursor: '' });
    expect(captured.url!.includes('?')).toBe(false);
  });

  it('resolves undefined for a 204 No Content response without parsing a body', async () => {
    useHandlers(http.get('/v1/local/runtime/health', () => new HttpResponse(null, { status: 204 })));
    const client = new BrowserClient();
    await expect(client.health()).resolves.toBeUndefined();
  });

  it('unwraps the canonical error envelope on a POST via the registry handler', async () => {
    useHandlers(
      http.post('/v1/local/works', () =>
        HttpResponse.json(
          { success: false, error: { code: 'validation_failed', message: 'Title is required.' } },
          { status: 400 },
        ),
      ),
    );
    const client = new BrowserClient();
    await expect(
      client.createWork({ title: '', long_term_goal: '', initial_idea: '' }),
    ).rejects.toMatchObject({
      name: 'NexusClientError',
      status: 400,
      code: 'validation_failed',
      message: 'Title is required.',
    });
  });

  it('the registry handlers round-trip through the adapter contract', async () => {
    useHandlers(healthOk('0.9.9'), worksList([{ work_id: 'w1' }]), createWorkCreated());
    const client = new BrowserClient();

    const health = await client.health();
    expect(health).toEqual({ status: 'ok', version: '0.9.9' });

    const works = await client.listWorks();
    expect(works.items).toEqual([{ work_id: 'w1' }]);

    const created = await client.createWork({ title: 'Hello', long_term_goal: '', initial_idea: '' });
    expect(created.work_id).toBe('w-new');
    expect(created.status).toBe('draft');
  });

  it('covers the preset adapter surface (list/scaffold/validate/reload)', async () => {
    // The validate/reload endpoints use Google-API-style `:validate` / `:reload`
    // suffixes; msw's path-to-regexp would read those as params, so match them
    // with regexes (deterministic, no param-name collision).
    useHandlers(
      http.get('/v1/local/presets', () =>
        HttpResponse.json({ embedded: [], system: [], user: [{ name: 'user/foo' }] }),
      ),
      http.post('/v1/local/presets', () =>
        HttpResponse.json({ id: 'user/foo', path: '/presets/foo.yaml' }, { status: 201 }),
      ),
      http.post(/\/v1\/local\/presets:validate$/, () =>
        HttpResponse.json({ valid: true, errors: [], warnings: [] }),
      ),
      http.post(/\/v1\/local\/presets\/[^/]+:reload$/, ({ request }) =>
        HttpResponse.json({ reloaded: new URL(request.url).pathname.split('/').pop()!.replace(':reload', '') }),
      ),
    );
    const client = new BrowserClient();

    const presets = await client.listPresets();
    expect(presets.user).toEqual([{ name: 'user/foo' }]);

    const scaffolded = await client.scaffoldPreset({ name: 'foo' });
    expect(scaffolded.id).toBe('user/foo');
    expect(scaffolded.path).toBe('/presets/foo.yaml');

    const validated = await client.validatePreset({ path: '/presets/foo.yaml' });
    expect(validated.valid).toBe(true);

    const reloaded = await client.reloadPreset('foo');
    expect(reloaded.reloaded).toBe('foo');
  });

  it('sends a PATCH with a JSON body for patchWork', async () => {
    let receivedMethod = '';
    let receivedBody: unknown = null;
    useHandlers(
      http.patch('/v1/local/works/:workId', async ({ request, params }) => {
        receivedMethod = request.method;
        receivedBody = await request.json().catch(() => null);
        return HttpResponse.json({ work_id: String(params.workId), title: 'Patched' });
      }),
    );

    const client = new BrowserClient();
    const res = await client.patchWork('w1', { status: 'draft' });
    expect(receivedMethod).toBe('PATCH');
    expect(receivedBody).toEqual({ status: 'draft' });
    expect(res.work_id).toBe('w1');
  });
});

describe('NexusClientError contract (adapter surface)', () => {
  it('is the error type every adapter failure throws', () => {
    // Pinning the class identity keeps the toast bridge (`instanceof` check in
    // queries.ts useErrorToast) stable across both client implementations.
    const err = NexusClientError.fromBody(500, {
      success: false,
      error: { code: 'internal', message: 'boom' },
    });
    expect(err).toBeInstanceOf(NexusClientError);
    expect(err).toBeInstanceOf(Error);
    expect(err.name).toBe('NexusClientError');
  });
});
