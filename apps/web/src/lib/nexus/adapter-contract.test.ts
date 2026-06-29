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
 *    `BrowserClient`: the 24 `NexusClient` methods reuse the identical HTTP
 *    transport to the resolved desktop loopback origin. This pins that contract
 *    — every data method hits the same `/v1/local/*` path as `BrowserClient`,
 *    just against `http://127.0.0.1:<port>`.
 *
 * 3. **Adapter contract enforcement.** The success/envelope/network paths are
 *    covered in browser-client.test.ts; here we pin the *contract* edges those
 *    tests do not: the `fetchImpl` injection seam, query serialization through
 *    the public API, and the 204 No-Content path. Together they make the
 *    adapter a real boundary, not a convention.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { BrowserClient, NexusClientError, type NexusClient } from '@/lib/nexus';
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
    // Exercise all 28 NexusClient methods (health + 27 data). The three
    // preset methods (getPreset/updatePreset/deletePreset) were promoted in
    // V1.67 G2 (R-V167P1-QC3-S1), and the four outline+timeline methods
    // (getWorkOutline/patchOutlineStructure/patchOutlineChapter/patchTimelineEvent)
    // were promoted in V1.72 Track A — they must hit the same transport as the
    // rest of the surface, not silently no-op.
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
    await client.getPreset('foo');
    await client.updatePreset('foo', { yaml: 'name: foo\n' });
    await client.deletePreset('foo');
    await client.listChapters(workId);
    await client.getChapter(workId, 1);
    await client.getChapterOutline(workId, 1);
    await client.putChapterOutline(workId, 1, { content: '' });
    await client.patchChapter(workId, 1, { slug: 'ch' });
    await client.getChapterBody(workId, 1);
    await client.getWorkOutline(workId);
    await client.patchOutlineStructure(workId, {
      work_id: workId,
      base_revision: 1,
      operation: 'attach_to_volume',
    });
    await client.patchOutlineChapter(workId, 1, {
      work_id: workId,
      chapter_id: 1,
      base_revision: 1,
      set: {},
    });
    await client.patchTimelineEvent(workId, {
      work_id: workId,
      base_revision: 1,
      operation: 'add_event',
    });

    // Every method must have hit a /v1/local/* path (transport parity with the
    // browser client). If a method silently no-op'd or threw — as the V1.65
    // stub did — its path would be missing and this set would be smaller.
    const paths = [...seen].sort();
    expect(paths.every((p) => p.includes('/v1/local/'))).toBe(true);
    expect(seen.size).toBe(28);
    // Spot-check the chapter surface (the Q5 action target).
    expect(seen).toContain('GET /v1/local/works/w1/chapters/1/body');
    expect(seen).toContain('GET /v1/local/works/w1/chapters/1/outline');
    // Spot-check the V1.72 outline+timeline surface.
    expect(seen).toContain('GET /v1/local/works/w1/outline');
    expect(seen).toContain('POST /v1/local/works/w1/outline/patch');
    expect(seen).toContain('POST /v1/local/works/w1/chapters/1/patch');
    expect(seen).toContain('POST /v1/local/works/w1/timeline/patch');
    // Spot-check the V1.67 G2 preset-promotion surface.
    expect(seen).toContain('GET /v1/local/presets/foo');
    expect(seen).toContain('PATCH /v1/local/presets/foo');
    expect(seen).toContain('DELETE /v1/local/presets/foo');
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

  it('routes the V1.67 G2 preset methods to the {id} path with the right verb (get/update/delete)', async () => {
    // Contract edge: the three promoted preset methods (getPreset/updatePreset/
    // deletePreset) target `/v1/local/presets/{id}` with GET/PATCH/DELETE and
    // URL-encode the id into the path. Pinned here via the `fetchImpl` seam
    // (this file owns the adapter boundary — fetchImpl injection + path
    // serialization + 204 handling) rather than in browser-client.test.ts,
    // which owns the behavioral response shapes. R-V167P1-QC3-S1.
    const seen: { method: string; url: string; body?: unknown }[] = [];
    const fetchImpl: typeof fetch = async (input, init) => {
      seen.push({
        method: init?.method ?? 'GET',
        url: String(input),
        body: init?.body ? JSON.parse(String(init.body)) : undefined,
      });
      if ((init?.method ?? 'GET') === 'DELETE') {
        return new Response(null, { status: 204 });
      }
      return new Response(
        JSON.stringify({ id: 'user/foo', source: 'user', path: 'p.yaml', yaml: 'name: x\n', updated: true }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      );
    };

    const client = new BrowserClient({ fetchImpl });
    await client.getPreset('user/foo');
    await client.updatePreset('user/foo', { yaml: 'name: x\n' });
    // deletePreset returns void on 204 — resolves without throwing.
    await expect(client.deletePreset('user/foo')).resolves.toBeUndefined();

    // The id (`user/foo`) is encoded into the path as `user%2Ffoo`; PATCH
    // carries the YAML body; DELETE emits no body.
    expect(seen).toEqual([
      { method: 'GET', url: '/v1/local/presets/user%2Ffoo' },
      { method: 'PATCH', url: '/v1/local/presets/user%2Ffoo', body: { yaml: 'name: x\n' } },
      { method: 'DELETE', url: '/v1/local/presets/user%2Ffoo' },
    ]);
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

// ── 4. Preset-method parity guard (R-V167P1-QC3-S1) ─────────────────────────

/**
 * The V1.67 G2 preset promotion (getPreset/updatePreset/deletePreset) added
 * three methods to the `NexusClient` interface. This guard fails — at compile
 * time or runtime — if the interface and either adapter implementation drift
 * on those methods:
 *  - Compile-time: the `satisfies readonly (keyof NexusClient)[]` constraint
 *    makes a future interface removal/rename a type error in this file.
 *  - Runtime: the tests below make a missing adapter implementation a test
 *    failure. (`class BrowserClient implements NexusClient` already enforces
 *    presence at compile time; the runtime check pins it against this curated
 *    list so a future override that drops a method is caught by the suite, not
 *    just tsc.)
 */
const PRESET_METHODS = [
  'getPreset',
  'updatePreset',
  'deletePreset',
] as const satisfies readonly (keyof NexusClient)[];

describe('NexusClient preset-method parity guard (R-V167P1-QC3-S1)', () => {
  it('BrowserClient implements every preset method on the NexusClient interface', () => {
    const client = new BrowserClient();
    for (const method of PRESET_METHODS) {
      expect(typeof client[method], `BrowserClient.${method} must be a function`).toBe('function');
    }
  });

  it('TauriClient implements every preset method on the NexusClient interface', () => {
    // TauriClient is thin-over-BrowserClient (extends it); the guard pins that
    // the inheritance is not accidentally broken by a future override that
    // drops a preset method. The methods are never invoked here, so the
    // fetchImpl is a defensive stub only.
    const client = new TauriClient({
      fetchImpl: async () =>
        new Response(JSON.stringify({ ok: true }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        }),
    });
    for (const method of PRESET_METHODS) {
      expect(typeof client[method], `TauriClient.${method} must be a function`).toBe('function');
    }
  });
});
