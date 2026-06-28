/**
 * Strategy-canvas layout pieces — header (title + revision + alt view toggle +
 * live-session banner) and footer (idea input + steering artifacts).
 *
 * Extracted so the orchestrator stays under the 200-line limit
 * (R-V171P0-QC1-006).
 */
import { IdeaInput, type IdeaArtifact } from '@/components/canvas/idea-input';

import { ArtifactsList, RevisionBadge } from './state-machine';

export function CanvasHeader({
  revision,
  status,
  activeSession,
  showAlt,
  setShowAlt,
}: {
  revision: number;
  status: 'clean' | 'dirty' | 'conflict';
  activeSession: { current_task_id?: string; status: string } | null | undefined;
  showAlt: boolean;
  setShowAlt: (v: boolean) => void;
}) {
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-heading-20 font-heading text-gray-1000">Strategy</h2>
            <RevisionBadge revision={revision} status={status} />
          </div>
          <p className="text-copy-13 text-gray-700">
            Preset as a state-machine graph. Select a state to edit it; the revision badge shows graph freshness.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setShowAlt(!showAlt)}
          aria-pressed={showAlt}
          className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-button-12 text-gray-900 hover:bg-gray-alpha-100"
        >
          {showAlt ? 'Show graph' : 'Show list view'}
        </button>
      </div>
      {activeSession ? (
        <div className="flex items-center gap-2 rounded-card border border-blue-700/30 bg-[color-mix(in_srgb,var(--color-blue-700)_6%,transparent)] px-3 py-2 text-copy-13 text-gray-900">
          <span className="inline-block h-2 w-2 rounded-pill bg-blue-700" aria-hidden />
          Live: node <span className="font-mono">{activeSession.current_task_id ?? '—'}</span> · status {activeSession.status}
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
