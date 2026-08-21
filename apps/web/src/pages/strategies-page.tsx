import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useNavigate } from 'react-router';
import { Plus, RefreshCw, Sparkles, Trash2 } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { EmptyState, ErrorState, LoadingState, UnavailableState } from '@/components/ui/states';
import { useDeletePreset, usePresetProfile, usePresets, useReloadPreset, type PresetGroups } from '@/api/queries';
import { isOrchestrationEngineUnavailable } from '@/lib/nexus/errors';
import type { PresetSummary } from '@42ch/nexus-contracts';

import { ScaffoldPresetDialog } from './dialogs/scaffold-preset-dialog';
import { ValidatePresetDialog } from './dialogs/validate-preset-dialog';

/**
 * Strategies list — unified entry point for Presets + Strategy canvas.
 *
 * Lists presets grouped by source. Selecting a row navigates to the canvas
 * detail at `/strategies/:presetId`. The canvas surface itself is preserved
 * verbatim from the previous `/strategy` route.
 *
 * V1.120 strategies-repair: the list is an author-facing preset manager —
 * `_system.*` internals are hidden from the System section, Validate is a
 * per-row action (no global header button), and user presets can be deleted
 * from a confirm dialog.
 */
export function StrategiesPage() {
  const { t } = useTranslation('strategies');
  const presets = usePresets();
  const reload = useReloadPreset();
  const removePreset = useDeletePreset();
  const navigate = useNavigate();
  const [scaffoldOpen, setScaffoldOpen] = useState(false);
  // Per-row Validate reuses the existing path-based ValidatePresetDialog — no
  // new validate flow (strategies-repair AD-P0-4). A single page-level dialog
  // instance is opened by any row's Validate action.
  const [validateOpen, setValidateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);

  // Filter `_system.*` from the System presets section only (AD-P0-3). User and
  // embedded groups are passed through untouched.
  const systemPresets = presets.data?.system.filter(isAuthorSystemPreset) ?? [];

  function confirmDelete() {
    if (deleteTarget === null) return;
    const id = deleteTarget;
    removePreset.mutate(id, {
      onSettled: () => setDeleteTarget(null),
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-heading-24 font-heading text-gray-1000">{t('title')}</h1>
          <p className="text-copy-14 text-gray-900">{t('description')}</p>
        </div>
        <div className="flex items-center gap-2">
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
        isOrchestrationEngineUnavailable(presets.error) ? (
          <UnavailableState
            title={t('engineUnavailableTitle')}
            description={t('engineUnavailableDescription')}
            onRetry={() => presets.refetch()}
          />
        ) : (
          <ErrorState
            title={t('errorTitle')}
            description={t('errorDescription')}
            onRetry={() => presets.refetch()}
          />
        )
      ) : presets.isLoading ? (
        <Card className="shadow-card">
          <CardContent>
            <LoadingState label={t('loading')} />
          </CardContent>
        </Card>
      ) : !presets.data ? null : (
        <div className="flex flex-col gap-4">
          <StrategyCatalog
            presets={presets.data}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
          />
          <PresetGroup
            title={t('userPresets.title')}
            description={t('userPresets.description')}
            presets={presets.data.user}
            canDelete
            testId="preset-group-user"
            onReload={(id) => reload.mutate(id)}
            onValidate={() => setValidateOpen(true)}
            onDelete={(id) => setDeleteTarget(id)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('userPresets.empty')}
          />
          <PresetGroup
            title={t('systemPresets.title')}
            description={t('systemPresets.description')}
            presets={systemPresets}
            testId="preset-group-system"
            onReload={(id) => reload.mutate(id)}
            onValidate={() => setValidateOpen(true)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('systemPresets.empty')}
          />
          <PresetGroup
            title={t('embeddedPresets.title')}
            description={t('embeddedPresets.description')}
            presets={presets.data.embedded}
            testId="preset-group-embedded"
            onReload={(id) => reload.mutate(id)}
            onValidate={() => setValidateOpen(true)}
            onSelect={(id) => navigate(`/strategies/${encodeURIComponent(id)}`)}
            reloadingId={reload.isPending ? reload.variables : undefined}
            empty={t('embeddedPresets.empty')}
          />
        </div>
      )}

      <ScaffoldPresetDialog open={scaffoldOpen} onOpenChange={setScaffoldOpen} />
      <ValidatePresetDialog open={validateOpen} onOpenChange={setValidateOpen} />

      <Dialog open={deleteTarget !== null} onOpenChange={(o) => !o && setDeleteTarget(null)}>
        <DialogContent
          title={t('deleteConfirm.title', { name: deleteTarget ?? '' })}
          description={t('deleteConfirm.description')}
        >
          <div className="flex justify-end gap-2">
            <Button type="button" variant="tertiary" size="small" onClick={() => setDeleteTarget(null)}>
              {t('common:action.cancel')}
            </Button>
            <Button
              type="button"
              variant="primary"
              size="small"
              onClick={confirmDelete}
              disabled={removePreset.isPending}
            >
              {removePreset.isPending ? t('deleteConfirm.deleting') : t('deleteConfirm.delete')}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** Hide internal `_system.` prefixed presets from authors (AD-P0-3). */
function isAuthorSystemPreset(preset: PresetSummary): boolean {
  return !preset.id.startsWith('_system.');
}

/**
 * V1.171 P1 — Develop strategy catalog (PL-8/PL-9, AR-27/AR-28).
 *
 * Lists USER + embedded (non-hidden) presets with trigger-lane badges and
 * honest entry paths, read from each preset's P0 profile through
 * `NexusClient` (`usePresetProfile`). The preset LIST endpoint returns ids
 * only — no lane/role/capability data is derived client-side from list
 * facts (AR-27). A missing profile renders a graceful summary (id + list
 * facts), never a hard "preset gone" error (PL-13 boundary).
 *
 * The catalog attaches to the existing `/strategies` route, already
 * `develop-only` in `ENTRANCE_ROUTE_RULES` (AR-28) — no new guard
 * mechanism; the creator entrance bounces via the existing rule table.
 */
function StrategyCatalog({
  presets,
  onSelect,
}: {
  presets: PresetGroups;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation('strategies');
  // USER + embedded non-hidden presets (PL-8). Non-hidden = not a `_system.`
  // internal (AD-P0-3 filter reuse). The list endpoint returns ids only —
  // lane data comes from each preset's profile (AR-27).
  const catalogPresets = useMemo(
    () => [
      ...presets.user,
      ...presets.embedded.filter((preset) => !preset.id.startsWith('_system.')),
    ],
    [presets],
  );

  return (
    <Card className="shadow-card" data-testid="strategy-catalog">
      <CardHeader>
        <CardTitle>{t('catalog.title')}</CardTitle>
        <CardDescription>{t('catalog.description')}</CardDescription>
      </CardHeader>
      <CardContent>
        {catalogPresets.length === 0 ? (
          <EmptyState title={t('catalog.empty')} />
        ) : (
          <ul className="flex flex-col gap-2">
            {catalogPresets.map((preset) => (
              <CatalogRow key={preset.id} preset={preset} onSelect={onSelect} />
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function CatalogRow({
  preset,
  onSelect,
}: {
  preset: PresetSummary;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation('strategies');
  const profile = usePresetProfile(preset.id);
  const lanes = profile.data?.lanes;
  // PL-8 vocabulary: trigger lane = the integrator or product fires it
  // (session start / direct run); scheduled lane = time-driven entry
  // (daemon schedule and/or Work cron).
  const triggerLane = lanes ? lanes.session || lanes.direct : false;
  const scheduledLane = lanes ? lanes.wallClock || lanes.cron : false;

  return (
    <li
      className="flex flex-col gap-2 rounded-card border border-gray-alpha-400 p-3"
      data-testid={`catalog-row-${preset.id}`}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <button
          type="button"
          onClick={() => onSelect(preset.id)}
          className="flex items-center gap-2 text-left"
          aria-label={t('catalog.openAria', { name: preset.id })}
        >
          <Sparkles className="h-4 w-4 text-purple-700" aria-hidden />
          <span className="text-copy-13-mono text-gray-1000">{preset.id}</span>
        </button>
        <div className="flex flex-wrap items-center gap-2">
          {lanes && triggerLane && (
            <Badge variant="running" data-testid={`catalog-trigger-${preset.id}`}>
              {t('catalog.triggerBadge')}
            </Badge>
          )}
          {lanes && scheduledLane && (
            <Badge variant="queued" data-testid={`catalog-scheduled-${preset.id}`}>
              {t('catalog.scheduledBadge')}
            </Badge>
          )}
          <Badge variant="preset" data-testid={`catalog-source-${preset.id}`}>
            {preset.source === 'user' ? t('catalog.sourceUser') : t('catalog.sourceEmbedded')}
          </Badge>
        </div>
      </div>
      {lanes ? (
        <div className="flex flex-col gap-1" data-testid={`catalog-paths-${preset.id}`}>
          <span className="text-label-12 text-gray-700">{t('catalog.entryPathsTitle')}</span>
          <ul className="flex flex-col gap-1">
            {triggerLane && (
              <li className="flex flex-col gap-0.5">
                <span className="text-copy-13 text-gray-900">{t('catalog.pathConnect')}</span>
                <span className="text-copy-12 text-gray-700">{t('catalog.pathConnectDetail')}</span>
              </li>
            )}
            {lanes.wallClock && <li className="text-copy-13 text-gray-900">{t('catalog.pathDaemon')}</li>}
            {lanes.cron && (
              <li className="flex flex-col gap-0.5">
                <span className="text-copy-13 text-gray-900">{t('catalog.pathCron')}</span>
                <span className="text-copy-12 text-gray-700">{t('catalog.pathCronDetail')}</span>
              </li>
            )}
          </ul>
        </div>
      ) : profile.isLoading ? (
        <p className="text-copy-12 text-gray-700" data-testid={`catalog-lanes-loading-${preset.id}`}>
          {t('catalog.lanesLoading')}
        </p>
      ) : (
        <p
          className="text-copy-12 text-gray-700"
          data-testid={`catalog-profile-unavailable-${preset.id}`}
        >
          {t('catalog.profileUnavailable')}
        </p>
      )}
    </li>
  );
}

function PresetGroup({
  title,
  description,
  presets,
  canDelete = false,
  testId,
  onReload,
  onValidate,
  onDelete,
  onSelect,
  reloadingId,
  empty,
}: {
  title: string;
  description: string;
  presets: PresetSummary[];
  canDelete?: boolean;
  testId?: string;
  onReload: (id: string) => void;
  onValidate: () => void;
  onDelete?: (id: string) => void;
  onSelect: (id: string) => void;
  reloadingId: string | undefined;
  empty: string;
}) {
  const { t } = useTranslation('strategies');
  return (
    <Card className="shadow-card" data-testid={testId}>
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
                <div className="flex items-center gap-2">
                  <Button type="button" variant="tertiary" size="small" onClick={onValidate}>
                    {t('validatePreset')}
                  </Button>
                  <Button
                    type="button"
                    variant="tertiary"
                    size="small"
                    onClick={() => onReload(p.id)}
                    disabled={reloadingId === p.id}
                  >
                    {reloadingId === p.id ? t('reloading') : t('reload')}
                  </Button>
                  {canDelete && onDelete && (
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      onClick={() => onDelete(p.id)}
                      aria-label={t('deleteAria', { name: p.id })}
                    >
                      <Trash2 className="h-4 w-4" aria-hidden />
                      {t('delete')}
                    </Button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
