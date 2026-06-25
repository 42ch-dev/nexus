/**
 * NexusClient adapter-contract enforcement (R-V164-QC1-S1-P1 T1).
 *
 * Two concerns live here:
 *
 * 1. **Contract guard (architectural invariant).** web-ui.md §5 + apps/web
 *    AGENTS.md: every screen/component/query must depend on the `NexusClient`
 *    *interface* — never on `fetch`/`invoke` directly. That boundary is what
 *    keeps the V1.65 Tauri desktop shell a one-impl swap instead of a rewrite.
 *    The guard scans every non-test source module outside the adapter
 *    implementations and fails if any calls the global `fetch` (the browser
 *    transport). The two adapter impls (`browser-client.ts`, `tauri-client.ts`)
 *    are the only modules permitted to touch transport primitives.
 *
 * 2. **Adapter contract enforcement.** The success/envelope/network paths are
 *    covered in browser-client.test.ts; here we pin the *contract* edges those
 *    tests do not: the `fetchImpl` injection seam, query serialization through
 *    the public API, and the 204 No-Content path. Together they make the
 *    adapter a real boundary, not a convention.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';
import { createWorkCreated, healthOk, worksList } from '@/test/handlers';

// ── 1. Contract guard ───────────────────────────────────────────────────────

/**
 * Modules allowed to use transport primitives. The browser adapter owns
 * `fetch`; the (stub) Tauri adapter will own `invoke`. Everything else must go
 * through the `NexusClient` interface.
 */
const ADAPTER_IMPLS = new Set([
  '/src/lib/nexus/browser-client.ts',
  '/src/lib/nexus/tauri-client.ts',
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

  it('every NexusClient method on TauriClient throws in the browser build', async () => {
    // The stub freezes the boundary: the desktop impl is selected only in the
    // Tauri shell. In the browser build it must fail loud, not silently no-op.
    const { TauriClient } = await import('@/lib/nexus/tauri-client');
    const client = new TauriClient();
    for (const method of [
      'health',
      'listWorks',
      'getWork',
      'createWork',
      'listSessions',
      'listSchedules',
      'listCapabilities',
      'listFindings',
      'listPresets',
      'scaffoldPreset',
      'validatePreset',
      'reloadPreset',
      'listChapters',
      'getChapter',
      'getChapterOutline',
      'putChapterOutline',
      'patchChapter',
      'getChapterBody',
    ] as const) {
      await expect(
        // Every method is either async-throws or returns-then-throws; both
        // surface as a rejected promise from the call site's perspective.
        Promise.resolve().then(() => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          (client as any)[method]();
        }),
      ).rejects.toMatchObject({
        name: 'NexusClientError',
        code: 'not_implemented_in_browser_build',
      });
    }
  });
});

// ── 2. Adapter contract enforcement ─────────────────────────────────────────

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
        return HttpResponse.json({ works: [], pagination: { limit: 5, has_more: false } });
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
        return HttpResponse.json({ works: [], pagination: { limit: 20, has_more: false } });
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
    expect(works.works).toEqual([{ work_id: 'w1' }]);

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
