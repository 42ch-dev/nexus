/**
 * MemoryPage render + interaction tests — V1.78 Creator Memory review-loop.
 *
 * Covers the three affordances against a real BrowserClient + msw: the
 * pending-review list with live count badge, the read-only fragments browser
 * (incl. the absent-world_id "(none)" legibility rule, open item #3), and the
 * delete + review flows. The active creator id is derived from sessions
 * (mirrors the canvas `useDerivedCreatorId` derivation).
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it, vi } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { MemoryPage } from '@/pages/memory-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const CREATOR = 'creator-active';

/** Wire the sessions list so `useActiveCreatorId` resolves the active creator. */
function sessionListHandler(creatorId = CREATOR) {
  return http.get('/v1/local/orchestration/sessions', ({ request }) => {
    // useActiveCreatorId issues listSessions({ limit: 1 }); echo a session row
    // carrying the creator_id so the derivation resolves.
    const url = new URL(request.url);
    void url;
    return HttpResponse.json({
      items: [{ session_id: 's1', creator_id: creatorId, preset_id: 'p', status: 'completed' }],
      pagination: { limit: 1, has_more: false },
    });
  });
}

describe('MemoryPage — pending list, count badge, fragments, delete, review', () => {
  it('renders the pending-review list with the live count badge', async () => {
    useHandlers(
      sessionListHandler(),
      http.get('/v1/local/memory/pending-review', () =>
        HttpResponse.json({
          items: [
            {
              pending_id: 'p1',
              session_id: 's1',
              creator_id: CREATOR,
              task_kind: 'brainstorm',
              raw_digest: 'A captured thought.',
              created_at: '2026-07-01T09:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/local/memory/pending-review/count', () => HttpResponse.json({ count: 1 })),
      http.get('/v1/local/memory/fragments', () => HttpResponse.json({ fragments: [] })),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    // Count badge renders the live count (memory-pending-count token).
    await waitFor(() =>
      expect(screen.getByTestId('memory-pending-count')).toHaveTextContent('1'),
    );
    // The pending row's raw digest + humanized task_kind render.
    expect(await screen.findByText('A captured thought.')).toBeInTheDocument();
    expect(screen.getByText('Brainstorm')).toBeInTheDocument();
    // Fragments empty state renders (read-only browser).
    expect(await screen.findByText('No fragments')).toBeInTheDocument();
  });

  it('renders fragments and shows "(none)" for absent world_id in the inspector (open item #3)', async () => {
    useHandlers(
      sessionListHandler(),
      http.get('/v1/local/memory/pending-review', () =>
        HttpResponse.json({
          items: [
            {
              pending_id: 'p2',
              session_id: 's2',
              creator_id: CREATOR,
              world_id: undefined,
              task_kind: 'chapter',
              raw_digest: 'digest-2',
              created_at: '2026-07-01T09:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/local/memory/pending-review/count', () => HttpResponse.json({ count: 1 })),
      http.get('/v1/local/memory/fragments', () =>
        HttpResponse.json({
          fragments: [{ fragment_id: 'f1', summary: 'A long-term memory fragment.' }],
        }),
      ),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    // Fragment row renders fragment_id (monospace) + summary.
    expect(await screen.findByText('A long-term memory fragment.')).toBeInTheDocument();
    expect(screen.getByText('f1')).toBeInTheDocument();

    // Open the inspector by selecting the pending row.
    fireEvent.click(screen.getByText('digest-2'));
    // open item #3: absent world_id reads as "(none)".
    expect(await screen.findByTestId('memory-world-id')).toHaveTextContent('(none)');
    expect(screen.getByTestId('memory-raw-digest')).toHaveTextContent('digest-2');
  });

  it('deletes a pending review via the row action and removes it from the list', async () => {
    const deleteSpy = vi.fn(() => HttpResponse.json({ success: true, pending_id: 'p1' }));
    // Stateful list: returns the row until the DELETE lands, then empty —
    // mirroring a real server that honors the deletion before the refetch.
    let deleted = false;
    useHandlers(
      sessionListHandler(),
      http.get('/v1/local/memory/pending-review', () =>
        HttpResponse.json({
          items: deleted
            ? []
            : [
                {
                  pending_id: 'p1',
                  session_id: 's1',
                  creator_id: CREATOR,
                  task_kind: 'research',
                  raw_digest: 'to be deleted',
                  created_at: '2026-07-01T09:00:00Z',
                },
              ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/local/memory/pending-review/count', () =>
        HttpResponse.json({ count: deleted ? 0 : 1 }),
      ),
      http.get('/v1/local/memory/fragments', () => HttpResponse.json({ fragments: [] })),
      http.delete('/v1/local/memory/pending-review/:pendingId', () => {
        deleted = true;
        return deleteSpy();
      }),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });
    expect(await screen.findByText('to be deleted')).toBeInTheDocument();

    // window.confirm is the web-friendly confirmation (D-UX LOCKED).
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    fireEvent.click(screen.getByRole('button', { name: /delete pending review p1/i }));

    await waitFor(() => expect(deleteSpy).toHaveBeenCalledTimes(1));
    // Optimistic update removes the row; the refetch confirms the empty list.
    await waitFor(() => expect(screen.queryByText('to be deleted')).not.toBeInTheDocument());
    confirmSpy.mockRestore();
  });

  it('runs Review & Summarize and surfaces the result-counters toast', async () => {
    const reviewSpy = vi.fn(() =>
      HttpResponse.json({ promoted: 3, fragmented: 5, dropped: 2 }),
    );
    useHandlers(
      sessionListHandler(),
      http.get('/v1/local/memory/pending-review', () =>
        HttpResponse.json({
          items: [
            {
              pending_id: 'p1',
              session_id: 's1',
              creator_id: CREATOR,
              task_kind: 'outline',
              raw_digest: 'review me',
              created_at: '2026-07-01T09:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/local/memory/pending-review/count', () => HttpResponse.json({ count: 1 })),
      http.get('/v1/local/memory/fragments', () => HttpResponse.json({ fragments: [] })),
      http.post('/v1/local/memory/review', reviewSpy),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });
    // CTA is enabled once pending count resolves (> 0).
    const cta = await screen.findByRole('button', { name: /review and summarize/i });
    await waitFor(() => expect(cta).not.toBeDisabled());

    fireEvent.click(cta);

    await waitFor(() => expect(reviewSpy).toHaveBeenCalledTimes(1));
    // Result-counters confirmation toast (ReviewResponse promoted/fragmented/dropped).
    expect(await screen.findByText(/3 promoted to long-term memory/)).toBeInTheDocument();
    expect(screen.getByText(/5 saved as fragments/)).toBeInTheDocument();
    expect(screen.getByText(/2 dropped/)).toBeInTheDocument();
  });

  it('shows the no-active-creator empty state when no sessions exist', async () => {
    useHandlers(
      http.get('/v1/local/orchestration/sessions', () =>
        HttpResponse.json({ items: [], pagination: { limit: 1, has_more: false } }),
      ),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    expect(await screen.findByText('No active creator')).toBeInTheDocument();
  });
});
