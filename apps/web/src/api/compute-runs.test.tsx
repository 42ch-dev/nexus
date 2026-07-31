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
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import {
  flattenPages,
  useAcceptRun,
  useComputeRun,
  useComputeRuns,
  useDiscardRun,
  useRunCompute,
} from '@/api/queries';
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
  function renderRunInspector(button: 'accept' | 'discard') {
    function Harness() {
      const runs = useComputeRuns();
      const run = useComputeRun('run_1');
      const acceptRun = useAcceptRun();
      const discardRun = useDiscardRun();
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

  it('accept refetches the runs list and the run detail', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    const detailSpy = vi.fn(() => HttpResponse.json(makeRun()));
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

    renderRunInspector('accept');
    expect(await screen.findByText('succeeded')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);
    expect(detailSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /accept/i }));
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(detailSpy).toHaveBeenCalledTimes(2));
  });

  it('discard refetches the runs list and the run detail', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makeRun()], has_more: false }),
    );
    const detailSpy = vi.fn(() => HttpResponse.json(makeRun()));
    useHandlers(
      http.get('/v1/daemon/compute/runs', () => listSpy()),
      http.get('/v1/daemon/compute/runs/:runId', () => detailSpy()),
      http.post('/v1/daemon/compute/runs/:runId/discard', ({ params }) =>
        HttpResponse.json({ run_id: params.runId, status: 'discarded' }),
      ),
    );

    renderRunInspector('discard');
    expect(await screen.findByText('succeeded')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);
    expect(detailSpy).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: /discard/i }));
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(detailSpy).toHaveBeenCalledTimes(2));
  });
});
