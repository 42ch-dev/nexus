import { Navigate, Route, Routes } from 'react-router-dom';

import { RootLayout } from '@/components/layout/root-layout';
import {
  CapabilitiesPage,
  FindingsPage,
  NotFoundPage,
  PresetsPage,
  SchedulePage,
  SessionsPage,
  WorkDetailPage,
  WorksPage,
} from '@/pages/screens';

/**
 * App routes — Control Room + Setup shell (plan P1). All screens are
 * placeholders; P2 fills them against the hardened Local API (web-ui.md §6).
 */
export function App() {
  return (
    <Routes>
      <Route element={<RootLayout />}>
        <Route index element={<Navigate to="/works" replace />} />
        <Route path="works" element={<WorksPage />} />
        <Route path="works/:workId" element={<WorkDetailPage />} />
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
