/**
 * useDeletePendingReview + useReviewMemory — optimistic update, invalidation,
 * and result-toast contract for the V1.78 Creator Memory review-loop.
 *
 * Against a real BrowserClient + msw:
 *  - `useDeletePendingReview` optimistically drops the row from the cached
 *    pending-review list AND decrements the count badge before the server
 *    responds, then invalidates pending-list + count + fragments on settle.
 *  - `useReviewMemory` surfaces the result counters in a toast and invalidates
 *    pending-list + count + fragments on success.
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import {
  flattenPages,
  useDeletePendingReview,
  useMemoryFragments,
  usePendingReviewCount,
  usePendingReviews,
  useReviewMemory,
} from '@/api/queries';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import type { PendingReviewInfo } from '@42ch/nexus-contracts';

const CREATOR = 'c1';

function makePending(over: Partial<PendingReviewInfo> = {}): PendingReviewInfo {
  return {
    pending_id: 'p1',
    session_id: 's1',
    creator_id: CREATOR,
    world_id: undefined,
    task_kind: 'brainstorm',
    raw_digest: 'digest body',
    created_at: '2026-07-01T00:00:00Z',
    ...over,
  };
}

/** Harness exposing the first pending row + count + a delete trigger. */
function DeleteHarness({ onMutate }: { onMutate: () => void }) {
  const reviews = usePendingReviews(CREATOR);
  const count = usePendingReviewCount(CREATOR);
  const deleteReview = useDeletePendingReview();
  const first = flattenPages(reviews.data)[0];
  return (
    <div>
      <span data-testid="count">{count.data?.count ?? 'none'}</span>
      <span data-testid="first-id">{first?.pending_id ?? 'none'}</span>
      <button
        type="button"
        onClick={() => {
          deleteReview.mutate({ pendingId: 'p1', creatorId: CREATOR });
          onMutate();
        }}
      >
        Delete
      </button>
    </div>
  );
}

describe('useDeletePendingReview — optimistic remove + count decrement + invalidation', () => {
  it('drops the row and decrements the count before the server responds', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({
        items: [makePending()],
        pagination: { limit: 20, has_more: false },
      }),
    );
    const countSpy = vi.fn(() => HttpResponse.json({ count: 3 }));
    // Deferred gate so the optimistic window is observable (not racy).
    let releaseDelete!: () => void;
    const deleteGate = new Promise<void>((resolve) => {
      releaseDelete = resolve;
    });
    let deleteQuery: string | null = null;
    useHandlers(
      http.get('/v1/local/memory/pending-review', () => listSpy()),
      http.get('/v1/local/memory/pending-review/count', () => countSpy()),
      http.delete('/v1/local/memory/pending-review/:pendingId', async ({ request }) => {
        deleteQuery = new URL(request.url).searchParams.get('creator_id');
        await deleteGate;
        return HttpResponse.json({ success: true, pending_id: 'p1' });
      }),
    );

    const client = new BrowserClient();
    renderInApp(<DeleteHarness onMutate={() => {}} />, { client });

    // Initial load: count=3, row p1 present.
    await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('3'));
    await waitFor(() => expect(screen.getByTestId('first-id')).toHaveTextContent('p1'));

    // Trigger delete. The optimistic update drops the row and decrements the
    // count (3 → 2) while the DELETE is still in-flight.
    fireEvent.click(screen.getByRole('button', { name: /delete/i }));
    await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('2'));
    await waitFor(() => expect(screen.getByTestId('first-id')).toHaveTextContent('none'));

    // The DELETE carried creator_id as a query param (creator-scoped).
    await waitFor(() => expect(deleteQuery).toBe(CREATOR));

    // Release the server response. onSettled invalidates pending-list + count,
    // triggering refetches.
    releaseDelete();
    await waitFor(() => expect(listSpy.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => expect(countSpy.mock.calls.length).toBeGreaterThanOrEqual(2));
  });

  it('rolls back the row + count when the server rejects the delete', async () => {
    let listCount = 0;
    useHandlers(
      http.get('/v1/local/memory/pending-review', () => {
        listCount += 1;
        return HttpResponse.json({
          items: [makePending()],
          pagination: { limit: 20, has_more: false },
        });
      }),
      http.get('/v1/local/memory/pending-review/count', () => HttpResponse.json({ count: 1 })),
      http.delete('/v1/local/memory/pending-review/:pendingId', () =>
        HttpResponse.json(
          { success: false, error: { code: 'not_found', message: 'gone' } },
          { status: 404 },
        ),
      ),
    );

    const client = new BrowserClient();
    renderInApp(<DeleteHarness onMutate={() => {}} />, { client });

    await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('1'));
    await waitFor(() => expect(screen.getByTestId('first-id')).toHaveTextContent('p1'));

    fireEvent.click(screen.getByRole('button', { name: /delete/i }));

    // The optimistic drop flashes, then onError restores the row + count once
    // the 404 lands.
    await waitFor(() => expect(screen.getByTestId('first-id')).toHaveTextContent('p1'));
    await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('1'));
    // onSettled also invalidates → refetch.
    await waitFor(() => expect(listCount).toBeGreaterThanOrEqual(2));
  });
});

describe('useReviewMemory — result counters + invalidation', () => {
  it('invalidates pending-list + count + fragments on success', async () => {
    const listSpy = vi.fn(() =>
      HttpResponse.json({ items: [makePending()], pagination: { limit: 20, has_more: false } }),
    );
    const countSpy = vi.fn(() => HttpResponse.json({ count: 1 }));
    const fragmentsSpy = vi.fn(() => HttpResponse.json({ fragments: [] }));

    let reviewBody: unknown = null;
    useHandlers(
      http.get('/v1/local/memory/pending-review', () => listSpy()),
      http.get('/v1/local/memory/pending-review/count', () => countSpy()),
      http.get('/v1/local/memory/fragments', () => fragmentsSpy()),
      http.post('/v1/local/memory/review', async ({ request }) => {
        reviewBody = await request.json();
        return HttpResponse.json({ promoted: 2, fragmented: 1, dropped: 0 });
      }),
    );

    function ReviewHarness() {
      const reviews = usePendingReviews(CREATOR);
      const count = usePendingReviewCount(CREATOR);
      const fragments = useMemoryFragments(CREATOR);
      const reviewMemory = useReviewMemory();
      // Touch all three queries so they subscribe (mount) before the mutation.
      void reviews.data;
      void count.data;
      void fragments.data;
      return (
        <button type="button" onClick={() => reviewMemory.mutate(CREATOR)}>
          Review
        </button>
      );
    }

    const client = new BrowserClient();
    renderInApp(<ReviewHarness />, { client });

    // Wait for the initial fetches to land.
    await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(fragmentsSpy).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByRole('button', { name: /review/i }));

    // The POST carried { creator_id } in the body.
    await waitFor(() => expect(reviewBody).toEqual({ creator_id: CREATOR }));

    // onSuccess invalidates pending-list + count + fragments → all refetch.
    await waitFor(() => expect(listSpy.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => expect(countSpy.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => expect(fragmentsSpy.mock.calls.length).toBeGreaterThanOrEqual(2));

    // The result-counters toast surfaces the promoted/fragmented/dropped counts.
    await screen.findByText(/2 promoted to long-term memory/);
  });
});
