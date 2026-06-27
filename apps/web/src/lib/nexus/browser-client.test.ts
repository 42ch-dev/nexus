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
import { describe, expect, it } from 'vitest';

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';

describe('BrowserClient cursor list', () => {
  it('returns { items, pagination } and threads the cursor into the next request', async () => {
    let firstCalled = false;
    let secondCalledWithCursor: string | null = null;
    useHandlers(
      http.get('/v1/local/works', ({ request }) => {
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
      http.post('/v1/local/works', () =>
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
      http.get('/v1/local/works', () => HttpResponse.error()),
    );

    const client = new BrowserClient();
    await expect(client.listWorks()).rejects.toMatchObject({
      name: 'NexusClientError',
      code: 'transport_unreachable',
    });
  });

  it('parses the findings list canonical { items, pagination } shape (F-P2)', async () => {
    useHandlers(
      http.get('/v1/local/works/:workId/findings', () =>
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
      http.get('/v1/local/orchestration/sessions', () =>
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
      http.get('/v1/local/works/:workId/chapters', () =>
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
      http.get('/v1/local/works/:workId/chapters/:n/outline', ({ params }) =>
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

  it('writes a chapter outline via PUT', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.put('/v1/local/works/:workId/chapters/:n/outline', async ({ request, params }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          work_id: params.workId,
          chapter: Number(params.n),
          volume: 1,
          outline_path: 'Works/WRK/Outlines/chapters/ch01-outline.md',
          content: (receivedBody as { content?: string }).content ?? '',
          updated_at: '2026-06-25T00:00:00Z',
        });
      }),
    );

    const client = new BrowserClient();
    const res = await client.putChapterOutline('w1', 1, { content: '# Updated' });
    expect(receivedBody).toEqual({ content: '# Updated' });
    expect(res.content).toBe('# Updated');
  });

  it('patches chapter structure with confirm flag for finalized chapters', async () => {
    let receivedBody: unknown = null;
    useHandlers(
      http.patch('/v1/local/works/:workId/chapters/:n', async ({ request, params }) => {
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
      http.get('/v1/local/works/:workId/chapters/:n/body', ({ params }) =>
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
      http.get('/v1/local/presets/:id', ({ params }) =>
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
      http.patch('/v1/local/presets/:id', async ({ request, params }) => {
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
      http.delete('/v1/local/presets/:id', () => new HttpResponse(null, { status: 204 })),
    );

    const client = new BrowserClient();
    await expect(client.deletePreset('my-strategy')).resolves.toBeUndefined();
  });
});
