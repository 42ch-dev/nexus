import { lazy, Suspense } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { ActiveCreatorProvider } from '@/lib/active-creator-context';
import { SetupCompletedProvider } from '@/lib/setup-completed-context';
import { RootLayout } from '@/components/layout/root-layout';
import { SetupGate } from '@/components/setup/setup-gate';
import { CapabilitiesPage } from '@/pages/capabilities-page';
import { ChapterPage } from '@/pages/chapter-page';
import { ChaptersPage } from '@/pages/chapters-page';
import { FindingsPage } from '@/pages/findings-page';
import { MemoryPage } from '@/pages/memory-page';
import { NotFoundPage } from '@/pages/not-found-page';
import { SchedulePage } from '@/pages/schedule-page';
import { SessionsPage } from '@/pages/sessions-page';
import { WorkDetailPage } from '@/pages/work-detail-page';
import { WorksPage } from '@/pages/works-page';
import { ConnectDaemonPage } from '@/pages/connect-daemon-page';
import { SetupWizardPage } from '@/pages/setup-wizard-page';
import { StrategiesPage } from '@/pages/strategies-page';
import { LoadingState } from '@/components/ui/states';

// Route-split: the Strategy canvas pulls in `@xyflow/react`, which is a
// significant interactive dependency. Lazy-loading keeps it out of the Control
// Room bootstrap chunk (canvas-strategy-surface.md Draft §3.1 bundle/perf).
const StrategyDetailPage = lazy(() =>
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
 * V1.94 adds the setup wizard (`/setup`) and per-launch daemon-ready gate. The
 * wizard is rendered outside the main shell; the main shell is shown only after
 * setup is complete and the daemon is reachable. `/presets` and `/strategy`
 * redirect to the unified `/strategies` list + `/strategies/:presetId` detail.
 */
function AppRoutes() {
  return (
    <Routes>
      <Route path="setup" element={<SetupWizardPage />} />
      <Route element={<SetupGate><RootLayout /></SetupGate>}>
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
        <Route path="memory" element={<MemoryPage />} />
        <Route path="strategies" element={<StrategiesPage />} />
        <Route
          path="strategies/:presetId"
          element={
            <Suspense fallback={<LoadingState label="Loading Strategy…" />}>
              <StrategyDetailPage />
            </Suspense>
          }
        />
        <Route path="presets" element={<Navigate to="/strategies" replace />} />
        <Route path="strategy" element={<Navigate to="/strategies" replace />} />
        <Route path="connect" element={<ConnectDaemonPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}

export function App() {
  return (
    <ActiveCreatorProvider>
      <SetupCompletedProvider>
        <AppRoutes />
      </SetupCompletedProvider>
    </ActiveCreatorProvider>
  );
}
