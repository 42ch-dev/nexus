import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router-dom';
import { Plus, RefreshCw, ShieldCheck, Sparkles } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { EmptyState, ErrorState, LoadingState } from '@/components/ui/states';
import { usePresets, useReloadPreset } from '@/api/queries';
import type { PresetSummary } from '@42ch/nexus-contracts';

import { ScaffoldPresetDialog } from './dialogs/scaffold-preset-dialog';
import { ValidatePresetDialog } from './dialogs/validate-preset-dialog';

/**
 * Strategies list — unified entry point for Presets + Strategy canvas.
 *
 * Lists presets grouped by source. Selecting a row navigates to the canvas
 * detail at `/strategies/:presetId`. The canvas surface itself is preserved
 * verbatim from the previous `/strategy` route.
 */
export function StrategiesPage() {
  const { t } = useTranslation('strategies');
  const presets = usePresets();
  const reload = useReloadPreset();
  const navigate = useNavigate();
  const [scaffoldOpen, setScaffoldOpen] = useState(false);
  const [validateOpen, setValidateOpen] = useState(false);

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-heading-24 font-heading text-gray-1000">{t('title')}</h1>
          <p className="text-copy-14 text-gray-900">{t('description')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" variant="secondary" size="small" onClick={() => setValidateOpen(true)}>
            <ShieldCheck className="h-4 w-4" aria-hidden />
            {t('validatePreset')}
          </Button>
          <Button type="button" variant="primary" size="small" onClick={() => setScaffoldOpen(true)}>
            <Plus className="h-4 w-4" aria-hidden />
            {t('scaffoldPreset')}
          </Button>
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={() => presets.refetch()}
            disabled={presets.isFetching}
            aria-label={t('refreshAria')}
          >
            <RefreshCw className={`h-4 w-4 ${presets.isFetching ? 'animate-spin' : ''}`} aria-hidden />
          </Button>
        </div>
      </div>

      {presets.isError ? (
        <ErrorState description={t('errorDescription')} onRetry={() => presets.refetch()} />
      ) : presets.isLoading ? (
        <Card className="shadow-card">
          <CardContent>
            <LoadingState label={t('loading')} />
          </CardContent>
        </Card>
      ) : !presets.data ? null : (
        <div className="flex flex-col gap-4">
          <PresetGroup
            title={t('userPresets.title')}
            description={t('userPresets.description')}
            presets={presets.data.user}
            onReload={(id) => reload.mutate(id)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('userPresets.empty')}
          />
          <PresetGroup
            title={t('systemPresets.title')}
            description={t('systemPresets.description')}
            presets={presets.data.system}
            onReload={(id) => reload.mutate(id)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('systemPresets.empty')}
          />
          <PresetGroup
            title={t('embeddedPresets.title')}
            description={t('embeddedPresets.description')}
            presets={presets.data.embedded}
            onReload={(id) => reload.mutate(id)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('embeddedPresets.empty')}
          />
        </div>
      )}

      <ScaffoldPresetDialog open={scaffoldOpen} onOpenChange={setScaffoldOpen} />
      <ValidatePresetDialog open={validateOpen} onOpenChange={setValidateOpen} />
    </div>
  );
}

function PresetGroup({
  title,
  description,
  presets,
  onReload,
  onSelect,
  reloadingId,
  empty,
}: {
  title: string;
  description: string;
  presets: PresetSummary[];
  onReload: (id: string) => void;
  onSelect: (id: string) => void;
  reloadingId: string | undefined;
  empty: string;
}) {
  const { t } = useTranslation('strategies');
  return (
    <Card className="shadow-card">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent>
        {presets.length === 0 ? (
          <EmptyState title={empty} />
        ) : (
          <ul className="flex flex-col gap-2">
            {presets.map((p) => (
              <li
                key={p.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-gray-alpha-400 p-3"
              >
                <button
                  type="button"
                  onClick={() => onSelect(p.id)}
                  className="flex items-center gap-2 text-left"
                >
                  <Sparkles className="h-4 w-4 text-purple-700" aria-hidden />
                  <span className="text-copy-13-mono text-gray-1000">{p.id}</span>
                  {p.run_intents && p.run_intents.length > 0 && (
                    <div className="flex flex-wrap gap-1">
                      {p.run_intents.map((intent) => (
                        <Badge key={intent} variant="preset">
                          {intent}
                        </Badge>
                      ))}
                    </div>
                  )}
                </button>
                <Button
                  type="button"
                  variant="tertiary"
                  size="small"
                  onClick={() => onReload(p.id)}
                  disabled={reloadingId === p.id}
                >
                  {reloadingId === p.id ? t('reloading') : t('reload')}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
