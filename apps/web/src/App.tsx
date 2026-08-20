import { lazy, Suspense } from 'react';
import { useTranslation } from 'react-i18next';
import { Navigate, Route, Routes, useLocation } from 'react-router';

import {
  ActiveCreatorProvider,
  DefaultProfileCoordinator,
} from '@/lib/active-creator-context';
import { SetupCompletedProvider } from '@/lib/setup-completed-context';
import { EntranceProvider } from '@/lib/entrance-context';
import {
  EntranceGuard,
  EntranceIndexRedirect,
} from '@/components/layout/entrance-guard';
import { RootLayout } from '@/components/layout/root-layout';
import { SettingsModalHost } from '@/components/layout/settings-modal-host';
import {
  SettingsModalProvider,
  useSettingsModal,
} from '@/components/layout/settings-modal-context';
import { DaemonLaunchGate } from '@/components/setup/daemon-launch-gate';
import { SetupGate } from '@/components/setup/setup-gate';
import { ChapterPage } from '@/pages/chapter-page';
import { ChaptersPage } from '@/pages/chapters-page';
import { CapabilitiesPage } from '@/pages/capabilities-page';
import { FindingsPage } from '@/pages/findings-page';
import { GlobalTimelinePage } from '@/pages/global-timeline-page';
import { MemoryPage } from '@/pages/memory-page';
import { ModulesPage } from '@/pages/modules-page';
import { NotFoundPage } from '@/pages/not-found-page';
import { SchedulePage } from '@/pages/schedule-page';
import { SessionsPage } from '@/pages/sessions-page';
import { WorkShellLayout } from '@/components/layout/work-shell-layout';
import { CreatorHubPage } from '@/pages/creator-hub-page';
import { SetupWizardPage } from '@/pages/setup-wizard-page';
import { EntrancePage } from '@/pages/entrance-page';
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

// Route-split: the Work Timeline canvas (V1.123 P2 T5) is a peer of Outline
// from the Work Canvas shell. Lazy-loaded alongside the other canvas routes so
// React Flow stays out of the Control Room bootstrap chunk
// (canvas-strategy-surface.md §3.1). Work entry stays Outline (V1.118
// regression) — this route is a SIBLING at `/works/:workId/timeline`, NOT the
// index.
const WorkTimelinePage = lazy(() =>
  import('@/pages/work-timeline-page').then((m) => ({ default: m.WorkTimelinePage })),
);

// Route-split: the World KB canvas pulls in `@xyflow/react` and is lazy-loaded
// alongside the other canvas routes (canvas-strategy-surface.md §3.1).
const WorldKbPage = lazy(() =>
  import('@/pages/world-kb-page').then((m) => ({ default: m.WorldKbPage })),
);

// V1.151 P1 (DF-76) — Assembly Inspector debug surface. A Control-Room-style
// read-only page (no `@xyflow/react`), sibling of outline/timeline/chapters
// under the Work canvas shell.
const WorkInspectorPage = lazy(() =>
  import('@/pages/work-inspector-page').then((m) => ({ default: m.WorkInspectorPage })),
);

// Route-split: the Timeline canvas (V1.122 P1 T3) is the default World entry.
// Lazy-loaded alongside the other canvas routes so React Flow stays out of the
// Control Room bootstrap chunk (canvas-strategy-surface.md §3.1).
const TimelinePage = lazy(() =>
  import('@/pages/timeline-page').then((m) => ({ default: m.TimelinePage })),
);

// V1.166 P2 (DR-64 surfacing half) — World Findings is a light Control-Room
// list page (no `@xyflow/react`), sibling of the world `timeline` / `kb`
// routes (PD-2 locked placement). Lazy-loaded to keep the route-split
// convention uniform.
const WorldFindingsPage = lazy(() =>
  import('@/pages/world-findings-page').then((m) => ({ default: m.WorldFindingsPage })),
);

// Route-split: the Develop hub (V1.170 P1 — AR-18) is the developer-entrance
// land route. Lazy-loaded like the canvas routes so the hub chunk stays out
// of the bootstrap path (canvases keep React Flow out; the hub is static cards).
const DeveloperHubPage = lazy(() =>
  import('@/pages/developer-hub-page').then((m) => ({ default: m.DeveloperHubPage })),
);

/**
 * App routes — Control Room + Setup shell.
 *
 * V1.105: outer {@link DaemonLaunchGate} waits for daemon Ready on every
 * desktop launch; inner {@link SetupGate} routes by `setup_completed` only.
 * `/setup` and the main shell are siblings under the outer gate. `/strategy`
 * redirects to `/strategies` list + `/strategies/:presetId` detail.
 */
/**
 * `/strategy` legacy redirect preserves query state (e.g. preset ID) rather
 * than dropping the search string on the floor.
 */
function StrategyRedirect() {
  const { search } = useLocation();
  return <Navigate to={{ pathname: '/strategies', search }} replace />;
}

/**
 * Product routes render against the last safe non-settings location while the
 * browser URL is `/settings/*` or `/modules` (modal-primary, V1.131 P2).
 */
