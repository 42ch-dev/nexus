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

/**
 * App routes — Control Room + Setup shell.
 *
 * Seven screen groups (web-ui.md §6): Works dashboard + detail, Sessions,
 * Schedule, Capabilities, Findings (Control Room — READ), Work CRUD + Preset
 * management (Setup — writes). All screens consume the hardened Local API via
 * the NexusClient interface (transport-agnostic, Tauri-ready).
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
        <Route path="sessions" element={<SessionsPage />} />
        <Route path="schedule" element={<SchedulePage />} />
        <Route path="capabilities" element={<CapabilitiesPage />} />
        <Route path="findings" element={<FindingsPage />} />
        <Route path="presets" element={<PresetsPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
