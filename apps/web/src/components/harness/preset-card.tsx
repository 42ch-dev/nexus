/**
 * One Harness catalog row (plan v1220 T2): preset name, list facts, strategy
 * binding state, and running-session state.
 *
 * Read-only: the two controls are the canvas drill-down (row name) and the
 * profile drill-down link. Binding is derived from the preset profile's
 * `states.length > 0`; a failed/404 profile is a graceful 「未绑定」 row
 * (PL-13) — the preset is still listed.
 */
import { useTranslation } from 'react-i18next';
import { Sparkles } from 'lucide-react';
import type { PresetSummary } from '@42ch/nexus-contracts';

import type { PresetProfileQuery } from '@/api/queries';
import { Badge } from '@/components/ui/badge';

const SOURCE_LABEL_KEY: Record<PresetSummary['source'], string> = {
  user: 'harness.sourceUser',
  embedded: 'harness.sourceEmbedded',
  system: 'harness.sourceSystem',
};

export interface PresetCardProps {
  preset: PresetSummary;
  profile: PresetProfileQuery;
  runningCount: number;
  onOpenCanvas: () => void;
  onOpenProfile: () => void;
}

export function PresetCard({
  preset,
  profile,
  runningCount,
  onOpenCanvas,
  onOpenProfile,
}: PresetCardProps) {
  const { t } = useTranslation('strategies');
  const bound = Boolean(profile.data && profile.data.states.length > 0);
  const source = t(SOURCE_LABEL_KEY[preset.source]);
  const intents = preset.run_intents?.join(' · ');
  const facts = intents ? t('harness.facts', { source, intents }) : source;

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
          variant={bound ? 'running' : 'neutral'}
          data-testid={`preset-binding-${preset.id}`}
        >
          {profile.isLoading
            ? t('harness.bindingLoading')
            : bound
              ? t('harness.bindingBound')
              : t('harness.bindingUnbound')}
        </Badge>
        <Badge
          variant={runningCount > 0 ? 'queued' : 'neutral'}
          data-testid={`preset-running-${preset.id}`}
        >
          {runningCount > 0 ? t('harness.running', { count: runningCount }) : t('harness.idle')}
        </Badge>
      </div>
    </li>
  );
}
