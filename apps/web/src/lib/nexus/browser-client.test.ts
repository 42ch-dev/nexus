/**
 * BrowserClient integration tests via msw — the end-to-end transport path the
 * screens rely on: cursor pagination shape, the W-1 error-envelope unwrapping
 * on a real fetch, and transport-unreachable handling.
 *
 * These complement the pure unit tests in errors.test.ts (fromBody parsing)
 * by exercising the actual fetch → fromBody → thrown NexusClientError chain.
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { UNAUTHORIZED_EVENT } from '@/lib/nexus/errors';
import type { WorkflowObservationFrame } from '@/lib/nexus/types';
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
    await expect(
      client.createWork({ title: '', long_term_goal: '', initial_idea: '', world_id: 'world-1' }),
    ).rejects
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
// immediately. See the v1.129 `profile-create-reliability` record (local).
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
    // Release-mode unrouted paths fall through to the static SPA fallback shell,
    // which serves text/html with HTTP 200. The classifier must intercept this
    // BEFORE `response.json()` would throw a raw SyntaxError (legacy V1.128
    // behavior).
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

// ── Workflow observation stream (v1.201 P1 T1) ─────────────────────────────
//
// The same-run observation route's frozen client contract
// (`.agents/knowledge/api-design/sse-fetch-stream-consumer.md`):
// `{id, event, data}` triples whose control frames are first-class values
// (a data-less `gap` and an `id`-less `history_unavailable` must survive — the
// data-only agent-host parser drops both), streaming decoding is chunk-split
// safe, a trailing frame without its blank line flushes at stream end, a
// non-200 typed refusal surfaces before any yield, the resume cursor travels
// as `Last-Event-ID`, and an abort ends iteration without throwing.
describe('BrowserClient workflow observation stream (v1.201 P1)', () => {
  const encoder = new TextEncoder();

  /** A `Response` whose SSE body is exactly `chunks`, in order, then EOF. */
  function chunkedResponse(chunks: Uint8Array[]): Response {
    return new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          for (const chunk of chunks) controller.enqueue(chunk);
          controller.close();
        },
      }),
      { status: 200, headers: { 'Content-Type': 'text/event-stream' } },
    );
  }

  /** A `Response` streaming the given SSE text, one network chunk per argument. */
  function frameResponse(...frames: string[]): Response {
    return chunkedResponse(frames.map((frame) => encoder.encode(frame)));
  }

  /** A client whose one fetch call resolves with `response`. */
  function clientReturning(response: Response): BrowserClient {
    return new BrowserClient({
      fetchImpl: (async () => response) as unknown as typeof fetch,
    });
  }

  /** Drain the iterator into an array (the transport's consumer shape). */
  async function collect(
    client: BrowserClient,
    runId: string,
    options: { lastEventId?: string; signal: AbortSignal },
  ): Promise<WorkflowObservationFrame[]> {
    const frames: WorkflowObservationFrame[] = [];
    for await (const frame of client.subscribeWorkflowEvents(runId, options)) {
      frames.push(frame);
    }
    return frames;
  }

  it('(a) yields scripted frames in order, keeping a data-less gap and an id-less history_unavailable', async () => {
    const frames = await collect(
      clientReturning(
        frameResponse(
          'id: e1:1\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"state_revision":1,"status":"running"}\n\n'
            + 'id: e1:2\nevent: host_event\ndata: {"run_id":"r1","epoch":"e1","sequence":2,"step_id":"step-1","attempt_id":"attempt-1","host_event":{"OpStarted":{"session_id":"r1","op_id":"op-1"}}}\n\n'
            + 'event: gap\n\n'
            + 'id: e1:3\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":3,"state_revision":2,"status":"succeeded"}\n\n'
            + 'event: history_unavailable\ndata: {"run_id":"r1","inspect_url":"/v1/daemon/orchestration/sessions/r1"}\n\n',
        ),
      ),
      'r1',
      { signal: new AbortController().signal },
    );

    expect(frames.map((frame) => frame.kind)).toEqual([
      'run_state',
      'host_event',
      'gap',
      'run_state',
      'history_unavailable',
    ]);
    expect(frames[0]).toEqual({
      kind: 'run_state',
      id: 'e1:1',
      payload: { run_id: 'r1', epoch: 'e1', sequence: 1, state_revision: 1, status: 'running' },
    });
    expect(frames[1]).toEqual({
      kind: 'host_event',
      id: 'e1:2',
      payload: {
        run_id: 'r1',
        epoch: 'e1',
        sequence: 2,
        step_id: 'step-1',
        attempt_id: 'attempt-1',
        host_event: { OpStarted: { session_id: 'r1', op_id: 'op-1' } },
      },
    });
    expect(frames[3]).toEqual({
      kind: 'run_state',
      id: 'e1:3',
      payload: { run_id: 'r1', epoch: 'e1', sequence: 3, state_revision: 2, status: 'succeeded' },
    });
  });

  it('(b) survives a multibyte UTF-8 sequence split across network chunks', async () => {
    const prefix =
      'id: e1:1\nevent: host_event\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"step_id":"s1","attempt_id":"a1","host_event":{"ThoughtDelta":{"session_id":"r1","op_id":"op-1","text":"';
    const encoded = encoder.encode(`${prefix}思考の断片"}}}\n\n`);
    // Split one byte INTO the first 3-byte character — a non-streaming
    // `TextDecoder` would decode it as U+FFFD and corrupt the frame.
    const split = encoder.encode(prefix).length + 1;

    const frames = await collect(
      clientReturning(chunkedResponse([encoded.slice(0, split), encoded.slice(split)])),
      'r1',
      { signal: new AbortController().signal },
    );

    expect(frames).toHaveLength(1);
    expect(frames[0]).toEqual({
      kind: 'host_event',
      id: 'e1:1',
      payload: {
        run_id: 'r1',
        epoch: 'e1',
        sequence: 1,
        step_id: 's1',
        attempt_id: 'a1',
        host_event: {
          ThoughtDelta: { session_id: 'r1', op_id: 'op-1', text: '思考の断片' },
        },
      },
    });
  });

  it('(c) flushes a trailing frame that never received its terminating blank line', async () => {
    const frames = await collect(
      clientReturning(
        frameResponse(
          'id: e1:1\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"state_revision":1,"status":"running"}\n\n'
            + 'id: e1:2\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":2,"state_revision":2,"status":"succeeded"}',
        ),
      ),
      'r1',
      { signal: new AbortController().signal },
    );

    expect(
      frames.map((frame) => (frame.kind === 'run_state' ? frame.id : frame.kind)),
    ).toEqual(['e1:1', 'e1:2']);
  });

  it('(d) surfaces a non-200 typed JSON refusal before yielding anything', async () => {
    const response = new Response(
      JSON.stringify({ error: { code: 'not_found', message: 'run does not exist' } }),
      { status: 404, headers: { 'Content-Type': 'application/json' } },
    );
    const frames: WorkflowObservationFrame[] = [];
    let thrown: unknown;
    try {
      for await (const frame of clientReturning(response).subscribeWorkflowEvents('r1', {
        signal: new AbortController().signal,
      })) {
        frames.push(frame);
      }
    } catch (error) {
      thrown = error;
    }

    expect(frames).toEqual([]);
    expect(thrown).toBeInstanceOf(NexusClientError);
    expect((thrown as NexusClientError).status).toBe(404);
    expect((thrown as NexusClientError).code).toBe('not_found');
  });

  it('(e) sends lastEventId as Last-Event-ID and omits the header when absent', async () => {
    const seen: { url: string; headers: Record<string, string> }[] = [];
    const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), headers: (init?.headers ?? {}) as Record<string, string> });
      return chunkedResponse([]);
    }) as unknown as typeof fetch;
    const client = new BrowserClient({
      baseUrl: 'http://127.0.0.1:8420',
      apiKey: 'secret',
      fetchImpl,
    });

    await collect(client, 'run/1', { lastEventId: 'e1:7', signal: new AbortController().signal });
    await collect(client, 'run/1', { signal: new AbortController().signal });

    // Zero query parameters (the route refuses them) + the path param encoded.
    expect(seen[0].url).toBe(
      'http://127.0.0.1:8420/v1/daemon/orchestration/sessions/run%2F1/events',
    );
    expect(seen[0].headers).toEqual({
      Accept: 'text/event-stream',
      'X-API-Key': 'secret',
      'Last-Event-ID': 'e1:7',
    });
    expect(seen[1].url).toBe(seen[0].url);
    expect(seen[1].headers).toEqual({ Accept: 'text/event-stream', 'X-API-Key': 'secret' });
  });

  it('(f) aborting the signal stops iteration without throwing', async () => {
    const controller = new AbortController();
    let releasePull = () => {};
    const pulled = new Promise<void>((resolve) => {
      releasePull = resolve;
    });
    // `highWaterMark: 0` defers `pull` until the first read request, so the
    // iterator is provably blocked on the open stream when the abort lands.
    const body = new ReadableStream<Uint8Array>(
      {
        start(streamController) {
          controller.signal.addEventListener('abort', () => {
            streamController.error(new DOMException('Aborted', 'AbortError'));
          });
        },
        pull() {
          releasePull();
        },
      },
      { highWaterMark: 0 },
    );
    const client = clientReturning(
      new Response(body, { status: 200, headers: { 'Content-Type': 'text/event-stream' } }),
    );

    const frames: WorkflowObservationFrame[] = [];
    const drained = (async () => {
      for await (const frame of client.subscribeWorkflowEvents('r1', {
        signal: controller.signal,
      })) {
        frames.push(frame);
      }
    })();

    await pulled;
    controller.abort();
    await expect(drained).resolves.toBeUndefined();
    expect(frames).toEqual([]);
  });

  it('(g) recognizes a mixed `\\n` + `\\r\\n` blank boundary split across chunks', async () => {
    // §2: a blank-line boundary may mix `\n` and `\r\n`, and either half may
    // straddle a network chunk. Without the `\n\r\n` case the two frames merge
    // into one block (or the first is delayed until EOF), breaking the control
    // frame order the view relies on.
    const first =
      'id: e1:1\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"state_revision":1,"status":"running"}\n\r';
    const second =
      '\nid: e1:2\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":2,"state_revision":2,"status":"succeeded"}\n\n';

    const frames = await collect(
      clientReturning(chunkedResponse([encoder.encode(first), encoder.encode(second)])),
      'r1',
      { signal: new AbortController().signal },
    );

    expect(
      frames.map((frame) => (frame.kind === 'run_state' ? frame.id : frame.kind)),
    ).toEqual(['e1:1', 'e1:2']);
  });

  it('(h) stops draining buffered frames once the consumer aborts mid-chunk', async () => {
    // §3: abort stops iteration without throwing. Two complete frames arrive in
    // ONE chunk, so the resumed generator must re-check the signal between
    // yields instead of draining the rest of the buffer.
    const controller = new AbortController();
    const frames: WorkflowObservationFrame[] = [];
    for await (const frame of clientReturning(
      frameResponse(
        'id: e1:1\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"state_revision":1,"status":"running"}\n\n'
          + 'id: e1:2\nevent: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":2,"state_revision":2,"status":"succeeded"}\n\n',
      ),
    ).subscribeWorkflowEvents('r1', { signal: controller.signal })) {
      frames.push(frame);
      controller.abort();
    }

    expect(
      frames.map((frame) => (frame.kind === 'run_state' ? frame.id : frame.kind)),
    ).toEqual(['e1:1']);
  });

  it('(i) rejects a data frame with no usable `id:` cursor instead of inventing one', async () => {
    // §2: the `<epoch>:<sequence>` cursor is never fabricated. A data frame
    // with an absent or empty `id:` is a wire-protocol violation, so it
    // surfaces as a typed refusal rather than an event replayed/persisted
    // under a synthetic empty cursor.
    const frames = [
      'event: run_state\ndata: {"run_id":"r1","epoch":"e1","sequence":1,"state_revision":1,"status":"running"}\n\n',
      'id:\nevent: host_event\ndata: {"run_id":"r1","epoch":"e1","sequence":2,"step_id":"s1","attempt_id":"a1","host_event":{"OpStarted":{"session_id":"r1","op_id":"op-1"}}}\n\n',
    ];

    for (const frame of frames) {
      const yielded: WorkflowObservationFrame[] = [];
      let thrown: unknown;
      try {
        for await (const value of clientReturning(frameResponse(frame)).subscribeWorkflowEvents(
          'r1',
          { signal: new AbortController().signal },
        )) {
          yielded.push(value);
        }
      } catch (error) {
        thrown = error;
      }

      expect(yielded).toEqual([]);
      expect(thrown).toBeInstanceOf(NexusClientError);
      expect((thrown as NexusClientError).code).toBe('invalid_response');
    }
  });
});

