/**
 * Strategy-canvas layout pieces — header (title + revision + alt view toggle +
 * live-session banner) and footer (idea input + steering artifacts).
 *
 * Extracted so the orchestrator stays under the 200-line limit
 * (R-V171P0-QC1-006).
 */
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus } from 'lucide-react';

import { IdeaInput, type IdeaArtifact } from '@/components/canvas/idea-input';

import { ArtifactsList, RevisionBadge } from './state-machine';

export function CanvasHeader({
  revision,
  status,
  activeSession,
  showAlt,
  setShowAlt,
  onOpenCreateTransition,
}: {
  revision: number;
  status: 'clean' | 'dirty' | 'conflict';
  activeSession: { current_task_id?: string; status: string } | null | undefined;
  showAlt: boolean;
  setShowAlt: (v: boolean) => void;
  /**
   * FB-SE-004 — when provided, the header renders a **Create Transition…**
   * button and binds the Shift+N keyboard shortcut that opens the keyboard
   * edge-creation dialog. The shortcut is suppressed while focus is in a text
   * field, select, or contenteditable so authors can type the letter N.
   */
  onOpenCreateTransition?: () => void;
}) {
  const { t } = useTranslation('canvas');

  // Shift+N opens the keyboard edge-creation dialog (FB-SE-004 §4.4). Suppressed
  // inside text-entry controls so the letter is passed through to the field.
  useEffect(() => {
    if (!onOpenCreateTransition) return;
    const open = onOpenCreateTransition;
    function onKeyDown(e: KeyboardEvent) {
      if (!e.shiftKey || e.key !== 'N') return;
      const target = e.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName;
        if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target.isContentEditable) return;
      }
      e.preventDefault();
      open();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onOpenCreateTransition]);

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-heading-20 font-heading text-gray-1000">{t('strategy.header.title')}</h2>
            <RevisionBadge revision={revision} status={status} />
          </div>
          <p className="text-copy-13 text-gray-700">
            {t('strategy.header.description')}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {onOpenCreateTransition ? (
            <button
              type="button"
              onClick={onOpenCreateTransition}
              className="inline-flex items-center gap-1 rounded-control bg-purple-700 px-3 py-1.5 text-button-12 text-white hover:bg-purple-800"
            >
              <Plus className="h-3.5 w-3.5" aria-hidden />
              {t('strategy.header.createTransition')}
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => setShowAlt(!showAlt)}
            aria-pressed={showAlt}
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
          >
            {showAlt ? t('strategy.header.showGraph') : t('strategy.header.showList')}
          </button>
        </div>
      </div>
      {activeSession ? (
        <div className="flex items-center gap-2 rounded-card border border-info-surface-border bg-info-surface px-3 py-2 text-copy-13 text-gray-900">
          <span className="inline-block h-2 w-2 rounded-pill bg-blue-700" aria-hidden />
          {t('strategy.header.livePrefix')}{' '}
          <span className="font-mono">{activeSession.current_task_id ?? '—'}</span> · {t('strategy.header.liveStatus')}{' '}
          {activeSession.status}
        </div>
      ) : null}
    </>
  );
}

export function CanvasFooter({
  presetId,
  creatorId,
  scheduleId,
  artifacts,
  setArtifacts,
}: {
  presetId: string;
  creatorId: string | undefined;
  scheduleId: string | undefined;
  artifacts: IdeaArtifact[];
  setArtifacts: (v: IdeaArtifact[] | ((prev: IdeaArtifact[]) => IdeaArtifact[])) => void;
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
      <IdeaInput
        presetId={presetId}
        creatorId={creatorId}
        scheduleId={scheduleId}
        onArtifact={(a) => setArtifacts((prev) => [a, ...prev].slice(0, 12))}
      />
      <ArtifactsList artifacts={artifacts} />
    </div>
  );
}
