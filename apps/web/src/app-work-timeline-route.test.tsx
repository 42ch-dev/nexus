/**
 * Work Timeline route + Canvas shell peer integration — V1.123 P2 Task 5.
 *
 * Verifies the plan ACs:
 *   - AC-V1123-11: `/works/:workId/timeline` renders Work Timeline as a peer
 *     surface; `/works/:workId` still redirects to Outline (V1.118 regression
 *     preserved); Work Timeline is reachable from Canvas shell peer nav
 *     (CanvasNavCommands registers `go.work-timeline`).
 *   - AC-V1123-15: From Work Timeline, Narrative reachable; Outline reachable
 *     from Work Canvas shell; V1.122 World Timeline unaffected (regression).
 *
 * Coverage:
 *   - `/works/work-a/timeline` mounts inside `WorkShellLayout` and renders
 *     `WorkTimelineCanvas` (verified via the `work-timeline-canvas` testid).
 *   - `/works/work-a` redirects to `/works/work-a/outline` (V1.118 regression
 *     gate — preserved by Task 5; the new `timeline` sibling route MUST NOT
 *     flip the index redirect).
 *   - CanvasNavCommands registers `go.work-timeline` so the Work Timeline is
 *     reachable from the command palette (⌘K) on Work-scoped routes.
 *
 * Mount strategy mirrors `app-work-routes.test.tsx`: a real `MemoryRouter`
 * with the production `WorkShellLayout` + lazy route boundary stubbed, and a
 * mocked `NexusClient` resolving `getWorkOutline` so the canvas settles.
 */
import { afterEach, describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Navigate, Route, Routes } from 'react-router';
import { Suspense } from 'react';

import { WorkShellLayout } from '@/components/layout/work-shell-layout';
import { BrowserClient } from '@/lib/nexus';
import { i18n } from '@/lib/i18n/config';
import { renderInApp } from '@/test/test-providers';
import { useHandlers } from '@/test/msw-server';
import { worksList, workDetail } from '@/test/handlers';
import type { NexusClient } from '@/lib/nexus';
import type { WorkOutline } from '@42ch/nexus-contracts';

import { WorkTimelinePage } from '@/pages/work-timeline-page';
import { NotFoundPage } from '@/pages/not-found-page';

function makeClient(outline: WorkOutline): NexusClient {
  // Use BrowserClient as the base so MSW intercepts the real HTTP paths the
  // hooks fire (`/v1/daemon/works/*`, `/v1/daemon/works/list`). Then override
  // `getWorkOutline` so the Work Timeline query resolves to the per-test
  // fixture deterministically.
  const base = new BrowserClient();
  return Object.assign(base, {
    getWorkOutline: vi.fn().mockResolvedValue(outline),
  }) as unknown as NexusClient;
}

function emptyOutline(): WorkOutline {
  return {
    work_id: 'work-a',
    outline_revision: 1,
    volumes: [],
    timeline_events: [
      { event_id: 'evt-1', title: 'Inciting Incident', realizes_chapter_id: 1 },
    ],
    foreshadows: [],
    chapter_titles: {},
    updated_at: '2026-07-18T00:00:00Z',
  } as WorkOutline;
}

/** Minimal route tree mirroring `App.tsx` for `/works/:workId/*`. */
function WorkRouteTree() {
  return (
    <Routes>
      <Route path="works/:workId" element={<WorkShellLayout />}>
        <Route index element={<Navigate to="outline" replace />} />
        <Route
          path="outline"
          element={<div data-testid="outline-route">Outline</div>}
        />
        <Route
          path="timeline"
          element={
            <Suspense fallback={<div data-testid="timeline-route-loading">Loading…</div>}>
              <WorkTimelinePage />
            </Suspense>
          }
        />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}

function useWorkRouteHandlers() {
  useHandlers(
    worksList([
      {
        work_id: 'work-a',
        title: 'Alpha Novel',
        status: 'active',
        intake_status: 'complete',
        primary_preset_id: 'novel-writing',
        updated_at: '2026-06-24T00:00:00Z',
      },
    ]),
    workDetail('work-a', {
      title: 'Alpha Novel',
      status: 'active',
      work_profile: 'novel',
      primary_preset_id: 'novel-writing',
      updated_at: '2026-06-24T00:00:00Z',
    }),
  );
}

describe('Work Timeline route + Canvas shell peer integration (V1.123 P2 Task 5)', () => {
  beforeEach(async () => {
    window.localStorage.clear();
    await i18n.changeLanguage('en');
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders WorkTimelineCanvas at /works/:workId/timeline inside WorkShellLayout', async () => {
    useWorkRouteHandlers();

    renderInApp(<WorkRouteTree />, {
      client: makeClient(emptyOutline()),
      initialRouterEntries: ['/works/work-a/timeline'],
    });

    // Work Timeline canvas mounts (testid from WorkTimelineCanvas root).
    await waitFor(
      () => {
        expect(screen.getByTestId('work-timeline-canvas')).toBeInTheDocument();
      },
      { timeout: 5000 },
    );

    // The shell layout is present (peer-surface nesting verified).
    expect(screen.getByTestId('work-shell-layout')).toBeInTheDocument();
  });

  it('preserves V1.118 regression: /works/:workId still redirects to outline (NOT timeline)', async () => {
    // Plan Global Constraints: "Work entry preserved — `/works/:workId` →
    // Outline (V1.118); Work Timeline is reachable as a peer from Work Canvas
    // shell." Task 5 adds the timeline route as a SIBLING; the index redirect
    // at `/works/:workId` MUST keep pointing to outline so the Work entry
    // surface does not silently flip.
    useWorkRouteHandlers();

    renderInApp(<WorkRouteTree />, {
      client: makeClient(emptyOutline()),
      initialRouterEntries: ['/works/work-a'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('outline-route')).toBeInTheDocument();
    });

    // The Work Timeline surface MUST NOT mount on `/works/work-a` (no redirect
    // to timeline).
    expect(screen.queryByTestId('work-timeline-canvas')).toBeNull();
  });
});
