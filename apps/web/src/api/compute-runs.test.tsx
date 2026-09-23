/**
 * Compute run hooks (V1.147 P1) — query + mutation invalidation contract.
 *
 * Against a real BrowserClient + msw:
 * - `useComputeRuns` cursor-paginates `GET /compute/runs` (fetchNextPage
 *   threads the opaque cursor) and keys the cache by filter.
 * - `useComputeRun` fetches one run's detail.
 * - `useRunCompute` invalidates the runs lists so a freshly created run
 *   appears without a manual refresh.
 * - `useAcceptRun` / `useDiscardRun` invalidate the runs lists + that run's
 *   detail so the status flip (Needs review → Applied / Discarded) is
 *   reflected everywhere it is cached.
 * - `useClearRuns` invalidates the runs lists + every cached run detail so the
 *   deleted terminal rows leave the mounted views (the World effect they
 *   already committed stays).
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { useQuery } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import {
  flattenPages,
  useAcceptRun,
  useClearRuns,
  useComputeRun,
  useComputeRuns,
  useDiscardRun,
  useRunCompute,
} from '@/api/queries';
import { queryKeys } from '@/lib/nexus/query-keys';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { RunSummary } from '@42ch/nexus-contracts';

function makeRun(over: Partial<RunSummary> = {}): RunSummary {
  return {
    run_id: 'run_1',
    status: 'succeeded',
    module_id: 'basic-combat',
    module_version: '1.0.0',
    world_id: 'w1',
    created_at: '2026-07-31T00:00:00Z',
    ...over,
  };
}

describe('useComputeRuns — cursor pagination', () => {
  it('fetches the first page, then threads next_cursor into fetchNextPage', async () => {
    let secondCursor: string | null = null;
    useHandlers(
      http.get('/v1/daemon/compute/runs', ({ request }) => {
        const cursor = new URL(request.url).searchParams.get('cursor');
        if (!cursor) {
          return HttpResponse.json({
            items: [makeRun({ run_id: 'run_1' })],
            has_more: true,
            next_cursor: 'cur-2',
          });
        }
        secondCursor = cursor;
        return HttpResponse.json({
          items: [makeRun({ run_id: 'run_2', created_at: '2026-07-30T00:00:00Z' })],
          has_more: false,
        });
      }),
    );

    function Harness() {
      const runs = useComputeRuns();
      const items = flattenPages(runs.data);
      return (
        <div>
          <span data-testid="runs">{items.map((r) => r.run_id).join(',') || 'none'}</span>
          <button type="button" onClick={() => void runs.fetchNextPage()}>
            More
          </button>
        </div>
      );
    }

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(await screen.findByText('run_1')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /more/i }));
    await waitFor(() => expect(screen.getByTestId('runs')).toHaveTextContent('run_1,run_2'));
    expect(secondCursor).toBe('cur-2');
  });

  it('passes filters through to the request', async () => {
    let seenUrl: URL | null = null;
    useHandlers(
      http.get('/v1/daemon/compute/runs', ({ request }) => {
        seenUrl = new URL(request.url);
        return HttpResponse.json({ items: [], has_more: false });
      }),
    );

    function Harness() {
      const runs = useComputeRuns({ world_id: 'w1', module_id: 'basic-combat', status: 'failed' });
      return <span data-testid="state">{runs.isSuccess ? 'ok' : 'loading'}</span>;
    }

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(await screen.findByText('ok')).toBeInTheDocument();
    expect(seenUrl!.searchParams.get('world_id')).toBe('w1');
    expect(seenUrl!.searchParams.get('module_id')).toBe('basic-combat');
    expect(seenUrl!.searchParams.get('status')).toBe('failed');
  });
});

describe('useComputeRun — detail', () => {
  it('fetches the run detail by id', async () => {
    useHandlers(
      http.get('/v1/daemon/compute/runs/:runId', ({ params }) =>
        HttpResponse.json({
          ...makeRun({ run_id: String(params.runId) }),
          invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
        }),
      ),
    );

    function Harness() {
      const run = useComputeRun('run_1');
      return <span data-testid="run">{run.data?.run_id ?? 'none'}</span>;
    }

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(await screen.findByText('run_1')).toBeInTheDocument();
  });
});

describe('useRunCompute — runs-list invalidation', () => {
  it('refetches the runs list after a run is invoked', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    let receivedBody: unknown = null;
    useHandlers(
      http.get('/v1/daemon/compute/runs', () => listSpy()),
      http.post('/v1/daemon/compute/run', async ({ request }) => {
        receivedBody = await request.json();
        return HttpResponse.json({
          run_id: 'run_9',
          status: 'succeeded',
          module_id: 'basic-combat',
          module_version: '1.0.0',
          created_at: '2026-07-31T01:00:00Z',
        });
      }),
    );

    function Harness() {
      const runs = useComputeRuns();
      const runCompute = useRunCompute();
      return (
        <div>
          <span data-testid="runs">{flattenPages(runs.data).length}</span>
          <button
            type="button"
            onClick={() =>
              runCompute.mutate({
                world_id: 'w1',
                module_id: 'basic-combat',
                invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
              })
            }
          >
            Run
          </button>
        </div>
      );
    }

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(await screen.findByText('1')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
    expect(receivedBody).toMatchObject({ world_id: 'w1', module_id: 'basic-combat' });
  });

  it('refetches the runs list when a run fails (daemon still records a Failed row)', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    useHandlers(
      http.get('/v1/daemon/compute/runs', () => listSpy()),
      http.post('/v1/daemon/compute/run', () =>
        HttpResponse.json(
          {
            success: false,
            error: {
              code: 'compute_module_error',
              message: 'manifest validation failed at key_blocks[0]',
              details: {},
              extensions: {},
            },
          },
          { status: 500 },
        ),
      ),
    );

    function Harness() {
      const runs = useComputeRuns();
      const runCompute = useRunCompute();
      return (
        <div>
          <span data-testid="runs">{flattenPages(runs.data).length}</span>
          <button
            type="button"
            onClick={() =>
              runCompute.mutate({
                world_id: 'w1',
                module_id: 'basic-combat',
                invocation_params: { attacker_id: 'kb-atk', defender_id: 'kb-def' },
              })
            }
          >
            Run
          </button>
        </div>
      );
    }

    renderInApp(<Harness />, { client: new BrowserClient() });
    expect(await screen.findByText('1')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /^run$/i }));
    // Mutation rejects (error envelope) but the runs lists are invalidated so
    // the server-recorded Failed row surfaces without a manual refresh.
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
  });
});

describe('useAcceptRun / useDiscardRun — runs-list + run-detail invalidation', () => {
  /**
   * Harness for the accept/discard invalidation contract. Mounts the runs
   * list + run detail (the mutation's direct targets) AND the Timeline
   * overview + World-KB graph consumers that must also refetch after an
   * Accept (qc1 W-001 / qc3 W-1: Accept mutates World + Timeline + KB
   * together; `refetchOnWindowFocus: false` means invalidation is the only
   * freshness path).
   */
  function renderRunInspector(
    button: 'accept' | 'discard',
    crossCache: {
      // Only the call is needed here; Vitest 4's bare `vi.fn()` overload types
      // as `Mock<Constructable | Procedure>`, which is not callable.
      timelineSpy: () => unknown;
      worldKbSpy: () => unknown;
    },
  ) {
    function Harness() {
      const runs = useComputeRuns();
      const run = useComputeRun('run_1');
      const acceptRun = useAcceptRun();
      const discardRun = useDiscardRun();
      useQuery({
        queryKey: queryKeys.timeline.overview(),
        queryFn: () => crossCache.timelineSpy(),
      });
      useQuery({
        queryKey: queryKeys.worldKb.graph('w1'),
        queryFn: () => crossCache.worldKbSpy(),
      });
      return (
        <div>
          <span data-testid="status">{run.data?.status ?? 'none'}</span>
          <span data-testid="runs">{flattenPages(runs.data).length}</span>
          {button === 'accept' ? (
            <button type="button" onClick={() => acceptRun.mutate({ runId: 'run_1' })}>
              Accept
            </button>
          ) : (
            <button type="button" onClick={() => discardRun.mutate('run_1')}>
              Discard
            </button>
          )}
        </div>
      );
    }
    renderInApp(<Harness />, { client: new BrowserClient() });
  }

  it('accept refetches the runs list, the run detail, the Timeline and the World KB graph', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    const detailSpy = vi.fn(() => HttpResponse.json(makeRun()));
    const timelineSpy = vi.fn(() => ({ eras: [], events: [] }));
    const worldKbSpy = vi.fn(() => ({ entities: [], source_anchors: [], relationships: [] }));
    useHandlers(
      http.get('/v1/daemon/compute/runs', () => listSpy()),
      http.get('/v1/daemon/compute/runs/:runId', () => detailSpy()),
      http.post('/v1/daemon/compute/runs/:runId/accept', ({ params }) =>
        HttpResponse.json({
          run_id: params.runId,
          status: 'applied',
          applied: { state_delta_count: 1, events_created: 1, new_entries_created: 0 },
          timeline_event_ids: ['evt_0'],
        }),
      ),
    );

    renderRunInspector('accept', { timelineSpy, worldKbSpy });
    expect(await screen.findByText('succeeded')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);
    expect(detailSpy).toHaveBeenCalledTimes(1);
    expect(timelineSpy).toHaveBeenCalledTimes(1);
    expect(worldKbSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /accept/i }));
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(detailSpy).toHaveBeenCalledTimes(2));
    // Cross-cache fan-out: the post-Accept World state must be fresh on the
    // Timeline and the KB graph without a manual reload.
    await waitFor(() => expect(timelineSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(worldKbSpy).toHaveBeenCalledTimes(2));
  });

  it('discard refetches the runs list, the run detail, the Timeline and the World KB graph', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    const detailSpy = vi.fn(() => HttpResponse.json(makeRun()));
    const timelineSpy = vi.fn(() => ({ eras: [], events: [] }));
    const worldKbSpy = vi.fn(() => ({ entities: [], source_anchors: [], relationships: [] }));
    useHandlers(
      http.get('/v1/daemon/compute/runs', () => listSpy()),
      http.get('/v1/daemon/compute/runs/:runId', () => detailSpy()),
      http.post('/v1/daemon/compute/runs/:runId/discard', ({ params }) =>
        HttpResponse.json({ run_id: params.runId, status: 'discarded' }),
      ),
    );

    renderRunInspector('discard', { timelineSpy, worldKbSpy });
    expect(await screen.findByText('succeeded')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);
    expect(detailSpy).toHaveBeenCalledTimes(1);
    expect(timelineSpy).toHaveBeenCalledTimes(1);
    expect(worldKbSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /discard/i }));
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(detailSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(timelineSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(worldKbSpy).toHaveBeenCalledTimes(2));
  });
});

