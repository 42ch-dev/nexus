/**
 * Strategy detail — route entry for the Canvas Strategy Surface (α).
 *
 * Renders the selected preset as a state-machine graph via {@link StrategyCanvas}.
 * UI label is "Strategy"; persisted identifiers remain "preset" (Draft §4.2).
 *
 * Route-split: this page (and therefore `@xyflow/react`) is lazy-loaded by
 * `App.tsx` so React Flow never enters the Control Room bootstrap chunk
 * (Draft §3.1 bundle/performance).
 *
 * V1.120 P0 T2: a Back control to `/strategies` is always reachable. It lives
 * on this page (not inside the canvas) so it stays visible both in the
 * not-found empty state and in the detail header — the header renders above
 * the canvas load-error shell, so the author is never trapped in a dead-end
 * canvas `ErrorState` (AD-P0-1d, AC-P0-2).
 */
import { ArrowLeft } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { useNavigate, useParams } from 'react-router-dom';

import { StrategyCanvas } from '@/components/canvas/strategy-canvas';
import { Button } from '@/components/ui/button';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { usePresets } from '@/api/queries';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';

export function StrategyPage() {
  const { t } = useTranslation('strategies');
  const { presetId } = useParams<{ presetId: string }>();
  const navigate = useNavigate();
  const presets = usePresets();

  function handleBack() {
    navigate('/strategies');
  }

  if (presets.isLoading) {
    return <LoadingState label={t('strategyDetail.loading')} />;
  }

  if (presets.isError) {
    const retry = () => void presets.refetch();
    const backButton = (
      <Button type="button" variant="secondary" size="small" onClick={handleBack}>
        <ArrowLeft className="h-4 w-4" aria-hidden />
        {t('strategyDetail.back')}
      </Button>
    );

    if (isOrchestrationEngineUnavailable(presets.error)) {
      return (
        <UnavailableState
          title={t('engineUnavailableTitle')}
          description={t('engineUnavailableDescription')}
          onRetry={retry}
          action={backButton}
        />
      );
    }

    return (
      <div className="flex flex-col gap-4">
        <Button type="button" variant="tertiary" size="small" onClick={handleBack} className="self-start">
          <ArrowLeft className="h-4 w-4" aria-hidden />
          {t('strategyDetail.back')}
        </Button>
        <ErrorState
          title={t('errorTitle')}
          description={t('errorDescription')}
          onRetry={retry}
        />
      </div>
    );
  }

  const all = presets.data
    ? [...presets.data.user, ...presets.data.system, ...presets.data.embedded]
    : [];
  const activePreset = all.find((p) => p.id === presetId);

  if (!activePreset) {
    return (
      <EmptyState
        title={t('strategyDetail.notFoundTitle')}
        description={t('strategyDetail.notFoundDescription')}
        action={
          <Button type="button" variant="secondary" size="small" onClick={handleBack}>
            <ArrowLeft className="h-4 w-4" aria-hidden />
            {t('strategyDetail.back')}
          </Button>
        }
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <Button type="button" variant="tertiary" size="small" onClick={handleBack} className="mb-2">
          <ArrowLeft className="h-4 w-4" aria-hidden />
          {t('strategyDetail.back')}
        </Button>
        <h1 className="text-heading-24 font-heading text-gray-1000">{t('strategyDetail.title')}</h1>
        <p className="text-copy-14 text-gray-900">
          {t('strategyDetail.description')}
        </p>
      </div>
      <StrategyCanvas presetId={activePreset.id} />
    </div>
  );
}