describe('BrowserClient access bootstrap (v1.219 access spec §3.1)', () => {
  it('sends credentials so the session cookie flows on every request', async () => {
    let init: RequestInit | undefined;
    const fetchImpl = (async (_input: RequestInfo | URL, options?: RequestInit) => {
      init = options;
      return new Response(
        JSON.stringify({ items: [], pagination: { limit: 50, has_more: false, next_cursor: null } }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      );
    }) as typeof fetch;

    await new BrowserClient({ fetchImpl }).listWorks();

    expect(init).toMatchObject({ credentials: 'include' });
  });

  it('dispatches the app-level unauthorized event on a 401', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json(
          { success: false, error: { code: 'auth_required', message: 'Authentication required' } },
          { status: 401 },
        )),
    );
    const unauthorized = vi.fn();
    window.addEventListener(UNAUTHORIZED_EVENT, unauthorized);

    await expect(new BrowserClient().listWorks()).rejects.toMatchObject({ status: 401 });

    expect(unauthorized).toHaveBeenCalledTimes(1);
    window.removeEventListener(UNAUTHORIZED_EVENT, unauthorized);
  });
});

describe('BrowserClient character family (v240 actor web surfaces, I-000126)', () => {
  const character = {
    schema_version: 1,
    character_id: 'chr_0123456789abcdef0123456789abcdef',
    owner_creator_id: 'creator-1',
    display_name: 'Mira',
    status: 'active',
    persona: { traits: ['guarded'] },
    revision: 3,
    created_at: '2026-11-01T00:00:00Z',
    updated_at: '2026-11-02T00:00:00Z',
  };

  const binding = {
    schema_version: 1,
    binding_id: 'awb_0123456789abcdef0123456789abcdef',
    character_id: character.character_id,
    world_id: 'wld_0123456789abcdef0123456789abcdef',
    status: 'active',
    revision: 1,
    created_at: '2026-11-01T00:00:00Z',
    updated_at: '2026-11-01T00:00:00Z',
  };

  const characterPath = `/v1/daemon/characters/${character.character_id}`;
  const bindingPath = `${characterPath}/bindings/${binding.binding_id}`;

  it('lists characters with { items, pagination } and threads limit/cursor as query params', async () => {
    let seenUrl = '';
    useHandlers(
      http.get('/v1/daemon/characters', ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          items: [character],
          pagination: { limit: 10, has_more: true, next_cursor: 'cur-2' },
        });
      }),
    );

    const res = await new BrowserClient().listCharacters({ limit: 10, cursor: 'cur-1' });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe('/v1/daemon/characters');
    expect(url.searchParams.get('limit')).toBe('10');
    expect(url.searchParams.get('cursor')).toBe('cur-1');
    expect(res.items[0]!.character_id).toBe(character.character_id);
    expect(res.pagination.next_cursor).toBe('cur-2');
  });

  it('creates a character and maps { character, binding } from POST /characters', async () => {
    let sent: unknown;
    useHandlers(
      http.post('/v1/daemon/characters', async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ character, binding }, { status: 201 });
      }),
    );

    const res = await new BrowserClient().createCharacter({
      display_name: 'Mira',
      world_id: binding.world_id,
    });

    expect(sent).toEqual({ display_name: 'Mira', world_id: binding.world_id });
    expect(res.character.display_name).toBe('Mira');
    expect(res.binding.binding_id).toBe(binding.binding_id);
  });

  it('reads one character by id from GET /characters/:id', async () => {
    useHandlers(http.get(characterPath, () => HttpResponse.json({ character })));

    const res = await new BrowserClient().getCharacter(character.character_id);

    expect(res.character.status).toBe('active');
    expect(res.character.revision).toBe(3);
  });

  it('patches tri-state fields, sending an explicit null and omitting absent keys', async () => {
    let sent: unknown;
    useHandlers(
      http.patch(characterPath, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ character: { ...character, image_uri: undefined } });
      }),
    );

    const res = await new BrowserClient().patchCharacter(character.character_id, {
      expected_revision: 3,
      image_uri: null,
    });

    // `image_uri: null` clears; `persona`/`display_name` are absent (keep).
    expect(sent).toEqual({ expected_revision: 3, image_uri: null });
    expect(res.character.image_uri).toBeUndefined();
  });

  it('archives a character with expected_revision', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/archive`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ character: { ...character, status: 'archived', revision: 4 } });
      }),
    );

    const res = await new BrowserClient().archiveCharacter(character.character_id, {
      expected_revision: 3,
    });

    expect(sent).toEqual({ expected_revision: 3 });
    expect(res.character.status).toBe('archived');
  });

  it('restores a character with expected_revision', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/restore`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ character: { ...character, revision: 5 } });
      }),
    );

    const res = await new BrowserClient().restoreCharacter(character.character_id, {
      expected_revision: 4,
    });

    expect(sent).toEqual({ expected_revision: 4 });
    expect(res.character.status).toBe('active');
    expect(res.character.revision).toBe(5);
  });

  it('adds a binding from POST /characters/:id/bindings', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/bindings`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ binding }, { status: 201 });
      }),
    );

    const res = await new BrowserClient().addCharacterBinding(character.character_id, {
      world_id: binding.world_id,
    });

    expect(sent).toEqual({ world_id: binding.world_id });
    expect(res.binding.world_id).toBe(binding.world_id);
  });

  it('lists bindings with { items, pagination } and cursor params', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(`${characterPath}/bindings`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          items: [binding],
          pagination: { limit: 5, has_more: false },
        });
      }),
    );

    const res = await new BrowserClient().listCharacterBindings(character.character_id, {
      limit: 5,
      cursor: 'cur-9',
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${characterPath}/bindings`);
    expect(url.searchParams.get('cursor')).toBe('cur-9');
    expect(res.items).toHaveLength(1);
    expect(res.pagination.has_more).toBe(false);
  });

  it('reads one binding by id from GET /characters/:id/bindings/:binding_id', async () => {
    useHandlers(http.get(bindingPath, () => HttpResponse.json({ binding })));

    const res = await new BrowserClient().getCharacterBinding(
      character.character_id,
      binding.binding_id,
    );

    expect(res.binding.binding_id).toBe(binding.binding_id);
    expect(res.binding.revision).toBe(1);
  });

  it('patches a binding, sending an explicit null to unlink the world sheet', async () => {
    let sent: unknown;
    useHandlers(
      http.patch(bindingPath, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({ binding: { ...binding, revision: 2 } });
      }),
    );

    const res = await new BrowserClient().patchCharacterBinding(
      character.character_id,
      binding.binding_id,
      { expected_revision: 1, world_sheet_entry_id: null },
    );

    expect(sent).toEqual({ expected_revision: 1, world_sheet_entry_id: null });
    expect(res.binding.revision).toBe(2);
  });

  it('removes a binding and resolves void on 204 No Content', async () => {
    let seenMethod = '';
    useHandlers(
      http.delete(bindingPath, ({ request }) => {
        seenMethod = request.method;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    await expect(
      new BrowserClient().removeCharacterBinding(character.character_id, binding.binding_id),
    ).resolves.toBeUndefined();
    expect(seenMethod).toBe('DELETE');
  });
});

describe('BrowserClient per-character memory family (v240 actor web surfaces, I-000127)', () => {
  const characterId = 'chr_0123456789abcdef0123456789abcdef';
  const bindingId = 'awb_0123456789abcdef0123456789abcdef';
  const worldId = 'wld_0123456789abcdef0123456789abcdef';
  const characterPath = `/v1/daemon/characters/${characterId}`;

  const pending = {
    pending_id: 'cpr_0123456789abcdef0123456789abcdef',
    session_id: 'ses_1',
    character_id: characterId,
    binding_id: bindingId,
    task_kind: 'chapter',
    raw_digest: 'drafted the harbor scene',
    created_at: '2026-11-03T00:00:00Z',
    source_operation_id: null,
  };

  const fragment = {
    fragment_id: 'cmf_0123456789abcdef0123456789abcdef',
    session_id: 'ses_1',
    character_id: characterId,
    binding_id: bindingId,
    summary: 'harbor scene',
    keywords: ['harbor'],
    created_at: '2026-11-03T00:00:00Z',
    revision: 2,
  };

  const tomItem = {
    carrier_entry_id: 'kne_0123456789abcdef0123456789abcdef',
    row_ordinal: 0,
    holder: characterId,
    proposition: 'the harbor is closed',
    order: 1,
    truth: 'Unknown',
  };

  it('captures a pending review from POST /characters/:id/memory/pending-review', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/memory/pending-review`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json(
          { success: true, pending_id: pending.pending_id },
          { status: 201 },
        );
      }),
    );

    const res = await new BrowserClient().captureCharacterPendingReview(characterId, {
      pending_id: pending.pending_id,
      session_id: pending.session_id,
      raw_digest: pending.raw_digest,
    });

    expect(sent).toEqual({
      pending_id: pending.pending_id,
      session_id: pending.session_id,
      raw_digest: pending.raw_digest,
    });
    expect(res.success).toBe(true);
    expect(res.pending_id).toBe(pending.pending_id);
  });

  it('lists character pending reviews with cursor params and no creator_id', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(`${characterPath}/memory/pending-review`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          items: [pending],
          pagination: { limit: 10, has_more: true, next_cursor: 'cur-2' },
        });
      }),
    );

    const res = await new BrowserClient().listCharacterPendingReviews(characterId, {
      binding_id: bindingId,
      limit: 10,
      cursor: 'cur-1',
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${characterPath}/memory/pending-review`);
    expect(url.searchParams.get('binding_id')).toBe(bindingId);
    expect(url.searchParams.get('limit')).toBe('10');
    expect(url.searchParams.get('cursor')).toBe('cur-1');
    // Character scope is the path, never the creator query member.
    expect(url.searchParams.has('creator_id')).toBe(false);
    expect(res.items[0]!.pending_id).toBe(pending.pending_id);
    expect(res.pagination.next_cursor).toBe('cur-2');
  });

  it('counts character pending reviews from the /count route', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(`${characterPath}/memory/pending-review/count`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({ count: 3 });
      }),
    );

    const res = await new BrowserClient().countCharacterPendingReviews(characterId, {
      binding_id: bindingId,
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${characterPath}/memory/pending-review/count`);
    expect(url.searchParams.get('binding_id')).toBe(bindingId);
    expect(res.count).toBe(3);
  });

  it('deletes a character pending review and resolves the { success, pending_id } body', async () => {
    let seenUrl = '';
    useHandlers(
      http.delete(`${characterPath}/memory/pending-review/${pending.pending_id}`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({ success: true, pending_id: pending.pending_id });
      }),
    );

    const res = await new BrowserClient().deleteCharacterPendingReview(
      characterId,
      pending.pending_id,
    );

    expect(new URL(seenUrl).pathname).toBe(
      `${characterPath}/memory/pending-review/${pending.pending_id}`,
    );
    expect(res).toEqual({ success: true, pending_id: pending.pending_id });
  });

  it('drains a character review batch from POST /characters/:id/memory/review', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/memory/review`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({
          promoted: 2,
          fragmented: 1,
          dropped: 0,
          has_more: true,
          processed: 3,
        });
      }),
    );

    const res = await new BrowserClient().reviewCharacterMemory(characterId, {
      binding_id: bindingId,
    });

    expect(sent).toEqual({ binding_id: bindingId });
    expect(res).toMatchObject({ promoted: 2, fragmented: 1, has_more: true });
  });

  it('lists character memory fragments under the fragments key', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(`${characterPath}/memory/fragments`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          fragments: [fragment],
          pagination: { limit: 5, has_more: false },
        });
      }),
    );

    const res = await new BrowserClient().listCharacterMemoryFragments(characterId, {
      binding_id: bindingId,
      limit: 5,
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${characterPath}/memory/fragments`);
    expect(url.searchParams.get('limit')).toBe('5');
    expect(res.fragments[0]!.fragment_id).toBe(fragment.fragment_id);
    expect(res.fragments[0]!.revision).toBe(2);
  });

  it('promotes a character fragment via the :promote verb with expected_revision', async () => {
    let sent: unknown;
    let seenUrl = '';
    useHandlers(
      http.post(
        new RegExp(`/characters/${characterId}/memory/fragments/[^/]+:promote$`),
        async ({ request }) => {
          seenUrl = request.url;
          sent = await request.json();
          return HttpResponse.json({
            fragment: { ...fragment, binding_id: undefined, revision: 3 },
          });
        },
      ),
    );

    const res = await new BrowserClient().promoteCharacterFragment(
      characterId,
      fragment.fragment_id,
      { expected_revision: 2 },
    );

    expect(new URL(seenUrl).pathname).toBe(
      `${characterPath}/memory/fragments/${fragment.fragment_id}:promote`,
    );
    expect(sent).toEqual({ expected_revision: 2 });
    // Promotion clears binding-local provenance and bumps the revision.
    expect(res.fragment.revision).toBe(3);
    expect(res.fragment.binding_id).toBeUndefined();
  });

  it('reads or regenerates the character SOUL narrative from /soul/reflect', async () => {
    let sent: unknown;
    useHandlers(
      http.post(`${characterPath}/soul/reflect`, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({
          character_id: characterId,
          state: 'current',
          narrative: 'Mira keeps her distance until trust is earned.',
          stale: false,
          current_fragment_count: 12,
          current_distinct_keyword_count: 21,
          min_fragment_count: 10,
          min_distinct_keyword_count: 20,
        });
      }),
    );

    const res = await new BrowserClient().reflectCharacterSoul(characterId, {
      force_regenerate: true,
    });

    expect(sent).toEqual({ force_regenerate: true });
    expect(res.character_id).toBe(characterId);
    expect(res.state).toBe('current');
  });

  it('lists character ToM beliefs with the required binding_id + world_id query', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(`${characterPath}/tom`, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          items: [tomItem],
          pagination: { limit: 50, has_more: false },
        });
      }),
    );

    const res = await new BrowserClient().listCharacterTom(characterId, {
      binding_id: bindingId,
      world_id: worldId,
      cursor: 'cur-3',
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${characterPath}/tom`);
    expect(url.searchParams.get('binding_id')).toBe(bindingId);
    expect(url.searchParams.get('world_id')).toBe(worldId);
    expect(url.searchParams.get('cursor')).toBe('cur-3');
    expect(res.items[0]!.holder).toBe(characterId);
    expect(res.items[0]!.order).toBe(1);
  });

  it('records a ToM belief with POST /characters/:id/tom and maps the CAS revision', async () => {
    let sent: unknown;
    let seenMethod = '';
    useHandlers(
      http.post(`${characterPath}/tom`, async ({ request }) => {
        seenMethod = request.method;
        sent = await request.json();
        return HttpResponse.json({
          carrier_entry_id: tomItem.carrier_entry_id,
          revision: 5,
          mind_state_id: 'mst_0123456789abcdef0123456789abcdef',
        });
      }),
    );

    const res = await new BrowserClient().recordCharacterTom(characterId, {
      world_id: worldId,
      binding_id: bindingId,
      carrier_entry_id: tomItem.carrier_entry_id,
      expected_revision: 4,
      holder: characterId,
      proposition: tomItem.proposition,
      order: 1,
    });

    expect(seenMethod).toBe('POST');
    expect(sent).toMatchObject({
      world_id: worldId,
      binding_id: bindingId,
      carrier_entry_id: tomItem.carrier_entry_id,
      expected_revision: 4,
      holder: characterId,
      order: 1,
    });
    expect(res.revision).toBe(5);
  });
});