describe('useClearRuns — cleared rows leave every cached view', () => {
  /**
   * The Runs table and the open Run inspector are the two views Clear history
   * must empty. The mutation is called with a per-call `onSuccess` exactly like
   * `run-studio.tsx` does (the toast copy), so this also pins that the
   * hook-level invalidation ordering keeps working at that call site.
   */
  function renderClearHarness(status: () => unknown) {
    function Harness() {
      const runs = useComputeRuns({ module_id: 'basic-combat', world_id: 'w1' });
      const run = useComputeRun('run_1');
      const clearRuns = useClearRuns();
      return (
        <div>
          <span data-testid="runs">{flattenPages(runs.data).length}</span>
          {/* The cleared Run's detail read fails (404). React Query keeps the
              last successful `data` through an error, so the honest consumer
              signal — and what the real studio renders from — is `isError`. */}
          <span data-testid="run">{run.isError ? 'error' : (run.data?.status ?? 'none')}</span>
          <button
            type="button"
            onClick={() =>
              clearRuns.mutate(
                { worldId: 'w1' },
                { onSuccess: () => status() },
              )
            }
          >
            Clear
          </button>
        </div>
      );
    }
    renderInApp(<Harness />, { client: new BrowserClient() });
  }

  it('drops the cleared row from the mounted runs list and run detail', async () => {
    // Server-side state the DELETE flips: the post-clear reads must disagree
    // with the pre-clear ones, so only a real refetch can move the mounted
    // consumers (call counting would pass on a spy echo).
    let cleared = false;
    useHandlers(
      http.get('/v1/daemon/compute/runs', () =>
        HttpResponse.json(
          cleared
            ? { items: [], has_more: false }
            : { items: [makeRun({ status: 'applied' })], has_more: false },
        ),
      ),
      http.get('/v1/daemon/compute/runs/:runId', () =>
        cleared
          ? HttpResponse.json(
              { success: false, error: { code: 'not_found', message: 'run run_1 not found' } },
              { status: 404 },
            )
          : HttpResponse.json(makeRun({ status: 'applied' })),
      ),
      http.delete('/v1/daemon/compute/runs', () => {
        cleared = true;
        return HttpResponse.json({ deleted: 1 });
      }),
    );

    renderClearHarness(() => undefined);
    await waitFor(() => expect(screen.getByTestId('runs')).toHaveTextContent('1'));
    await waitFor(() => expect(screen.getByTestId('run')).toHaveTextContent('applied'));

    fireEvent.click(screen.getByRole('button', { name: /clear/i }));

    // Without the invalidation the mounted views keep rendering the row the
    // server just deleted (observed against the real service: DOM kept both
    // rows while `GET /compute/runs` already returned `items: []`).
    await waitFor(() => expect(screen.getByTestId('runs')).toHaveTextContent('0'));
    // The refetched detail is a 404 now: the open Run's read is in its error
    // state (the real studio renders "Could not load this Run" from it).
    await waitFor(() => expect(screen.getByTestId('run')).toHaveTextContent('error'));
  });
});
