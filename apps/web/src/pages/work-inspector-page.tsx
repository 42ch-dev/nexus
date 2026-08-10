/**
 * Assembly Inspector page — P1 T3 (DF-76). Read-only debug surface.
 *
 * IA placement (plan Q1): a moment/assembly **debug surface in the creator
 * area** — a Control-Room-style sibling of `outline` / `timeline` / `chapters`
 * under the Work canvas shell at `/works/:workId/inspector`. It is **not** a
 * canvas node-inspector (assembly is moment-level) and adds **no new
 * `CanvasSurfaceKind`**.
 *
 * Inputs: `work_id` comes from the URL; `world_id` from the Work's bound world
 * (`WorkDetailResponse.world_id`); `generation_stage` is an optional debug
 * selector (defaults to the unspecified stage, omitted from the request). The
 * panel assembles via `useInspectMoment` — observation only, never writes
 * (AC-I4/I6).
 *
 * Batch B (T4): the Moment Directive set/clear form mounts beside the
 * directive-status section via `AssemblyInspectorPanel#directiveActions` —
 * the panel itself stays read-only (AC-I4); the form is the author write
 * surface (AC-I5), invalidating `useInspectMoment` on set/clear.
 */
import { useState } from 'react';
import { useParams } from 'react-router';
import { useTranslation } from 'react-i18next';

import { useInspectMoment, useWork } from '@/api/queries';
import { AssemblyInspectorPanel } from '@/components/inspector/assembly-inspector-panel';
import { MomentDirectiveForm } from '@/components/inspector/moment-directive-form';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import type { MomentInspectRequest } from '@42ch/nexus-contracts';

// NOTE: `unspecified` is intentionally NOT listed here — the empty-value
// default option (value="") below already expresses it and is omitted from
// the wire request so the daemon applies its own default (V1.155 P2 T3,
// R-V1151P2-003: listing it again rendered the stage selector with two
// "unspecified" entries).
const GENERATION_STAGES: MomentInspectRequest['generation_stage'][] = [
  'intake',
  'research',
  'produce',
  'review',
  'persist',
  'work_maintenance',
  'system_maintenance',
];

export function WorkInspectorPage() {
  const { t } = useTranslation('inspector');
  const { workId = '' } = useParams<{ workId?: string }>();
  const workQuery = useWork(workId);
  const [generationStage, setGenerationStage] = useState<
    MomentInspectRequest['generation_stage'] | undefined
  >(undefined);

  const worldId = workQuery.data?.world_id?.trim() ?? undefined;
  // `generation_stage` is optional on the wire; the default (unspecified)
  // stage is omitted so the daemon applies its own default.
  const request: MomentInspectRequest | undefined = worldId
    ? { world_id: worldId, work_id: workId, ...(generationStage ? { generation_stage: generationStage } : {}) }
    : undefined;
  const inspect = useInspectMoment(request);

  if (workQuery.isLoading) {
    return <LoadingState label={t('page.loading')} />;
  }
  if (workQuery.isError) {
    return <ErrorState description={t('page.noWorkDescription')} onRetry={() => workQuery.refetch()} />;
  }
  if (!worldId) {
    return <EmptyState title={t('page.noWorldTitle')} description={t('page.noWorldDescription')} />;
  }
  if (inspect.isLoading) {
    return <LoadingState label={t('page.loading')} />;
  }
  if (inspect.isError || !inspect.data) {
    return <ErrorState description={t('page.loadError')} onRetry={() => inspect.refetch()} />;
  }

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 p-6" data-testid="work-inspector-page">
      <header className="flex flex-col gap-1">
        <h1 className="text-heading-24 font-heading text-gray-1000">{t('page.title')}</h1>
        <p className="text-copy-14 text-gray-900">{t('page.description')}</p>
      </header>

      <div className="flex flex-wrap items-end gap-4">
        <div className="flex flex-col gap-1">
          <Label htmlFor="inspector-generation-stage">{t('page.generationStageLabel')}</Label>
          <Select
            id="inspector-generation-stage"
            value={generationStage ?? ''}
            onChange={(e) => setGenerationStage(e.target.value === '' ? undefined : (e.target.value as MomentInspectRequest['generation_stage']))}
            className="w-48"
          >
            <option value="">{t('page.generationStageDefault')}</option>
            {GENERATION_STAGES.map((stage) => (
              <option key={stage} value={stage}>
                {t(`page.generationStage.${stage}` as const)}
              </option>
            ))}
          </Select>
        </div>
        <Button
          type="button"
          variant="secondary"
          size="small"
          onClick={() => inspect.refetch()}
          disabled={inspect.isFetching}
          aria-label={t('page.refreshAria')}
        >
          {t('page.refresh')}
        </Button>
      </div>

      <AssemblyInspectorPanel
        packet={inspect.data}
        directiveActions={
          <MomentDirectiveForm
            workId={workId}
            worldId={worldId}
            momentDirective={inspect.data.moment_directive}
          />
        }
      />
    </div>
  );
}