describe('BrowserClient actor-knowledge family (v240 actor web surfaces, I-000128)', () => {
  const characterId = 'chr_0123456789abcdef0123456789abcdef';
  const bindingId = 'awb_0123456789abcdef0123456789abcdef';
  const worldId = 'wld_0123456789abcdef0123456789abcdef';
  const entryId = 'kne_0123456789abcdef0123456789abcdef';
  const characterPath = `/v1/daemon/characters/${characterId}`;
  const knowledgePath = `${characterPath}/knowledge`;

  const item = {
    entry_id: entryId,
    owner: { kind: 'character', id: characterId },
    holder_entry_id: 'hld_0123456789abcdef0123456789abcdef',
    disclosure: 'owner-private',
    block_type: 'character',
    canonical_name: 'Mira',
    status: 'confirmed',
    revision: 3,
    created_at: '2026-11-03T00:00:00Z',
  };

  it('lists character-scoped knowledge from GET /characters/:id/knowledge with cursor params', async () => {
    let seenUrl = '';
    useHandlers(
      http.get(knowledgePath, ({ request }) => {
        seenUrl = request.url;
        return HttpResponse.json({
          items: [item],
          pagination: { limit: 25, has_more: true, next_cursor: 'cur-2' },
        });
      }),
    );

    const res = await new BrowserClient().listCharacterKnowledge(characterId, {
      limit: 25,
      cursor: 'cur-1',
    });

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(knowledgePath);
    expect(url.searchParams.get('limit')).toBe('25');
    expect(url.searchParams.get('cursor')).toBe('cur-1');
    expect(res.items[0]!.entry_id).toBe(entryId);
    expect(res.items[0]!.owner).toEqual({ kind: 'character', id: characterId });
    expect(res.pagination.next_cursor).toBe('cur-2');
  });

  it('reads one character-scoped entry from GET /characters/:id/knowledge/:entry_id', async () => {
    let seenMethod = '';
    useHandlers(
      http.get(`${knowledgePath}/${entryId}`, ({ request }) => {
        seenMethod = request.method;
        return HttpResponse.json({ item, summary: 'the harbor pilot' });
      }),
    );

    const res = await new BrowserClient().getCharacterKnowledge(characterId, entryId);

    expect(seenMethod).toBe('GET');
    expect(res.item.entry_id).toBe(entryId);
    expect(res.summary).toBe('the harbor pilot');
  });

  it('patches a character-scoped entry with canonical_name, summary and audience under CAS', async () => {
    let sent: unknown;
    let seenMethod = '';
    useHandlers(
      http.patch(`${knowledgePath}/${entryId}`, async ({ request }) => {
        seenMethod = request.method;
        sent = await request.json();
        return HttpResponse.json({ item: { ...item, revision: 4 }, summary: null });
      }),
    );

    const res = await new BrowserClient().patchCharacterKnowledge(characterId, entryId, {
      expected_revision: 3,
      canonical_name: 'Mira Vell',
      summary: null,
      audience: { kind: 'shared' },
    });

    expect(seenMethod).toBe('PATCH');
    expect(sent).toEqual({
      expected_revision: 3,
      canonical_name: 'Mira Vell',
      summary: null,
      audience: { kind: 'shared' },
    });
    expect(res.item.revision).toBe(4);
    expect(res.summary).toBeNull();
  });

  it('deletes a character-scoped entry with expected_revision as a query member', async () => {
    let seenUrl = '';
    useHandlers(
      http.delete(`${knowledgePath}/${entryId}`, ({ request }) => {
        seenUrl = request.url;
        return new HttpResponse(null, { status: 204 });
      }),
    );

    await expect(
      new BrowserClient().deleteCharacterKnowledge(characterId, entryId, { expected_revision: 4 }),
    ).resolves.toBeUndefined();

    const url = new URL(seenUrl);
    expect(url.pathname).toBe(`${knowledgePath}/${entryId}`);
    expect(url.searchParams.get('expected_revision')).toBe('4');
  });

  it('reads the admitted-principal view from POST /actor-knowledge/view', async () => {
    let sent: unknown;
    let seenUrl = '';
    useHandlers(
      http.post('/v1/daemon/actor-knowledge/view', async ({ request }) => {
        seenUrl = request.url;
        sent = await request.json();
        return HttpResponse.json({
          items: [item],
          pagination: { limit: 10, has_more: false },
        });
      }),
    );

    const res = await new BrowserClient().viewActorKnowledge({
      actor_ref: { actor_kind: 'character', character_id: characterId },
      world_id: worldId,
      binding_id: bindingId,
      limit: 10,
    });

    expect(new URL(seenUrl).pathname).toBe('/v1/daemon/actor-knowledge/view');
    // Scope is the body's admitted actor ref; the path carries no character id.
    expect(sent).toEqual({
      actor_ref: { actor_kind: 'character', character_id: characterId },
      world_id: worldId,
      binding_id: bindingId,
      limit: 10,
    });
    expect(res.items[0]!.entry_id).toBe(entryId);
    expect(res.pagination.has_more).toBe(false);
  });

  it('creates an entry through the admitted-principal route, not the character path', async () => {
    let sent: unknown;
    let seenUrl = '';
    useHandlers(
      http.post('/v1/daemon/actor-knowledge/entries', async ({ request }) => {
        seenUrl = request.url;
        sent = await request.json();
        return HttpResponse.json({ item }, { status: 201 });
      }),
    );

    const res = await new BrowserClient().createActorKnowledgeEntry({
      owner_kind: 'character',
      character_id: characterId,
      block_type: 'character',
      canonical_name: 'Mira',
      audience: { kind: 'author-only' },
    });

    // Creation addresses the principal, never `…/characters/{id}/knowledge`.
    expect(new URL(seenUrl).pathname).toBe('/v1/daemon/actor-knowledge/entries');
    expect(sent).toEqual({
      owner_kind: 'character',
      character_id: characterId,
      block_type: 'character',
      canonical_name: 'Mira',
      audience: { kind: 'author-only' },
    });
    expect(res.item.entry_id).toBe(entryId);
  });
});

