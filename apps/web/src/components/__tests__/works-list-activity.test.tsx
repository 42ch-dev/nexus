/**
 * Works list — Timeline activity surface (V1.123 P3 Task 3).
 *
 * Verifies the Works dashboard surfaces per-Work Timeline activity so the
 * Works list doubles as a Timeline activity index, locked by:
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` (Works list
 *     surfaces Timeline activity).
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 3.
 *
 * Coverage:
 *   - Each work row exposes a "View Timeline" link targeting
 *     `/works/<workId>/timeline` (the V1.123 P2 T5 peer surface).
 *   - The existing Updated column still surfaces the last-edited time
 *     (V1.64 regression — already a Timeline activity signal).
 *
 * Per plan Global Constraints + architect §8: per-Work outline fetches for
 * `timeline_events.length` are an N+1 cost the list endpoint cannot absorb;
 * the durable slice for Task 3 is the existing `updated_at` column (already
 * a Timeline activity proxy) plus a per-row Timeline link. Per-Work event
 * counts are deferred to a future composite endpoint (`DF-V1123-MOMENT-WIRE`
 * stays deferred). The fallback is documented in the completion report.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { WorksPage } from '@/pages/works-page';

const client = () => new BrowserClient();

function renderWorks() {
  return renderInApp(<WorksPage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('WorksPage — Timeline activity surface (V1.123 P3 Task 3)', () => {
  it('renders a View Timeline link per work targeting the Work Timeline route', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    // Wait for the table to settle, then assert the per-row Timeline link.
    const timelineLink = await screen.findByTestId('work-timeline-link-w-123');
    expect(timelineLink).toBeInTheDocument();
    expect(timelineLink).toHaveAttribute(
      'href',
      '/works/w-123/timeline',
    );
  });

  it('renders a Timeline column header so the activity surface is discoverable', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    // The Timeline column header renders as a peer to the existing Updated
    // column so the activity surface is discoverable in the table chrome.
    expect(await screen.findByText('Timeline')).toBeInTheDocument();
  });

  it('preserves the existing Updated column (V1.64 regression)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w-123',
              title: 'Galaxy Novel',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    // The Updated column header still renders alongside the new Timeline
    // column (the Updated column was already a Timeline activity proxy via
    // `formatRelative(updated_at)`).
    expect(await screen.findByText('Updated')).toBeInTheDocument();
    // Sanity: the new Timeline column header is also present.
    expect(screen.getByText('Timeline')).toBeInTheDocument();
  });

  it('encodes the work id in the Timeline link (space-bearing ids stay one segment)', async () => {
    useHandlers(
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: 'w 42',
              title: 'Spaced',
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'novel-writing',
              updated_at: '2026-06-24T00:00:00Z',
            },
          ],
          pagination: { limit: 20, has_more: false },
        }),
      ),
      http.get('/v1/daemon/runtime/health', () =>
        HttpResponse.json({ status: 'ok', version: 'test' }),
      ),
    );

    renderWorks();

    const link = await screen.findByTestId('work-timeline-link-w 42');
    expect(link).toHaveAttribute('href', '/works/w%2042/timeline');
  });
});
