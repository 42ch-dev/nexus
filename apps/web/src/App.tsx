import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { RootLayout } from '@/components/layout/root-layout';
import { CapabilitiesPage } from '@/pages/capabilities-page';
import { ChapterPage } from '@/pages/chapter-page';
import { ChaptersPage } from '@/pages/chapters-page';
import { FindingsPage } from '@/pages/findings-page';
import { NotFoundPage } from '@/pages/not-found-page';
import { PresetsPage } from '@/pages/presets-page';
import { SchedulePage } from '@/pages/schedule-page';
import { SessionsPage } from '@/pages/sessions-page';
import { WorkDetailPage } from '@/pages/work-detail-page';
import { WorksPage } from '@/pages/works-page';
import { LoadingState } from '@/components/ui/states';

// Route-split: the Strategy canvas pulls in `@xyflow/react`, which is a
// significant interactive dependency. Lazy-loading keeps it out of the Control
// Room bootstrap chunk (canvas-strategy-surface.md Draft §3.1 bundle/perf).
const StrategyPage = lazy(() =>
  import('@/pages/strategy-page').then((m) => ({ default: m.StrategyPage })),
);

// Route-split: the Outline canvas contains the outline/timeline interactive
// surface and is not part of the Control Room bootstrap path.
const OutlinePage = lazy(() =>
  import('@/pages/outline-page').then((m) => ({ default: m.OutlinePage })),
);

// Route-split: the World KB canvas pulls in `@xyflow/react` and is lazy-loaded
// alongside the other canvas routes (canvas-strategy-surface.md §3.1).
const WorldKbPage = lazy(() =>
  import('@/pages/world-kb-page').then((m) => ({ default: m.WorldKbPage })),
);

/**
 * App routes — Control Room + Setup shell.
 *
 * Seven screen groups (web-ui.md §6): Works dashboard + detail, Sessions,
 * Schedule, Capabilities, Findings (Control Room — READ), Work CRUD + Preset
 * management (Setup — writes). All screens consume the hardened Local API via
 * the NexusClient interface (transport-agnostic, Tauri-ready). V1.70 adds the
 * Strategy canvas route (lazy-split — React Flow stays out of the bootstrap).
 */
export function App() {
  return (
    <Routes>
      <Route element={<RootLayout />}>
        <Route index element={<Navigate to="/works" replace />} />
        <Route path="works" element={<WorksPage />} />
        <Route path="works/chapters" element={<ChaptersPage />} />
        <Route path="works/:workId" element={<WorkDetailPage />} />
        <Route path="works/:workId/chapters" element={<ChaptersPage />} />
        <Route path="works/:workId/chapters/:chapter" element={<ChapterPage />} />
        <Route
          path="works/:workId/outline"
          element={
            <Suspense fallback={<LoadingState label="Loading Outline…" />}>
              <OutlinePage />
            </Suspense>
          }
        />
        <Route
          path="worlds/:worldId/kb"
          element={
            <Suspense fallback={<LoadingState label="Loading World KB…" />}>
              <WorldKbPage />
            </Suspense>
          }
        />
        <Route path="sessions" element={<SessionsPage />} />
        <Route path="schedule" element={<SchedulePage />} />
        <Route path="capabilities" element={<CapabilitiesPage />} />
        <Route path="findings" element={<FindingsPage />} />
        <Route path="presets" element={<PresetsPage />} />
        <Route
          path="strategy"
          element={
            <Suspense fallback={<LoadingState label="Loading Strategy…" />}>
              <StrategyPage />
            </Suspense>
          }
        />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
