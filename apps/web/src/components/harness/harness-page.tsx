/**
 * Harness rail app (plan v1220 T2) — replaces the legacy `/strategies` list
 * page inside the new shell.
 *
 * Reads only existing daemon endpoints (no new aggregation contract):
 * - catalog rows from `usePresets()` (user + embedded + system, `_system.*`
 *   internals hidden);
 * - strategy binding per row from the preset profile (`states.length > 0`);
 * - running-session counts from the bounded session walk, grouped by
 *   `preset_id`;
 * - the agent profile bar via the shared service-backed seam.
 *
 * The strategy canvas (`/strategies/:presetId`) and the profile drill-down
 * (`/strategies/:presetId/profile`) stay reachable from each row.
 */
import { useMemo, useState } from 'react';
import { useQueries } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Plus } from 'lucide-react';
import type { PresetSummary } from '@42ch/nexus-contracts';

import { presetProfileQueryOptions, usePresets } from '@/api/queries';
import { useNexusClient } from '@/lib/client-context';
import { countRunningSessions, useVisibleSessions } from '@/lib/developer/hub-summary';
import { errorMessage } from '@/lib/error-message';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/states';
import { ScaffoldPresetDialog } from '@/pages/dialogs/scaffold-preset-dialog';

import { PresetCard, type PresetRunningState } from './preset-card';
import { ProfileBar } from './profile-bar';

/** Hide internal `_system.` presets from authors (AD-P0-3). */
function isAuthorSystemPreset(preset: PresetSummary): boolean {
  return !preset.id.startsWith('_system.');
}

export function HarnessPage() {
  const { t } = useTranslation('strategies');
  const { t: tCommon } = useTranslation('common');
  const client = useNexusClient();
  const navigate = useNavigate();
  const presets = usePresets();
  const sessions = useVisibleSessions();
  const [scaffoldOpen, setScaffoldOpen] = useState(false);

  const catalogPresets = useMemo<PresetSummary[]>(
    () =>
      presets.data
        ? [
            ...presets.data.user,
            ...presets.data.embedded.filter(isAuthorSystemPreset),
            ...presets.data.system.filter(isAuthorSystemPreset),
          ]
        : [],
    [presets.data],
  );

  // One profile query per row sharing the exact `['presets','profile',<id>]`
  // cache keys of the canvas/profile pages, so both drill-downs are cache hits.
  const profiles = useQueries({
    queries: catalogPresets.map((preset) => presetProfileQueryOptions(preset.id, client)),
  });

  const runningByPreset = useMemo(() => {
    const byPreset = new Map<string, { status: string }[]>();
    for (const session of sessions.data?.sessions ?? []) {
      const bucket = byPreset.get(session.preset_id) ?? [];
      bucket.push(session);
      byPreset.set(session.preset_id, bucket);
    }
    const counts = new Map<string, number>();
    for (const [presetId, bucket] of byPreset) {
      counts.set(presetId, countRunningSessions(bucket));
    }
    return counts;
  }, [sessions.data]);

  // One row-level state: a failed session read is unknown (never `idle`), and
  // a truncated walk marks the count as a lower bound.
  const runningStateFor = (presetId: string): PresetRunningState => {
    if (sessions.isError) return { status: 'error' };
    if (!sessions.data) return { status: 'loading' };
    return {
      status: 'ready',
      count: runningByPreset.get(presetId) ?? 0,
      truncated: sessions.data.truncated,
    };
  };

  return (
    <div className="flex flex-col gap-4" data-testid="harness-page">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-heading-24 font-heading text-gray-1000">{t('title')}</h1>
          <p className="text-copy-14 text-gray-900">{t('description')}</p>
        </div>
        <Button
          type="button"
          variant="primary"
          size="small"
          onClick={() => setScaffoldOpen(true)}
        >
          <Plus className="h-4 w-4" aria-hidden />
          {t('harness.newPreset')}
        </Button>
      </div>

      <ProfileBar />

      <section className="flex flex-col gap-2" data-testid="harness-catalog">
        {presets.isLoading ? (
          <p className="text-copy-13 text-gray-700" data-testid="harness-catalog-loading">
            {t('harness.loading')}
          </p>
        ) : presets.isError ? (
          <div className="flex flex-col items-start gap-2" role="alert" data-testid="harness-catalog-error">
            <p className="text-copy-13 text-red-900">
              {t('harness.error', {
                reason: errorMessage(presets.error) || tCommon('error.requestFailed'),
              })}
            </p>
            <Button type="button" variant="tertiary" size="small" onClick={() => void presets.refetch()}>
              {t('harness.retry')}
            </Button>
          </div>
        ) : catalogPresets.length === 0 ? (
          <EmptyState title={t('harness.empty')} />
        ) : (
          <ul className="flex flex-col gap-2">
            {catalogPresets.map((preset, index) => (
              <PresetCard
                key={preset.id}
                preset={preset}
                profile={profiles[index]!}
                running={runningStateFor(preset.id)}
                onOpenCanvas={() => navigate(`/strategies/${encodeURIComponent(preset.id)}`)}
                onOpenProfile={() =>
                  navigate(`/strategies/${encodeURIComponent(preset.id)}/profile`)
                }
              />
            ))}
          </ul>
        )}
      </section>

      <ScaffoldPresetDialog
        open={scaffoldOpen}
        onOpenChange={setScaffoldOpen}
        onCreated={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
      />
    </div>
  );
}