describe('BrowserClient strategy apply (v241 P3.5 T1)', () => {
  const strategyId = 'user/demo';
  const applyPath = `/v1/daemon/strategies/${encodeURIComponent(strategyId)}/apply`;
  const appliedPath = `/v1/daemon/strategies/${encodeURIComponent(strategyId)}/applied`;
  const applied = {
    apply_id: 'ap_1',
    approval_id: 'apr_1',
    content_hash: 'hash-1',
    applied_at: '2026-11-14T00:00:00Z',
    supersedes_apply_id: null,
  };

  it('posts the approval content hash to .../apply and returns the appended apply row', async () => {
    let sent: unknown;
    let seenPath = '';
    useHandlers(
      http.post(applyPath, async ({ request }) => {
        seenPath = new URL(request.url).pathname;
        sent = await request.json();
        return HttpResponse.json(applied);
      }),
    );

    const res = await new BrowserClient().applyStrategy(strategyId, {
      approval_content_hash: 'hash-1',
    });

    expect(seenPath).toBe(applyPath);
    // The approval is named by content hash only: the path owns the strategy.
    expect(sent).toEqual({ approval_content_hash: 'hash-1' });
    expect(res).toEqual(applied);
  });

  it('reads the current apply and the newest approval from .../applied', async () => {
    let seenPath = '';
    useHandlers(
      http.get(appliedPath, ({ request }) => {
        seenPath = new URL(request.url).pathname;
        return HttpResponse.json({
          current_apply: applied,
          newest_approval: {
            approval_id: 'apr_2',
            content_hash: 'hash-2',
            candidate_revision: 4,
            approved_at: '2026-11-14T01:00:00Z',
          },
        });
      }),
    );

    const res = await new BrowserClient().getStrategyApplied(strategyId);

    expect(seenPath).toBe(appliedPath);
    expect(res.current_apply).toEqual(applied);
    expect(res.newest_approval?.approval_id).toBe('apr_2');
  });

  it('keeps a strategy with no apply readable: both halves are null', async () => {
    useHandlers(
      http.get(appliedPath, () =>
        HttpResponse.json({ current_apply: null, newest_approval: null }),
      ),
    );

    await expect(new BrowserClient().getStrategyApplied(strategyId)).resolves.toEqual({
      current_apply: null,
      newest_approval: null,
    });
  });

  it('keeps the refusal reason typed on a 409 already_applied', async () => {
    useHandlers(
      http.post(applyPath, () =>
        HttpResponse.json(
          {
            success: false,
            error: {
              code: 'invalid_input',
              message: 'approval apr_1 is already the current apply',
              details: {
                code: 'apply_refused',
                reason: 'already_applied',
                message: 'approval apr_1 is already the current apply',
                apply_id: 'ap_1',
                approval_id: 'apr_1',
              },
            },
          },
          { status: 409 },
        ),
      ),
    );

    const error = await new BrowserClient()
      .applyStrategy(strategyId, { approval_content_hash: 'hash-1' })
      .catch((thrown: unknown) => thrown);

    expect(error).toBeInstanceOf(NexusClientError);
    expect((error as NexusClientError).status).toBe(409);
    expect(((error as NexusClientError).details as { reason?: string }).reason).toBe(
      'already_applied',
    );
  });
});
