/**
 *
 * Contains graph/state projection helpers, the revision badge, validation
 * panel, steering artifacts list, and the edit-form shape used by the three
 * inspector sections.
 */
import { type Node } from '@xyflow/react';
import { useTranslation } from 'react-i18next';
import { AlertTriangle, ScrollText } from 'lucide-react';

import type { IdeaArtifact } from '@/components/canvas/idea-input';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import type { PresetState } from '@/lib/canvas/preset-yaml';

export type Section = 'state' | 'transition' | 'prompt';

export interface EditForm {
  label: string;
  description: string;
  nextTarget: string;
  promptBody: string;
}

export function originalFormOf(state: PresetState | undefined): EditForm {
  return {
    label: state?.id ?? '',
    description: state?.description ?? '',
    nextTarget: typeof state?.next === 'string' ? state.next : '',
    promptBody: '',
  };
}

export function templateRefOf(state: PresetState | undefined): string | undefined {
  if (!state) return undefined;
  const task = state.enter?.find((e) => e.kind === 'acp_prompt');
  if (task?.name) return task.name;
  return state.context_update?.template_file;
}

export function isSectionDirty(section: Section, form: EditForm, original: EditForm): boolean {
  switch (section) {
    case 'state':
      return form.label !== original.label || form.description !== original.description;
    case 'transition':
      return form.nextTarget !== original.nextTarget;
    case 'prompt':
      return form.promptBody !== original.promptBody;
  }
}

export function RevisionBadge({
  revision,
  status,
}: {
  revision: number;
  status: 'clean' | 'dirty' | 'conflict';
}) {
  const { t } = useTranslation('canvas');
  const color =
    status === 'conflict'
      ? 'border-canvas-write-conflict text-canvas-write-conflict bg-canvas-write-conflict/10'
      : status === 'dirty'
        ? 'border-canvas-write-dirty text-canvas-write-dirty bg-canvas-write-dirty/10'
        : 'border-gray-alpha-400 text-gray-700 bg-background-100';
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-label-12 ${color}`}
      title={status === 'conflict' ? t('strategy.revisionBadge.conflictTitle') : undefined}
    >
      {status === 'conflict' ? <AlertTriangle className="h-3 w-3" aria-hidden /> : null}
      {t('strategy.revisionBadge.label', { revision })}
    </span>
  );
}

export function ValidationPanel({ problems, dangling }: { problems: string[]; dangling: string[] }) {
  const { t } = useTranslation('canvas');
  if (problems.length === 0 && dangling.length === 0) return null;
  return (
    <div
      className="absolute bottom-3 left-3 max-w-[360px] rounded-card border border-amber-700/40 bg-background-100 p-2 text-copy-13 shadow-popover"
      role="status"
    >
      <div className="flex items-center gap-1.5 text-amber-1000">
        <AlertTriangle className="h-4 w-4" aria-hidden />
        <span className="font-semibold">{t('strategy.validation.title')}</span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5 text-gray-900">
        {problems.map((p, i) => <li key={`p${i}`}>{p}</li>)}
        {dangling.map((d, i) => (
          <li key={`d${i}`} className="text-amber-1000">
            {t('strategy.validation.dangling', { target: d })}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function ArtifactsList({ artifacts }: { artifacts: IdeaArtifact[] }) {
  const { t } = useTranslation('canvas');
  return (
    <section
      aria-label={t('strategy.artifacts.ariaLabel')}
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-card"
    >
      <div className="flex items-center gap-2">
        <ScrollText className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('strategy.artifacts.title')}</h3>
      </div>
      {artifacts.length === 0 ? (
        <p className="mt-2 text-copy-13 text-gray-700">
          {t('strategy.artifacts.empty')}
        </p>
      ) : (
        <ul className="mt-2 flex flex-col gap-1.5">
          {artifacts.map((a) => (
            <li key={a.id} className="rounded-control border border-gray-alpha-300 px-2 py-1.5 text-copy-13">
              <span className="mr-1.5 rounded-pill bg-purple-700/10 px-1.5 py-0.5 text-label-12 text-purple-1000">
                {a.verb}
              </span>
              <span className="text-gray-1000">{a.text}</span>
              {a.target ? (
                <span className="ml-1 font-mono text-gray-700">→ {a.target.slice(0, 8)}</span>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function selectedStateOf(
  selected: Node | null,
  states: PresetState[] | undefined,
): PresetState | undefined {
  if (!selected || !states) return undefined;
  const stateId = (selected.data as StrategyNodeData).stateId;
  return states.find((s) => s.id === stateId);
}
