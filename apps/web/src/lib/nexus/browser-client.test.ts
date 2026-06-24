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

import { BrowserClient, NexusClientError } from '@/lib/nexus';
import { useHandlers } from '@/test/msw-server';

describe('BrowserClient cursor list', () => {
  it('returns { works, pagination } and threads the cursor into the next request', async () => {
    let firstCalled = false;
    let secondCalledWithCursor: string | null = null;
    useHandlers(
      http.get('/v1/local/works', ({ request }) => {
        const url = new URL(request.url);
        const cursor = url.searchParams.get('cursor');
        if (!cursor) {
          firstCalled = true;
          return HttpResponse.json({
            works: [{ work_id: 'w1', title: 'A' }],
            pagination: { limit: 1, has_more: true, next_cursor: 'cur-2' },
          });
        }
        secondCalledWithCursor = cursor;
        return HttpResponse.json({
          works: [{ work_id: 'w2', title: 'B' }],
          pagination: { limit: 1, has_more: false },
        });
      }),
    );

    const client = new BrowserClient();
    const page1 = await client.listWorks({ limit: 1 });
    expect(firstCalled).toBe(true);
    expect(page1.works).toEqual([{ work_id: 'w1', title: 'A' }]);
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
    const error = await client.listSessions().catch((e) => e as NexusClientError);
    expect(error).toBeInstanceOf(NexusClientError);
    expect(error.status).toBe(502);
    expect(error.code).toBe('http_502');
  });
});
