/**
 * World Findings page — route entry for the world-scoped check findings
 * surface (V1.166 P2 / DR-64 surfacing half, Task 1).
 *
 * Reads the world id from the URL and renders {@link WorldFindingsPanel}.
 * The page is thin so the panel can be tested in isolation. IA (PD-2,
 * locked): `/worlds/:worldId/findings` is a **peer** of `/timeline` and
 * `/kb` — a Control Room list/sections surface, not a canvas mount and not
 * the Work `/findings` page (work-scoped remediation vocabulary).
 *
 * Read-only: both sections have zero write controls (PD-2). Composition:
 * the Rules section (Task 2) mounts below the findings panel on this same
 * page.
 */
import { useParams } from 'react-router';
import { useTranslation } from 'react-i18next';

import { WorldFindingsPanel } from '@/components/worlds/world-findings/world-findings-panel';
import { WorldRulesSection } from '@/components/worlds/world-rules/world-rules-section';
import { NotFoundPage } from '@/pages/not-found-page';

export function WorldFindingsPage() {
  const { t } = useTranslation('worldFindings');
  const { worldId } = useParams<{ worldId: string }>();
  if (!worldId) return <NotFoundPage />;
  return (
    <div className="flex flex-col gap-4" data-testid="world-findings-page">
      <div>
        <h1 className="font-display text-display-24 text-gray-1000">{t('page.title')}</h1>
        <p className="text-copy-14 text-gray-900">{t('page.description')}</p>
      </div>
      <WorldFindingsPanel worldId={worldId} />
      <WorldRulesSection worldId={worldId} />
    </div>
  );
}
