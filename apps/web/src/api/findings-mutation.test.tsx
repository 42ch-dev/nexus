/**
 * useUpdateFinding — optimistic update + invalidation contract.
 *
 * Against a real BrowserClient + msw: the mutation optimistically patches the
 * finding in the cached findings list before the server responds, then
 * invalidates the list on settle so it refetches with server truth. The PATCH
 * is held in-flight with a deferred response so the optimistic window is
 * observable (not racy).
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { useFindings, useUpdateFinding, flattenPages } from '@/api/queries';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { FindingDetailResponse } from '@42ch/nexus-contracts';

function makeFinding(over: Partial<FindingDetailResponse> = {}): FindingDetailResponse {
  return {
    finding_id: 'f1',
    work_id: 'w1',
    chapter: 1,
    severity: 'major',
    status: 'open',
    title: 'Pacing',
    description: 'd',
    target_executor: 'none',
    kind: 'k',
    created_at: 1,
    updated_at: 1,
    ...over,
  };
}

/** Harness exposing the first finding's status + a mutate trigger. */
function Harness({ onMutate }: { onMutate: () => void }) {
  const findings = useFindings('w1');
  const updateFinding = useUpdateFinding();
  const first = flattenPages(findings.data)[0];
  return (
    <div>
      <span data-testid="status">{first?.status ?? 'none'}</span>
      <button
        type="button"
        onClick={() => {
          updateFinding.mutate({ workId: 'w1', findingId: 'f1', patch: { status: 'triaged' } });
          onMutate();
        }}
      >
        Triage
      </button>
    </div>
  );
}

describe('useUpdateFinding — optimistic update + invalidation', () => {
  it('optimistically patches the cached finding while the PATCH is in-flight', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({
        items: [makeFinding({ status: 'open' })],
        pagination: { limit: 20, has_more: false },
      }),
    );
    // Deferred gate: the PATCH stays in-flight until we release it, so the
    // optimistic window is observable (not racy).
    let releasePatch!: () => void;
    const patchGate = new Promise<void>((resolve) => {
      releasePatch = resolve;
    });
    let patchBody: unknown = null;
    useHandlers(
      http.get('/v1/local/works/:workId/findings', () => listSpy()),
      http.patch('/v1/local/works/:workId/findings/:findingId', async ({ request }) => {
        patchBody = await request.json();
        await patchGate;
        return HttpResponse.json(makeFinding({ status: 'triaged', updated_at: 2 }));
      }),
    );

    const client = new BrowserClient();
    renderInApp(<Harness onMutate={() => {}} />, { client });

    // Initial load shows the open finding (findByText waits for the query).
    expect(await screen.findByText('open')).toBeInTheDocument();
    expect(listSpy).toHaveBeenCalledTimes(1);

    // Trigger the mutation. onMutate applies the optimistic patch to the cached
    // list (after its cancelQueries await), so the status flips while the PATCH
    // is still in-flight (patchInFlight holds the response open).
    fireEvent.click(screen.getByRole('button', { name: /triage/i }));
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('triaged'));

    // The PATCH carried the status transition payload.
    expect(patchBody).toEqual({ status: 'triaged' });

    // Now release the server response. onSettled invalidates the findings list,
    // triggering a refetch.
    releasePatch();
    await waitFor(() => {
      expect(listSpy).toHaveBeenCalledTimes(2);
    });
  });

  it('rolls back the optimistic patch when the server rejects the transition', async () => {
    // Initial list: open finding. The PATCH is rejected with 422
    // INVALID_TRANSITION (an illegal transition bypassed the UI guards).
    let listCount = 0;
    useHandlers(
      http.get('/v1/local/works/:workId/findings', () => {
        listCount += 1;
        return HttpResponse.json({
          items: [makeFinding({ status: 'open' })],
          pagination: { limit: 20, has_more: false },
        });
      }),
      http.patch('/v1/local/works/:workId/findings/:findingId', () =>
        HttpResponse.json(
          { success: false, error: { code: 'INVALID_TRANSITION', message: 'illegal' } },
          { status: 422 },
        ),
      ),
    );

    const client = new BrowserClient();
    renderInApp(<Harness onMutate={() => {}} />, { client });

    expect(await screen.findByText('open')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /triage/i }));

    // The optimistic patch flips to triaged, then onError rolls back to the
    // snapshotted open state once the 422 lands.
    await waitFor(() => expect(screen.getByTestId('status')).toHaveTextContent('open'));
    // onSettled also invalidates → refetch.
    await waitFor(() => expect(listCount).toBeGreaterThanOrEqual(2));
  });

  it('does not invalidate other Works findings lists on settle (qc3 W-QC3-P0-001)', async () => {
    // A mutation on w1 must refetch w1's list (a status transition can move a
    // finding between filter views of w1) but must NOT touch w2's list. The
    // global findings-list prefix is no longer invalidated.
    const listSpies: Record<string, ReturnType<typeof vi.fn>> = {};
    const listFor = (workId: string) => {
      listSpies[workId] ??= vi.fn(() =>
        HttpResponse.json({
          items: [makeFinding({ work_id: workId, status: 'open' })],
          pagination: { limit: 20, has_more: false },
        }),
      );
      return listSpies[workId];
    };
    useHandlers(
      http.get('/v1/local/works/:workId/findings', ({ params }) =>
        listFor(String(params.workId))(),
      ),
      http.patch('/v1/local/works/:workId/findings/:findingId', ({ params }) =>
        HttpResponse.json(
          makeFinding({
            work_id: String(params.workId),
            finding_id: 'f1',
            status: 'triaged',
            updated_at: 2,
          }),
        ),
      ),
    );

    function TwoWorkHarness() {
      const w1 = useFindings('w1');
      const w2 = useFindings('w2');
      const updateFinding = useUpdateFinding();
      return (
        <div>
          <span data-testid="w1-status">{flattenPages(w1.data)?.[0]?.status ?? 'none'}</span>
          <span data-testid="w2-status">{flattenPages(w2.data)?.[0]?.status ?? 'none'}</span>
          <button
            type="button"
            onClick={() =>
              updateFinding.mutate({ workId: 'w1', findingId: 'f1', patch: { status: 'triaged' } })
            }
          >
            Triage w1
          </button>
        </div>
      );
    }

    const client = new BrowserClient();
    renderInApp(<TwoWorkHarness />, { client });

    // Both lists load exactly once.
    await waitFor(() => expect(screen.getByTestId('w1-status')).toHaveTextContent('open'));
    await waitFor(() => expect(screen.getByTestId('w2-status')).toHaveTextContent('open'));
    expect(listSpies['w1']).toHaveBeenCalledTimes(1);
    expect(listSpies['w2']).toHaveBeenCalledTimes(1);

    // Mutate a finding in w1. The optimistic patch flips w1's status.
    fireEvent.click(screen.getByRole('button', { name: /triage w1/i }));
    await waitFor(() => expect(screen.getByTestId('w1-status')).toHaveTextContent('triaged'));

    // On settle, w1's list is invalidated → refetched. TanStack processes every
    // query invalidated by the same invalidation call in one batch, so by the
    // time w1 has been called a second time, w2's fate is sealed too.
    await waitFor(() => expect(listSpies['w1']).toHaveBeenCalledTimes(2));
    expect(listSpies['w2']).toHaveBeenCalledTimes(1);
  });
});
