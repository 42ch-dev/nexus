/**
 * SoulPanel integration tests (V1.79 P1 — Track B §D).
 *
 * Renders the full MemoryPage against msw handlers that vary the fragments list
 * to exercise the three density states (empty / low-data / rich) and the
 * click-to-filter affordance. The pure aggregation math is covered separately
 * in soul-stats.test.ts; these tests assert the rendered SOUL section behaves
 * per plan §F (empathetic copy, no broken single-point charts, growth folded in).
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';

import { BrowserClient } from '@/lib/nexus';
import { MemoryPage } from '@/pages/memory-page';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';

const CREATOR = 'creator-active';

function sessionListHandler(creatorId = CREATOR) {
  return http.get('/v1/local/orchestration/sessions', () =>
    HttpResponse.json({
      items: [{ session_id: 's1', creator_id: creatorId, preset_id: 'p', status: 'completed' }],
      pagination: { limit: 1, has_more: false },
    }),
  );
}

/** Wire the non-SOUL memory handlers as no-ops so MemoryPage fully renders. */
function baselineMemoryHandlers(fragments: ReturnType<typeof jsonFragments>) {
  return [
    http.get('/v1/local/memory/pending-review', () =>
      HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
    ),
    http.get('/v1/local/memory/pending-review/count', () => HttpResponse.json({ count: 0 })),
    http.get('/v1/local/memory/fragments', () => HttpResponse.json({ fragments })),
  ];
}

function jsonFragments(
  count: number,
  opts: { keywords?: string[]; spreadDays?: number } = {},
) {
  const { keywords = ['historical fiction'], spreadDays = 1 } = opts;
  return Array.from({ length: count }, (_, i) => ({
    fragment_id: `f${i}`,
    summary: `fragment ${i}`,
    keywords,
    // Spread fragments across `spreadDays` distinct days for temporal bucketing.
    created_at: `2026-06-${String(1 + (i % spreadDays)).padStart(2, '0')}T00:00:00Z`,
  }));
}

describe('MemoryPage SOUL section — density states + click-to-filter', () => {
  it('renders the empathetic empty state when there are zero fragments', async () => {
    useHandlers(sessionListHandler(), ...baselineMemoryHandlers([]));

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    expect(await screen.findByTestId('soul-empty-state')).toBeInTheDocument();
    expect(screen.getByText(/your soul is just beginning/i)).toBeInTheDocument();
    // No chart must render in the empty state.
    expect(screen.queryByTestId('soul-keyword-frequency')).not.toBeInTheDocument();
  });

  it('renders the low-data frequency list with the live count', async () => {
    useHandlers(
      sessionListHandler(),
      ...baselineMemoryHandlers(
        jsonFragments(3, { keywords: ['moral ambiguity', 'ensemble casts'] }),
      ),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    expect(await screen.findByTestId('soul-low-data')).toBeInTheDocument();
    expect(screen.getByText(/3 fragments captured so far/i)).toBeInTheDocument();
    // Frequency rows render the keyword + count.
    expect(await screen.findByTestId('soul-keyword-frequency')).toBeInTheDocument();
    expect(screen.getByText('moral ambiguity')).toBeInTheDocument();
  });

  it('renders the rich timeline with growth folded in', async () => {
    useHandlers(
      sessionListHandler(),
      ...baselineMemoryHandlers(
        jsonFragments(22, { keywords: ['political intrigue', 'slow-burn romance'], spreadDays: 6 }),
      ),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    expect(await screen.findByTestId('soul-rich')).toBeInTheDocument();
    // The temporal drift timeline renders with >=2 buckets and a cumulative total.
    expect(screen.getByTestId('soul-temporal-drift')).toBeInTheDocument();
    expect(screen.getByText(/22 fragments captured over time/i)).toBeInTheDocument();
  });

  it('clicks a SOUL keyword to filter the fragments browser', async () => {
    useHandlers(
      sessionListHandler(),
      ...baselineMemoryHandlers(
        jsonFragments(4, { keywords: ['political intrigue', 'ensemble casts'] }),
      ),
    );

    renderInApp(<MemoryPage />, { client: new BrowserClient() });

    // Wait for the SOUL frequency row to render, then click it.
    const row = await screen.findByText('political intrigue');
    fireEvent.click(row);

    // The lifted fragment-keyword state drives the fragments browser input.
    await waitFor(() =>
      expect(screen.getByLabelText(/filter by keyword/i)).toHaveValue('political intrigue'),
    );
  });
});
