/**
 * WorkDetailPage canvas CTA tests — V1.108 FB-UI-009.
 *
 * The Work detail action row must expose canvas entry points alongside the
 * existing **Open World KB** link:
 *   - **Open Outline** → `/works/:workId/outline` (always, Work exists)
 *   - **Open Strategy** → `/strategies/:presetId` (gated by `primary_preset_id`)
 *   - **Open World KB** → `/worlds/:worldId/kb` (gated by `world_id`, existing)
 *
 * CTAs render as `Button asChild` + `Link`, so the anchor `href` is the routing
 * contract assertion (matches `App.tsx` route declarations).
 */
import { http, HttpResponse } from 'msw';
import { describe, expect, it } from 'vitest';
import { Route, Routes } from 'react-router-dom';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { workDetail } from '@/test/handlers';
import { BrowserClient } from '@/lib/nexus';
import { WorkDetailPage } from '@/pages/work-detail-page';

const client = () => new BrowserClient();

/** `GET /v1/daemon/works/:workId/findings` → empty list (keeps MSW quiet). */
const emptyFindings = () =>
  http.get('/v1/daemon/works/:workId/findings', () =>
    HttpResponse.json({ items: [], pagination: { limit: 20, has_more: false } }),
  );

/**
 * Minimal WorkDetailResponse the page renders without crashing. The shared
 * `workDetail` handler only seeds `work_id` + `title`; WorkDetailPage reads
 * `status`, `current_chapter`, etc. directly, so tests override the fields
 * under test on top of this base.
 */
function workDetailFixture(workId: string, over: Record<string, unknown> = {}) {
  return workDetail(workId, {
    status: 'draft',
    intake_status: 'completed',
    current_stage: 'drafting',
    stage_status: 'active',
    long_term_goal: 'Finish the book',
    initial_idea: 'A story',
    inspiration_log: [],
    schedule_ids: [],
    created_at: '2026-06-25T00:00:00Z',
    updated_at: '2026-06-25T00:00:00Z',
    current_chapter: 0,
    ...over,
  });
}

function renderWorkDetail(initialEntry = '/works/w-123') {
  return renderInApp(
    <Routes>
      <Route path="works/:workId" element={<WorkDetailPage />} />
    </Routes>,
    {
      client: client(),
      initialRouterEntries: [initialEntry],
    },
  );
}

describe('WorkDetailPage canvas CTAs (V1.108 FB-UI-009)', () => {
  it('renders Open Outline linking to /works/:workId/outline', async () => {
    useHandlers(
      workDetailFixture('w-123', {
        title: 'Canvas Work',
        primary_preset_id: 'preset-abc',
        world_id: 'world-xyz',
      }),
      emptyFindings(),
    );

    renderWorkDetail();

    const outline = await screen.findByRole('link', { name: /Open Outline/i });
    expect(outline).toHaveAttribute('href', '/works/w-123/outline');
  });

  it('renders Open Strategy linking to /strategies/:presetId when primary_preset_id is set', async () => {
    useHandlers(
      workDetailFixture('w-123', {
        title: 'Canvas Work',
        primary_preset_id: 'preset-abc',
        world_id: 'world-xyz',
      }),
      emptyFindings(),
    );

    renderWorkDetail();

    const strategy = await screen.findByRole('link', { name: /Open Strategy/i });
    expect(strategy).toHaveAttribute('href', '/strategies/preset-abc');
  });

  it('hides Open Strategy when primary_preset_id is absent', async () => {
    // Empty string = no preset bound (WorkDetailResponse types it as required
    // string, so "" is the "not set" sentinel at runtime).
    useHandlers(
      workDetailFixture('w-123', {
        title: 'No Preset Work',
        primary_preset_id: '',
        world_id: 'world-xyz',
      }),
      emptyFindings(),
    );

    renderWorkDetail();

    // Wait for the page to settle on a CTA that always renders.
    expect(await screen.findByRole('link', { name: /Open Outline/i })).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: /Open Strategy/i })).not.toBeInTheDocument();
  });

  it('retains Open World KB linking to /worlds/:worldId/kb when world_id is set', async () => {
    useHandlers(
      workDetailFixture('w-123', {
        title: 'Canvas Work',
        primary_preset_id: 'preset-abc',
        world_id: 'world-xyz',
      }),
      emptyFindings(),
    );

    renderWorkDetail();

    const worldKb = await screen.findByRole('link', { name: /Open World KB/i });
    expect(worldKb).toHaveAttribute('href', '/worlds/world-xyz/kb');
  });
});

describe('WorkDetailPage archive destructive context (V1.117 AC-P4-6)', () => {
  it('keeps the destructive Archive verb and names the Work on the confirm step', async () => {
    const user = userEvent.setup();
    let patchedBody: unknown = null;
    useHandlers(
      workDetailFixture('w-123', {
        title: 'Archivable Work',
        status: 'draft',
        primary_preset_id: 'preset-abc',
        world_id: 'world-xyz',
      }),
      emptyFindings(),
      http.patch('/v1/daemon/works/:workId', async ({ request }) => {
        patchedBody = await request.json();
        return HttpResponse.json({ work_id: 'w-123', status: 'archived' });
      }),
    );

    renderWorkDetail();

    // Arming click: visible Verb-only label "Archive", no object-bearing
    // accessible name yet (this step only arms; it is not the destructive act).
    const arm = await screen.findByRole('button', { name: /^Archive$/i });
    await user.click(arm);

    // Confirm step keeps the destructive verb visible ("Archive") and gives the
    // button an accessible name that carries the object ("Archive Work") so a
    // screen reader never hears a contextless word for the irreversible action.
    const confirm = await screen.findByRole('button', { name: /Archive Work/i });
    expect(confirm).toHaveTextContent('Archive');
    await user.click(confirm);

    await waitFor(() => expect(patchedBody).toEqual({ status: 'archived' }));
  });
});
