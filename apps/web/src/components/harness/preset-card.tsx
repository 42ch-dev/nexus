/**
 * One Harness catalog row (plan v1220 T2): preset name, list facts, strategy
 * binding state, and running-session state.
 *
 * Author actions live on the row: the canvas drill-down (row name), the
 * profile drill-down link, Validate (dry-run of this preset's manifest),
 * Reload (re-read it from disk), and — user presets only — Delete. Binding is
 * derived from the preset profile's `states.length > 0`; a failed profile
 * renders an explicit unknown state — never "not bound", which would assert a
 * fact the read never established (PL-13 keeps the row itself listed).
 */
import { useTranslation } from 'react-i18next';
import { Sparkles, Trash2 } from 'lucide-react';
import type { PresetSummary } from '@42ch/nexus-contracts';

import type { PresetProfileQuery } from '@/api/queries';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';

const SOURCE_LABEL_KEY: Record<PresetSummary['source'], string> = {
  user: 'harness.sourceUser',
  embedded: 'harness.sourceEmbedded',
  system: 'harness.sourceSystem',
};

/**
 * Running-session state for one catalog row. `error` renders an explicit
 * unknown badge (never `idle`); `truncated` marks the count as a lower bound.
 */
export type PresetRunningState =
  | { status: 'ready'; count: number; truncated: boolean }
  | { status: 'loading' }
  | { status: 'error' };

export interface PresetCardProps {
  preset: PresetSummary;
  profile: PresetProfileQuery;
  running: PresetRunningState;
  onOpenCanvas: () => void;
  onOpenProfile: () => void;
  /** Open the dry-run validate dialog for this row. */
  onValidate: () => void;
  /** Re-read this preset's manifest from disk. */
  onReload: () => void;
  /** True while this row's reload is in flight. */
  reloading: boolean;
  /** User presets only — absent on embedded/system rows. */
  onDelete?: () => void;
}

export function PresetCard({
  preset,
  profile,
  running,
  onOpenCanvas,
  onOpenProfile,
  onValidate,
  onReload,
  reloading,
  onDelete,
}: PresetCardProps) {
  const { t } = useTranslation('strategies');
  const bound = Boolean(profile.data && profile.data.states.length > 0);
  // "Not bound" is only claimable from a successful empty profile; a failed
  // read is unknown, exactly like the running column.
  const bindingBadge = profile.isError
    ? { variant: 'warning' as const, label: t('harness.bindingUnknown') }
    : profile.isLoading
      ? { variant: 'neutral' as const, label: t('harness.bindingLoading') }
      : bound
        ? { variant: 'running' as const, label: t('harness.bindingBound') }
        : { variant: 'neutral' as const, label: t('harness.bindingUnbound') };
  const source = t(SOURCE_LABEL_KEY[preset.source]);
  const intents = preset.run_intents?.join(' · ');
  const facts = intents ? t('harness.facts', { source, intents }) : source;

  const runningBadge =
    running.status === 'error'
      ? { variant: 'warning' as const, label: t('harness.runningUnknown') }
      : running.status === 'loading'
        ? { variant: 'neutral' as const, label: t('harness.runningLoading') }
        : running.truncated
          ? {
              // A truncated walk is a lower bound even at zero: never `idle`.
              variant: running.count > 0 ? ('queued' as const) : ('neutral' as const),
              label: t('harness.runningTruncated', { count: running.count }),
            }
          : running.count > 0
            ? {
                variant: 'queued' as const,
                label: t('harness.running', { count: running.count }),
              }
            : { variant: 'neutral' as const, label: t('harness.idle') };

  return (
    <li
      data-testid={`preset-row-${preset.id}`}
      className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-gray-alpha-400 p-3"
    >
      <div className="flex min-w-0 flex-col gap-1">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onOpenCanvas}
            className="flex items-center gap-2 text-left"
            aria-label={t('harness.openCanvas', { name: preset.id })}
          >
            <Sparkles className="h-4 w-4 text-purple-700" aria-hidden />
            <span className="text-copy-13-mono text-gray-1000">{preset.id}</span>
          </button>
          <button
            type="button"
            onClick={onOpenProfile}
            className="text-label-12 text-blue-700 underline-offset-2 hover:underline"
          >
            {t('harness.openProfile')}
          </button>
        </div>
        <span
          className="text-copy-12 text-gray-700"
          data-testid={`preset-facts-${preset.id}`}
        >
          {facts}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Badge
          variant={bindingBadge.variant}
          data-testid={`preset-binding-${preset.id}`}
        >
          {bindingBadge.label}
        </Badge>
        <Badge
          variant={runningBadge.variant}
          data-testid={`preset-running-${preset.id}`}
        >
          {runningBadge.label}
        </Badge>
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onValidate}
          data-testid={`preset-validate-${preset.id}`}
        >
          {t('validatePreset')}
        </Button>
        <Button
          type="button"
          variant="tertiary"
          size="small"
          onClick={onReload}
          disabled={reloading}
          data-testid={`preset-reload-${preset.id}`}
        >
          {reloading ? t('reloading') : t('reload')}
        </Button>
        {onDelete && (
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={onDelete}
            aria-label={t('deleteAria', { name: preset.id })}
            data-testid={`preset-delete-${preset.id}`}
          >
            <Trash2 className="h-4 w-4" aria-hidden />
            {t('delete')}
          </Button>
        )}
      </div>
    </li>
  );
}