function AppRoutes() {
  const location = useLocation();
  const { open, backgroundLocation } = useSettingsModal();
  const { t } = useTranslation('strategies');
  const routesLocation = open ? backgroundLocation : location;

  return (
    <Routes location={routesLocation}>
      <Route path="setup" element={<SetupWizardPage />} />
      {/* V1.170 P1 (AR-16/AR-20) — entrance identity page. Standalone (sibling
          of /setup, outside the gated tree): first-run land for unset browser
          installs + the footer switch control's destination. Only Continue
          persists. */}
      <Route path="entrance" element={<EntrancePage />} />
      <Route
        element={
          <SetupGate>
            <EntranceGuard>
              <RootLayout />
            </EntranceGuard>
          </SetupGate>
        }
      >
        {/* V1.170 P1 (AR-18) — index redirect is entrance-aware: Create → /works,
            Develop → /developer. `landRoute` is the single source. */}
        <Route index element={<EntranceIndexRedirect />} />
        {/* V1.123 P3 Task 1 — global Timeline entry in primary nav.
            Cross-World overview composed client-side; sibling of `/works`
            and `/worlds`. Per-World Timeline stays at
            `/worlds/:worldId/timeline` (V1.122 P1 T3 hero surface); Work
            Timeline stays at `/works/:workId/timeline` (V1.123 P2 T5). */}
        <Route path="timeline" element={<GlobalTimelinePage />} />
        <Route path="works" element={<CreatorHubPage />} />
        <Route path="works/chapters" element={<ChaptersPage />} />
        <Route path="works/:workId" element={<WorkShellLayout />}>
          <Route index element={<Navigate to="outline" replace />} />
          <Route
            path="outline"
            element={
              <Suspense fallback={<LoadingState label="Loading Outline…" />}>
                <OutlinePage />
              </Suspense>
            }
          />
          {/* V1.123 P2 T5 — Work Timeline peer surface. Sibling of `outline`;
              index redirect above still points to `outline` so the Work entry
              default stays Outline (V1.118 regression preserved). */}
          <Route
            path="timeline"
            element={
              <Suspense fallback={<LoadingState label="Loading Work Timeline…" />}>
                <WorkTimelinePage />
              </Suspense>
            }
          />
          <Route path="chapters" element={<ChaptersPage />} />
          <Route path="chapters/:chapter" element={<ChapterPage />} />
          {/* V1.151 P1 (DF-76) — Assembly Inspector debug surface (read-only,
              moment-level; sibling of outline/timeline/chapters). */}
          <Route
            path="inspector"
            element={
              <Suspense fallback={<LoadingState label="Loading Assembly Inspector…" />}>
                <WorkInspectorPage />
              </Suspense>
            }
          />
        </Route>
        <Route
          path="worlds"
          element={<CreatorHubPage />}
        />
        {/* V1.122 P1 T3 — Timeline is the default World entry. The index
            redirect sends `/worlds/:worldId` to `/worlds/:worldId/timeline`
            (the World-building hero surface). Peer surfaces (World KB at
            `/kb`, Strategy via `/strategies`) stay reachable as siblings.
            Work entry is unchanged — `/works/:workId` still redirects to
            `outline` (V1.118 regression gate). */}
        <Route path="worlds/:worldId">
          <Route
            index
            element={<Navigate to="timeline" replace />}
          />
          <Route
            path="timeline"
            element={
              <Suspense fallback={<LoadingState label="Loading Timeline…" />}>
                <TimelinePage />
              </Suspense>
            }
          />
          <Route
            path="kb"
            element={
              <Suspense fallback={<LoadingState label="Loading World KB…" />}>
                <WorldKbPage />
              </Suspense>
            }
          />
          {/* V1.166 P2 (DR-64 surfacing half) — World Findings peer surface.
              Sibling of `timeline` + `kb` at `/worlds/:worldId/findings`
              (PD-2 locked placement: Control Room list/sections, read-only).
              The index redirect stays Timeline — this is a peer, not the
              World entry. */}
          <Route
            path="findings"
            element={
              <Suspense fallback={<LoadingState label="Loading World Findings…" />}>
                <WorldFindingsPage />
              </Suspense>
            }
          />
        </Route>
        <Route path="sessions" element={<SessionsPage />} />
        <Route path="schedule" element={<SchedulePage />} />
        {/* V1.170 P1 (EL-6) — capability browser restored as a live Develop-tree
            surface (read-only builtin capability schemas from
            GET /v1/daemon/orchestration/capabilities). The V1.120 P2 soft-remove
            redirect is replaced by the entrance guard: Create bounces to
            `/works`, Develop renders the browser (AR-15/AR-19). */}
        <Route path="capabilities" element={<CapabilitiesPage />} />
        {/* Compatibility only — Settings modal owns Modules (V1.131 P2). */}
        <Route path="modules" element={<ModulesPage />} />
        <Route path="findings" element={<FindingsPage />} />
        <Route path="memory" element={<MemoryPage />} />
        <Route path="strategies" element={<StrategiesPage />} />
        <Route
          path="strategies/:presetId"
          element={
            <Suspense fallback={<LoadingState label={t('strategyDetail.loading')} />}>
              <StrategyDetailPage />
            </Suspense>
          }
        />
        <Route path="strategy" element={<StrategyRedirect />} />
        <Route
          path="connect"
          element={<Navigate to="/settings/advanced#connection" replace />}
        />
        {/* V1.170 P1 (AR-18) — Develop hub v1: developer-entrance land route.
            Develop-only (guard-bounced on Create); lazy-split with the other
            route-split pages. */}
        <Route
          path="developer"
          element={
            <Suspense fallback={<LoadingState label="Loading Develop hub…" />}>
              <DeveloperHubPage />
            </Suspense>
          }
        />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}

export function App() {
  return (
    <ActiveCreatorProvider>
      <SetupCompletedProvider>
        <DaemonLaunchGate>
          {/* V1.134 P0: gate creator probes until daemon ready — avoids
              pre-boot `/v1/daemon/creators*` traffic during dist-load startup. */}
          <DefaultProfileCoordinator />
          <SettingsModalProvider>
            <EntranceProvider>
              <AppRoutes />
            </EntranceProvider>
            <SettingsModalHost />
          </SettingsModalProvider>
        </DaemonLaunchGate>
      </SetupCompletedProvider>
    </ActiveCreatorProvider>
  );
}
