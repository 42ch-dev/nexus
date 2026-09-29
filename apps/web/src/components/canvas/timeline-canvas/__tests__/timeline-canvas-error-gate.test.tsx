/**
 * TimelineCanvas — World Moment outline-read error gate (001/R3, qc3 F-004).
 *
 * The Moment layer's canonical carrier is composed from the bound Works'
 * outline reads, so a failed read is NOT an empty outline. This pins the gate
 * the V1.200 DR-26 round-2 change introduced: with the World Timeline mounted
 * on the Moment layer, an msw 500 on `/v1/daemon/works/:workId/outline`
 * renders the retryable `ErrorState`, never the honest-empty panel, and the
 * retry refetches the failed read.
 *
 * Scope: the error gate only — no new gate semantics. The partial-failure and
 * per-Work pending shapes are covered by `moment-projection.test.tsx`
 * (`honest Moment bound-Work read states`).
 */
import { describe, expect, it } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';

import type { WorkOutline } from '@42ch/nexus-contracts';

import { BrowserClient } from '@/lib/nexus';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { TimelineCanvas } from '../timeline-canvas';

const BOUND_WORK_ID = 'work-a';

function boundWorkDetail() {
  return {
    work_id: BOUND_WORK_ID,
    status: 'active',
    title: `Work ${BOUND_WORK_ID}`,
    long_term_goal: '',
    initial_idea: '',
    intake_status: 'complete',
    world_id: 'world-7',
    inspiration_log: [],
    primary_preset_id: 'preset-a',
    schedule_ids: [],
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    current_stage: 'draft',
    stage_status: 'active',
    auto_chain_enabled: false,
    auto_chain_interrupted: false,
    auto_review_master_on_timeout: false,
    total_planned_chapters: 0,
    current_chapter: 0,
  };
}

function boundWorkOutline(sceneTitle: string): WorkOutline {
  return {
    work_id: BOUND_WORK_ID,
    outline_revision: 1,
    volumes: [],
    timeline_events: [],
    foreshadows: [],
    scenes: [{ scene_id: 'scn_a1', chapter_id: 1, title: sceneTitle, status: 'drafted' }],
    beats: [],
    chapter_titles: {},
    updated_at: '2026-08-01T00:00:00Z',
  };
}

describe('TimelineCanvas — World Moment outline-read error gate (001/R3)', () => {
  it('renders the retryable error gate — never the honest-empty panel — when a bound Work’s outline read fails', async () => {
    let outlineReads = 0;
    let outlineFails = true;
    useHandlers(
      // One era keeps the global empty branch away from the surface: the
      // assertions below are about the bound-Work read, not World emptiness.
      http.get('/v1/daemon/worlds/:worldId/kb/graph', () =>
        HttpResponse.json({
          entities: [
            {
              world_id: 'world-7',
              key_block_id: 'kb-era-1',
              block_type: 'era',
              canonical_name: 'The First Age',
              status: 'confirmed',
              version: 1,
            },
          ],
          source_anchors: [],
          relationships: [],
        }),
      ),
      http.get('/v1/daemon/worlds/:worldId/timeline/events', () =>
        HttpResponse.json({ items: [], has_more: false, next_cursor: undefined }),
      ),
      http.get('/v1/daemon/works', () =>
        HttpResponse.json({
          items: [
            {
              work_id: BOUND_WORK_ID,
              title: `Work ${BOUND_WORK_ID}`,
              status: 'active',
              intake_status: 'complete',
              primary_preset_id: 'preset-a',
              updated_at: '2026-01-01T00:00:00Z',
            },
          ],
          pagination: { limit: 100, has_more: false },
        }),
      ),
      http.get('/v1/daemon/works/:workId', () => HttpResponse.json(boundWorkDetail())),
      http.get('/v1/daemon/works/:workId/outline', () => {
        outlineReads += 1;
        return outlineFails
          ? HttpResponse.json({ message: 'boom' }, { status: 500 })
          : HttpResponse.json(boundWorkOutline('A one'));
      }),
      http.get('/v1/daemon/compute/modules', () =>
        HttpResponse.json({ items: [], has_more: false }),
      ),
    );

    renderInApp(<TimelineCanvas worldId="world-7" />, {
      client: new BrowserClient(),
      initialRouterEntries: ['/worlds/world-7/timeline?layer=moment'],
    });

    // The failed read owns the surface: error envelope + retry.
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Could not load the world timeline.');
    // A failed read is not an empty World — the honest-empty Moment panel must
    // not claim the bound Work has no scene/beat data.
    expect(screen.queryByTestId('timeline-moment-empty-state')).toBeNull();

    // Retry refetches the failed read; the settled outline renders and the
    // error gate clears.
    const readsBeforeRetry = outlineReads;
    outlineFails = false;
    await userEvent.setup().click(screen.getByRole('button', { name: 'Try again' }));

    await waitFor(() => expect(outlineReads).toBeGreaterThan(readsBeforeRetry));
    expect((await screen.findAllByText('A one')).length).toBeGreaterThan(0);
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });
});
