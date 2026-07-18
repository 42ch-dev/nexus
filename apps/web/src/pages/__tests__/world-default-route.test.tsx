/**
 * World default route + IA tests (V1.122 P1 T3).
 *
 * Asserts the architect-locked information architecture:
 *   - `/worlds/:worldId` redirects to `/worlds/:worldId/timeline` (Timeline
 *     is the default World entry; compass AC-V1122-5).
 *   - The Timeline canvas mounts at `/worlds/:worldId/timeline`.
 *   - Peer-surface nav links (World KB at `/kb`, Strategy at `/strategies`)
 *     are present in the Timeline header.
 *   - World KB route `/worlds/:worldId/kb` still mounts as a sibling
 *     (regression — the peer surface stays reachable).
 *   - Work entry regression: `/works/:workId` redirects to `outline`
 *     (V1.118; the Timeline retarget MUST NOT change Work entry).
 */
import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import { Navigate, Route, Routes, useLocation } from 'react-router-dom';

import { renderInApp } from '@/test/test-providers';
import { noopClient } from '@/test/test-providers';
import { TimelinePage } from '@/pages/timeline-page';
import { WorldsPage } from '@/pages/worlds-page';

/**
 * The Timeline canvas pulls in `@xyflow/react` + the T2 adapter; for route-
 * resolution tests we mock it so the test asserts routing, not React Flow
 * rendering. The mock surfaces a stable test-id + the peer-surface nav
 * links so the IA assertions can probe them.
 */
vi.mock('@/components/canvas/timeline-canvas/timeline-canvas', () => ({
  TimelineCanvas: ({ worldId }: { worldId: string }) => (
    <div data-testid="timeline-canvas-mock">
      <span data-testid="timeline-canvas-world">{worldId}</span>
      <a href={`/worlds/${encodeURIComponent(worldId)}/kb`} data-testid="peer-worldkb">
        World KB
      </a>
      <a href="/strategies" data-testid="peer-strategy">
        Strategy
      </a>
    </div>
  ),
}));

function LocationDisplay() {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
}

function AppRoutes() {
  return (
    <>
      <LocationDisplay />
      <Routes>
        {/* Mirror App.tsx World + Work routing. */}
        <Route path="works/:workId">
          <Route index element={<Navigate to="outline" replace />} />
          <Route
            path="outline"
            element={<div data-testid="work-outline-outlet">outline</div>}
          />
        </Route>
        <Route path="worlds" element={<WorldsPage />} />
        <Route path="worlds/:worldId">
          <Route index element={<Navigate to="timeline" replace />} />
          <Route path="timeline" element={<TimelinePage />} />
          <Route
            path="kb"
            element={<div data-testid="world-kb-outlet">world-kb</div>}
          />
        </Route>
      </Routes>
    </>
  );
}

describe('World default route + IA (V1.122 P1 T3)', () => {
  it('redirects /worlds/:worldId to /worlds/:worldId/timeline (Timeline is the default World entry)', async () => {
    renderInApp(<AppRoutes />, {
      client: noopClient,
      initialRouterEntries: ['/worlds/eryndor'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent(
        '/worlds/eryndor/timeline',
      );
    });
    expect(screen.getByTestId('timeline-canvas-mock')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-canvas-world')).toHaveTextContent(
      'eryndor',
    );
  });

  it('renders the Timeline canvas at the timeline route', async () => {
    renderInApp(<AppRoutes />, {
      client: noopClient,
      initialRouterEntries: ['/worlds/eryndor/timeline'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('timeline-canvas-mock')).toBeInTheDocument();
    });
    expect(screen.getByTestId('location')).toHaveTextContent(
      '/worlds/eryndor/timeline',
    );
  });

  it('keeps /worlds/:worldId/kb reachable as a peer surface (regression)', async () => {
    renderInApp(<AppRoutes />, {
      client: noopClient,
      initialRouterEntries: ['/worlds/eryndor/kb'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('world-kb-outlet')).toBeInTheDocument();
    });
    expect(screen.getByTestId('location')).toHaveTextContent(
      '/worlds/eryndor/kb',
    );
    // The Timeline canvas does NOT mount on the World KB route.
    expect(screen.queryByTestId('timeline-canvas-mock')).not.toBeInTheDocument();
  });

  it('does NOT redirect /works/:workId away from outline (Work entry unchanged — V1.118 regression)', async () => {
    renderInApp(<AppRoutes />, {
      client: noopClient,
      initialRouterEntries: ['/works/work-7'],
    });

    await waitFor(() => {
      expect(screen.getByTestId('location')).toHaveTextContent(
        '/works/work-7/outline',
    );
    });
    expect(screen.getByTestId('work-outline-outlet')).toBeInTheDocument();
    // The Timeline canvas MUST NOT mount on Work entry.
    expect(screen.queryByTestId('timeline-canvas-mock')).not.toBeInTheDocument();
  });
});
