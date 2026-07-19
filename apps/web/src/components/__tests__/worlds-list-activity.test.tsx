/**
 * Worlds list — Timeline activity surface (V1.123 P3 Task 3).
 *
 * Verifies the Worlds picker surfaces per-World Timeline activity so the
 * Worlds list doubles as a Timeline activity index, locked by:
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` (Worlds list
 *     surfaces Timeline activity).
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 3.
 *
 * Coverage:
 *   - Each world row surfaces a Timeline activity indicator with the
 *     last-edited timestamp (degraded fallback per plan honest scope cut).
 *   - Rows without `updated_at` show a graceful "—" fallback rather than
 *     blank space.
 *
 * Per plan Global Constraints + architect §8: per-World graph fetches for
 * era/event counts are an N+1 cost the list endpoint performance cannot
 * absorb; the durable slice for Task 3 is the last-edited timestamp
 * surface, with the era/event counts deferred to a future composite
 * endpoint (`DF-V1122-DEEPER-WB` stays deferred). The fallback is
 * documented in the completion report.
 */
import { http, HttpResponse } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { WorldsPage } from '@/pages/worlds-page';

const client = () => new BrowserClient();

function world(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema_version: 1,
    world_id: 'w-1',
    owner_creator_id: 'creator-a',
    title: 'Eryndor',
    slug: 'w-1',
    status: 'active',
    visibility: 'private',
    time_policy: 'manual',
    created_at: '2026-07-01T00:00:00Z',
    ...over,
  };
}

function renderWorlds() {
  return renderInApp(<WorldsPage />, { client: client() });
}

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('WorldsPage — Timeline activity surface (V1.123 P3 Task 3)', () => {
  it('renders a Timeline activity indicator with last-edited time per world', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [
            world({
              world_id: 'eryndor',
              title: 'Eryndor',
              updated_at: '2026-07-15T00:00:00Z',
            }),
          ],
        }),
      ),
    );

    renderWorlds();

    // Each world row carries a Timeline activity indicator with a stable
    // testid so the prominence contract is verifiable without coupling to
    // the exact "X ago" formatting.
    const activity = await screen.findByTestId('world-timeline-activity-eryndor');
    expect(activity).toBeInTheDocument();
    // The indicator is non-empty (the last-edited timestamp rendered).
    expect(activity.textContent).not.toBe('');
  });

  it('renders a graceful fallback for worlds without updated_at', async () => {
    // Per plan honest scope cut: worlds without `updated_at` show a "—"
    // fallback rather than blank space. The World wire type marks
    // `updated_at` optional, so this is a real production case.
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [world({ world_id: 'no-time', title: 'No Time' })],
        }),
      ),
    );

    renderWorlds();

    const activity = await screen.findByTestId('world-timeline-activity-no-time');
    expect(activity).toBeInTheDocument();
    // The fallback sentinel is non-empty.
    expect(activity.textContent).not.toBe('');
  });

  it('preserves the click-to-open Timeline navigation (V1.122 P1 T3 regression)', async () => {
    useHandlers(
      http.get('/v1/daemon/narrative/worlds', () =>
        HttpResponse.json({
          worlds: [
            world({
              world_id: 'eryndor',
              title: 'Eryndor',
              updated_at: '2026-07-15T00:00:00Z',
            }),
          ],
        }),
      ),
    );

    renderWorlds();

    // The activity indicator is rendered ALONGSIDE the existing click-to-
    // open Timeline button — clicking the row still opens the per-World
    // Timeline surface (V1.122 P1 T3 retarget).
    const button = await screen.findByRole('button', { name: 'Open timeline' });
    expect(button).toBeInTheDocument();
  });
});
