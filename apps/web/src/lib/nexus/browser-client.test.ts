/**
 * BrowserClient integration tests via msw — the end-to-end transport path the
 * screens rely on: cursor pagination shape, the W-1 error-envelope unwrapping
 * on a real fetch, and transport-unreachable handling.
 *
 * These complement the pure unit tests in errors.test.ts (fromBody parsing)
 * and adapters.test.ts (F-P3/F-F1) by exercising the actual fetch → fromBody
 * → thrown NexusClientError chain.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';

describe('BrowserClient cursor list', () => {
  it('returns { items, pagination } and threads the cursor into the next request', async () => {
    let firstCalled = false;
    let secondCalledWithCursor: string | null = null;
    useHandlers(
      http.get('/v1/daemon/works', ({ request }) => {
        const url = new URL(request.url);
        const cursor = url.searchParams.get('cursor');
        if (!cursor) {
          firstCalled = true;
          return HttpResponse.json({
            items: [{ work_id: 'w1', title: 'A' }],
            pagination: { limit: 1, has_more: true, next_cursor: 'cur-2' },
          });
        }
        secondCalledWithCursor = cursor;
        return HttpResponse.json({
          items: [{ work_id: 'w2', title: 'B' }],
          pagination: { limit: 1, has_more: false },
        });
      }),
    );

    const client = new BrowserClient();
    const page1 = await client.listWorks({ limit: 1 });
    expect(firstCalled).toBe(true);
    expect(page1.items).toEqual([{ work_id: 'w1', title: 'A' }]);
    expect(page1.pagination.next_cursor).toBe('cur-2');
    expect(page1.pagination.has_more).toBe(true);

    const page2 = await client.listWorks({ limit: 1, cursor: page1.pagination.next_cursor });
    expect(secondCalledWithCursor).toBe('cur-2');
    expect(page2.pagination.has_more).toBe(false);
  });

  it('unwraps the daemon error envelope into a NexusClientError (W-1, live fetch)', async () => {
    useHandlers(
      http.post('/v1/daemon/works', () =>
        HttpResponse.json(
          {
            success: false,
            error: { code: 'validation_failed', message: 'Title is required.' },
          },
          { status: 400 },
        ),
      ),
    );

    const client = new BrowserClient();
    await expect(client.createWork({ title: '', long_term_goal: '', initial_idea: '' })).rejects
      .toMatchObject({
        name: 'NexusClientError',
        status: 400,
        code: 'validation_failed',
        message: 'Title is required.',
      });
  });

  it('rejects with transport_unreachable when the daemon is unreachable', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () => HttpResponse.error()),
    );

    const client = new BrowserClient();
    await expect(client.listWorks()).rejects.toMatchObject({
      name: 'NexusClientError',
      code: 'transport_unreachable',
    });
  });

  it('parses the findings list canonical { items, pagination } shape (F-P2)', async () => {
    useHandlers(
      http.get('/v1/daemon/works/:workId/findings', () =>
        HttpResponse.json({
          items: [{ finding_id: 'f1', work_id: 'w1', severity: 'critical', status: 'open', title: 't', description: 'd', target_executor: 'x', kind: 'k', created_at: 1, updated_at: 1 }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.listFindings('w1');
    expect(res.items).toHaveLength(1);
    expect(res.items[0]!.finding_id).toBe('f1');
    expect(res.pagination.has_more).toBe(false);
  });

  it('surfaces ad-hoc (StatusCode, String) error bodies via the generic fallback', async () => {
    // Some orchestration handlers still emit non-envelope bodies (R-V164-FE1-ORCH).
    useHandlers(
      http.get('/v1/daemon/orchestration/sessions', () =>
        new HttpResponse('upstream timeout', { status: 502 }),
      ),
    );

    const client = new BrowserClient();
    let error: unknown;
    try {
      await client.listSessions();
    } catch (e) {
      error = e;
    }
    expect(error).toBeInstanceOf(NexusClientError);
    const nexusError = error as NexusClientError;
    expect(nexusError.status).toBe(502);
    expect(nexusError.code).toBe('http_502');
  });
});

describe('BrowserClient chapter content routes (V1.65)', () => {
  it('lists chapters with the canonical { items, pagination } shape', async () => {
    useHandlers(
      http.get('/v1/daemon/works/:workId/chapters', () =>
        HttpResponse.json({
          items: [{ work_id: 'w1', chapter: 1, volume: 1, planned_word_count: 4000, status: 'not_started', created_at: '2026-06-25T00:00:00Z', updated_at: '2026-06-25T00:00:00Z' }],
          pagination: { limit: 20, has_more: false },
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.listChapters('w1');
    expect(res.items).toHaveLength(1);
    expect(res.items[0]!.chapter).toBe(1);
  });

  it('reads a chapter outline', async () => {
    useHandlers(
      http.get('/v1/daemon/works/:workId/chapters/:n/outline', ({ params }) =>
        HttpResponse.json({
          work_id: params.workId,
          chapter: Number(params.n),
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: '# Chapter 1',
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getChapterOutline('w1', 1);
    expect(res.content).toBe('# Chapter 1');
  });

  it('patches chapter structure with confirm flag for finalized chapters', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.patch('/v1/daemon/works/:workId/chapters/:n', async ({ request, params }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          work_id: params.workId,
          chapter: Number(params.n),
          volume: 1,
          planned_word_count: 5000,
          status: 'finalized',
          can_edit_outline: true,
          can_edit_structure: true,
          body_read_only: true,
          protection: { level: 'confirm_structure_edit', reason: 'Chapter is finalized.' },
          created_at: '2026-06-25T00:00:00Z',
          updated_at: '2026-06-25T00:00:00Z',
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.patchChapter('w1', 1, { planned_word_count: 5000, confirm_structural_edit: true });
    expect(receivedBody).toEqual({ planned_word_count: 5000, confirm_structural_edit: true });
    expect(res.planned_word_count).toBe(5000);
  });

  it('reads a chapter body', async () => {
    useHandlers(
      http.get('/v1/daemon/works/:workId/chapters/:n/body', ({ params }) =>
        HttpResponse.json({
          work_id: params.workId,
          chapter: Number(params.n),
          volume: 1,
          body_path: 'Works/WRK/Stories/ch01-ch01.md',
          content: 'Body prose.',
          frontmatter: { status: 'draft' },
          read_only: true,
          updated_at: '2026-06-25T00:00:00Z',
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getChapterBody('w1', 1);
    expect(res.content).toBe('Body prose.');
    expect(res.read_only).toBe(true);
  });
});

describe('BrowserClient preset CRUD (V1.67 G2 promotion)', () => {
  it('fetches a preset manifest as raw YAML via getPreset', async () => {
    useHandlers(
      http.get('/v1/daemon/presets/:id', ({ params }) =>
        HttpResponse.json({
          id: params.id,
          source: 'user',
          path: 'presets/my-strategy/preset.yaml',
          yaml: 'name: my-strategy\n',
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getPreset('my-strategy');
    expect(res.id).toBe('my-strategy');
    expect(res.source).toBe('user');
    expect(res.yaml).toContain('name: my-strategy');
  });

  it('replaces user preset YAML via updatePreset and echoes { id, updated }', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.patch('/v1/daemon/presets/:id', async ({ request, params }) => {
        receivedBody = await request.json();
        return HttpResponse.json({ id: params.id, updated: true });
      }),
    );

    const client = new BrowserClient();
    const res = await client.updatePreset('my-strategy', { yaml: 'name: edited\n' });
    expect(receivedBody).toEqual({ yaml: 'name: edited\n' });
    expect(res).toEqual({ id: 'my-strategy', updated: true });
  });

  it('resolves void when deletePreset returns 204 No Content', async () => {
    useHandlers(
      http.delete('/v1/daemon/presets/:id', () => new HttpResponse(null, { status: 204 })),
    );

    const client = new BrowserClient();
    await expect(client.deletePreset('my-strategy')).resolves.toBeUndefined();
  });
});

describe('BrowserClient remote connection model (V1.92 P1)', () => {
  it('uses the configured base URL for absolute requests', async () => {
    useHandlers(
      http.get('https://192.168.1.42:8420/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: '0.20.0' }),
      ),
    );

    const client = new BrowserClient({ baseUrl: 'https://192.168.1.42:8420' });
    const health = await client.health();
    expect(health.version).toBe('0.20.0');
  });

  it('injects X-API-Key on protected requests when apiKey is set', async () => {
    let receivedKey: string | null = null;
    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/works', ({ request }) => {
        receivedKey = request.headers.get('X-API-Key');
        return HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } });
      }),
    );

    const client = new BrowserClient({ baseUrl: 'https://remote.example:8420', apiKey: 'secret-key' });
    await client.listWorks();
    expect(receivedKey).toBe('secret-key');
  });

  it('omits X-API-Key on the unauthenticated fingerprint endpoint', async () => {
    let receivedKey: string | null = 'sentinel';
    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/runtime/cert-fingerprint', ({ request }) => {
        receivedKey = request.headers.get('X-API-Key');
        return HttpResponse.json({ fingerprint: 'SHA256:aa:bb:cc', algorithm: 'sha256' });
      }),
    );

    const client = new BrowserClient({ baseUrl: 'https://remote.example:8420', apiKey: 'secret-key' });
    const res = await client.certFingerprint();
    expect(receivedKey).toBeNull();
    expect(res.fingerprint).toBe('SHA256:aa:bb:cc');
  });

  it('surfaces a remote-aware transport message when baseUrl is set', async () => {
    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/works', () => HttpResponse.error()),
    );

    const client = new BrowserClient({ baseUrl: 'https://remote.example:8420' });
    await expect(client.listWorks()).rejects.toMatchObject({
      name: 'NexusClientError',
      code: 'transport_unreachable',
      message: 'Cannot reach the daemon at this address. This usually means the URL or port is wrong, the daemon is not running, or the browser blocked a self-signed certificate. For remote daemons using self-signed certificates, use the Nexus desktop app — it can trust the certificate and store it in the OS keychain.',
    });
  });

  it('rejects with unauthorized when the API key is rejected', async () => {
    useHandlers(
      http.get('https://remote.example:8420/v1/daemon/works', () =>
        HttpResponse.json(
          { success: false, error: { code: 'unauthorized', message: 'Invalid API key.' } },
          { status: 401 },
        )),
    );

    const client = new BrowserClient({ baseUrl: 'https://remote.example:8420', apiKey: 'wrong-key' });
    await expect(client.listWorks()).rejects.toMatchObject({
      name: 'NexusClientError',
      status: 401,
      code: 'unauthorized',
      message: 'Invalid API key.',
    });
  });
});

describe('BrowserClient Strategy canvas write boundary (V1.71)', () => {
  it('patches a state label and description', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/strategies/:strategyId/states/:stateId/patch', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          new_revision: 3,
          validation_summary: { errors: [], warnings: [] },
          side_effects: ["renamed state 'draft' -> 'outline'"],
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.strategyPatchState('novel-writing', 'draft', {
      strategy_id: 'novel-writing',
      state_id: 'draft',
      base_revision: 2,
      set: { label: 'outline', description: 'Outline the story.' },
    });
    expect(receivedBody).toEqual({
      strategy_id: 'novel-writing',
      state_id: 'draft',
      base_revision: 2,
      set: { label: 'outline', description: 'Outline the story.' },
    });
    expect(res.new_revision).toBe(3);
  });

  it('rewires a transition target', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/strategies/:strategyId/transitions/patch', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          new_revision: 4,
          validation_summary: { errors: [], warnings: [] },
          side_effects: ["transition draft -> revise set to edit"],
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.strategyPatchTransition('novel-writing', {
      strategy_id: 'novel-writing',
      base_revision: 3,
      source_state_id: 'draft',
      old_target: 'revise',
      new_target: 'edit',
      transition_kind: 'next',
    });
    expect(receivedBody).toEqual({
      strategy_id: 'novel-writing',
      base_revision: 3,
      source_state_id: 'draft',
      old_target: 'revise',
      new_target: 'edit',
      transition_kind: 'next',
    });
    expect(res.new_revision).toBe(4);
  });

  it('writes a prompt template body', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/strategies/:strategyId/states/:stateId/prompt/patch', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          new_revision: 5,
          validation_summary: { errors: [], warnings: [] },
          side_effects: ["wrote prompt template 'prompts/draft.md'"],
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.strategyPatchPromptTemplate('novel-writing', 'draft', {
      strategy_id: 'novel-writing',
      state_id: 'draft',
      base_revision: 4,
      template_ref: 'prompts/draft.md',
      set: { body: '# Draft prompt\n' },
    });
    expect(receivedBody).toEqual({
      strategy_id: 'novel-writing',
      state_id: 'draft',
      base_revision: 4,
      template_ref: 'prompts/draft.md',
      set: { body: '# Draft prompt\n' },
    });
    expect(res.new_revision).toBe(5);
  });
});

describe('BrowserClient compute module + KeyBlock state wiring (V1.114 P2)', () => {
  it('lists compute modules with { items, has_more }', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({
          items: [
            {
              module_id: 'combat-resolver',
              name: 'Combat Resolver',
              version: '1.0.0',
              nexus_abi_version: 1,
              required_key_block_types: ['battle_report'],
              compute_export: 'resolve',
              init_export: 'init',
            },
          ],
          has_more: false,
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getComputeModules();
    expect(res.items).toHaveLength(1);
    expect(res.items[0]!.module_id).toBe('combat-resolver');
    expect(res.has_more).toBe(false);
  });

  it('fetches a compute module manifest by id', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/modules/:moduleId', ({ params }) =>
        HttpResponse.json({
          module_id: params.moduleId,
          name: 'Combat Resolver',
          version: '1.0.0',
          nexus_abi_version: 1,
          required_key_block_types: ['battle_report'],
          compute_export: 'resolve',
          init_export: 'init',
          description: 'Resolves combat encounters.',
          host_functions: ['kb_read', 'narrative_query'],
          max_fuel: 1000,
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getComputeModule('combat-resolver');
    expect(res.module_id).toBe('combat-resolver');
    expect(res.name).toBe('Combat Resolver');
    expect(res.host_functions).toEqual(['kb_read', 'narrative_query']);
    expect(res.max_fuel).toBe(1000);
  });

  it('reads the mutable state of a computable KeyBlock', async () => {
    useHandlers(
      http.get('/v1/daemon/worlds/:worldId/kb/key-blocks/:keyBlockId/state', () =>
        HttpResponse.json({
          state: { hp: 42, buffs: ['shield'] },
          is_computable: true,
          version: 7,
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getKeyBlockState('w1', 'kb-1');
    expect(res.state).toEqual({ hp: 42, buffs: ['shield'] });
    expect(res.is_computable).toBe(true);
    expect(res.version).toBe(7);
  });
});

describe('BrowserClient compute runs (V1.147 P1)', () => {
  const runSummary = {
    run_id: 'run_1',
    status: 'succeeded',
    module_id: 'basic-combat',
    module_version: '1.0.0',
    world_id: 'w1',
    created_at: '2026-07-31T00:00:00Z',
  };

  it('invokes a module via POST /compute/run and returns the run outcome', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/compute/run', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          run_id: 'run_1',
          status: 'succeeded',
          module_id: 'basic-combat',
          module_version: '1.0.0',
          proposals: {
            schema_version: 1,
            state_delta: [{ op: 'sub', path: 'character.current_hp', target_key_block_id: 'kb-def', value: 6 }],
            timeline_events: [],
            new_key_blocks: [],
            battle_report: { kind: 'combat', winner: 'kb-atk' },
          },
          created_at: '2026-07-31T00:00:00Z',
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.runCompute({
      world_id: 'w1',
      module_id: 'basic-combat',
      invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
    });
    expect(receivedBody).toEqual({
      world_id: 'w1',
      module_id: 'basic-combat',
      invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
    });
    expect(res.run_id).toBe('run_1');
    expect(res.status).toBe('succeeded');
    expect(res.proposals?.state_delta).toHaveLength(1);
  });

  it('rejects with the canonical error envelope when the daemon fails a run', async () => {
    // Live daemon shape (qc2 W-001): `POST /run` never returns 200 with
    // status=failed — on compute failure the daemon persists a Failed row and
    // returns the shared ErrorResponse envelope (422 sandbox / 500 internal).
    // The Failed row is surfaced through the runs list refetch, not as data
    // from this call.
    useHandlers(
      http.post('/v1/daemon/compute/run', () =>
        HttpResponse.json(
          {
            success: false,
            error: {
              code: 'compute_wall_time_exceeded',
              message: 'module exceeded wall time',
              details: {},
              extensions: {},
            },
          },
          { status: 422 },
        ),
      ),
    );

    const client = new BrowserClient();
    const result = client.runCompute({ world_id: 'w1', module_id: 'basic-combat' });
    await expect(result).rejects.toMatchObject({
      status: 422,
      code: 'compute_wall_time_exceeded',
    });
  });

  it('accepts a run with an optional subset of timeline events', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.post('/v1/daemon/compute/runs/:runId/accept', async ({ params, request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          run_id: params.runId,
          status: 'applied',
          applied: { state_delta_count: 1, events_created: 1, new_entries_created: 0 },
          timeline_event_ids: ['evt_0'],
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.acceptRun('run_1', { timeline_event_ids_to_accept: ['evt_0'] });
    expect(receivedBody).toEqual({ timeline_event_ids_to_accept: ['evt_0'] });
    expect(res.status).toBe('applied');
    expect(res.applied.events_created).toBe(1);
  });

  it('accepts a run with an empty JSON body when no request is given', async () => {
    let receivedBody: unknown = null;
    let receivedContentType: string | null = null;
    useHandlers(
      http.post('/v1/daemon/compute/runs/:runId/accept', async ({ request }) => {
        receivedContentType = request.headers.get('content-type');
        receivedBody = await request.json();
        return HttpResponse.json({
          run_id: 'run_1',
          status: 'applied',
          applied: { state_delta_count: 0, events_created: 2, new_entries_created: 0 },
          timeline_event_ids: ['evt_0', 'evt_1'],
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.acceptRun('run_1');
    // The daemon's axum Json extractor requires a JSON body — the client sends
    // `{}` (accept everything) rather than an empty POST.
    expect(receivedContentType).toContain('application/json');
    expect(receivedBody).toEqual({});
    expect(res.status).toBe('applied');
  });

  it('discards a run via POST /runs/:id/discard', async () => {
    useHandlers(
      http.post('/v1/daemon/compute/runs/:runId/discard', ({ params }) =>
        HttpResponse.json({ run_id: params.runId, status: 'discarded' }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.discardRun('run_1');
    expect(res).toEqual({ run_id: 'run_1', status: 'discarded' });
  });

  it('lists runs with filters + cursor threaded as query params', async () => {
    let seenUrl: URL | null = null;
    useHandlers(
      http.get('/v1/daemon/compute/runs', ({ request }) => {
        seenUrl = new URL(request.url);
        return HttpResponse.json({
          items: [runSummary],
          has_more: true,
          next_cursor: 'cur-2',
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.listRuns({
      world_id: 'w1',
      module_id: 'basic-combat',
      status: 'succeeded',
      limit: 10,
      cursor: 'cur-1',
    });
    expect(seenUrl).not.toBeNull();
    expect(seenUrl!.searchParams.get('world_id')).toBe('w1');
    expect(seenUrl!.searchParams.get('module_id')).toBe('basic-combat');
    expect(seenUrl!.searchParams.get('status')).toBe('succeeded');
    expect(seenUrl!.searchParams.get('limit')).toBe('10');
    expect(seenUrl!.searchParams.get('cursor')).toBe('cur-1');
    expect(res.items).toHaveLength(1);
    expect(res.items[0]!.run_id).toBe('run_1');
    expect(res.has_more).toBe(true);
    expect(res.next_cursor).toBe('cur-2');
  });

  it('fetches a run detail by id', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/runs/:runId', ({ params }) =>
        HttpResponse.json({
          ...runSummary,
          run_id: params.runId,
          invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
          proposals: {
            schema_version: 1,
            state_delta: [],
            timeline_events: [],
            new_key_blocks: [],
            battle_report: { kind: 'combat' },
          },
        }),
      ),
    );

    const client = new BrowserClient();
    const res = await client.getRun('run_1');
    expect(res.run_id).toBe('run_1');
    expect(res.status).toBe('succeeded');
    expect(res.invocation_params).toEqual({ attacker_id: 'kb-atk', defender_id: 'kb-def' });
    expect(res.proposals?.battle_report.kind).toBe('combat');
  });
});

// ── Transport classification matrix (V1.129 P0 T3) ─────────────────────────
//
// The classifier is a pure function of `(baseUrl, cause)` plus the
// `http_fallback` response detector. Each kind is exercised once with the
// minimal signal shape so a regression in the classifier ordering surfaces
// immediately. See `.mstar/iterations/v1.129/specs/profile-create-reliability.md`
// § Classification algorithm for the locked ordering.
describe('BrowserClient transport classification (V1.129 P0)', () => {
  /** Build a client with a fetchImpl stub that rejects with the given cause. */
  function clientThrowing(cause: unknown, baseUrl?: string): BrowserClient {
    const fetchImpl = vi.fn<typeof globalThis.fetch>().mockRejectedValue(cause);
    return new BrowserClient({
      baseUrl,
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
  }

  /** Build a client with a fetchImpl stub that resolves with the given Response. */
  function clientResponding(response: Response, baseUrl?: string): BrowserClient {
    const fetchImpl = vi.fn<typeof globalThis.fetch>().mockResolvedValue(response);
    return new BrowserClient({
      baseUrl,
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
  }

  it('classifies `daemon_down` when baseUrl is empty (local mode, fetch throws)', async () => {
    const client = clientThrowing(new TypeError('Failed to fetch'));
    const err = await client.listWorks().catch((e) => e);
    expect(err).toBeInstanceOf(NexusClientError);
    expect(err.code).toBe('transport_unreachable');
    expect(err.kind).toBe('daemon_down');
    // Backwards-compat: the legacy multi-cause `message` is still attached so
    // existing toast tests keep passing during the P1 sweep.
    expect(err.message).toContain('nexus42 daemon start');
  });

  it('classifies `daemon_down` for a TypeError against a loopback baseUrl (desktop local)', async () => {
    const client = clientThrowing(
      new TypeError('Failed to fetch'),
      'http://127.0.0.1:8420',
    );
    const err = await client.listWorks().catch((e) => e);
    expect(err.kind).toBe('daemon_down');
    expect(err.code).toBe('transport_unreachable');
    expect(err.message).toContain('nexus42 daemon start');
    expect(err.message).not.toContain('self-signed certificate');
  });

  it('classifies `network` for a TypeError "Failed to fetch" with a remote baseUrl', async () => {
    const client = clientThrowing(
      new TypeError('Failed to fetch'),
      'https://remote.example:8420',
    );
    const err = await client.listWorks().catch((e) => e);
    expect(err.kind).toBe('network');
    expect(err.code).toBe('transport_unreachable');
  });

  it('classifies `timeout` for an AbortError (explicit user / signal abort)', async () => {
    const client = clientThrowing(
      new DOMException('The user aborted a request', 'AbortError'),
      'https://remote.example:8420',
    );
    const err = await client.listWorks().catch((e) => e);
    expect(err.kind).toBe('timeout');
  });

  it('classifies `tls` when the cause message carries a TLS signal', async () => {
    // Browsers do not surface the real cert failure to JS — the classifier
    // matches common signal substrings best-effort.
    const client = clientThrowing(
      new TypeError('ERR_CERT_AUTHORITY_INVALID'),
      'https://remote.example:8420',
    );
    const err = await client.listWorks().catch((e) => e);
    expect(err.kind).toBe('tls');
  });

  it('falls open to `network` when the throw carries no TLS signal (TLS fail-open)', async () => {
    // Locked by spec: when in doubt, `network` — do not over-claim cert rejection.
    const client = clientThrowing(
      new TypeError('Failed to fetch'),
      'https://remote.example:8420',
    );
    const err = await client.listWorks().catch((e) => e);
    expect(err.kind).toBe('network');
  });

  it('classifies `http_fallback` when the daemon returns 200 + text/html (release SPA fallback)', async () => {
    // Release-mode unrouted paths fall through to the embedded SPA shell, which
    // serves text/html with HTTP 200. The classifier must intercept this BEFORE
    // `response.json()` would throw a raw SyntaxError (legacy V1.128 behavior).
    const response = new Response('<!doctype html><html>…</html>', {
      status: 200,
      headers: { 'content-type': 'text/html; charset=utf-8' },
    });
    const client = clientResponding(response);
    const err = await client.listWorks().catch((e) => e);
    expect(err).toBeInstanceOf(NexusClientError);
    expect(err.kind).toBe('http_fallback');
    expect(err.code).toBe('transport_unreachable');
    expect(err.details).toEqual({ status: 200, content_type: 'text/html; charset=utf-8' });
  });

  it('keeps the legacy transport_unreachable code for backwards-compat with toast routing', async () => {
    // Existing tests across the suite assert on `code === 'transport_unreachable'`.
    // The new `kind` field layers on top — it does not rename the legacy code.
    const client = clientThrowing(new TypeError('Failed to fetch'), 'https://remote.example:8420');
    const err = await client.listWorks().catch((e) => e);
    expect(err.code).toBe('transport_unreachable');
    expect(err.kind).toBe('network');
  });

  it('still routes HTTP errors through fromBody (kind stays undefined for HTTP errors)', async () => {
    // HTTP error responses are NOT transport-class errors — they have a real
    // status and a parseable body. `kind` must remain undefined so HTTP-error
    // UX (driven by `code`/`details`) does not get mis-routed into the dialog's
    // transport-recovery CTA path.
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json(
          { success: false, error: { code: 'validation_failed', message: 'bad' } },
          { status: 400 },
        ),
      ),
    );
    const client = new BrowserClient();
    const err = await client.listWorks().catch((e) => e);
    expect(err.status).toBe(400);
    expect(err.code).toBe('validation_failed');
    expect(err.kind).toBeUndefined();
  });
});
