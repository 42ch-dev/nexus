/**
 * Inspector panel — the aside that hosts the three per-section inspectors.
 *
 * Renders the selected-node header, the edit/read-only toggle, and the
 * state/transition/prompt sections. The shell is kept here so the orchestrator
 * stays under the 200-line limit (R-V171P0-QC1-006).
 */
import { useMemo } from 'react';
import type { Node } from '@xyflow/react';
import { Info, Pencil, X } from 'lucide-react';

import { ConflictModal } from '@/components/canvas/conflict-modal';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import type { PresetState } from '@/lib/canvas/preset-yaml';
import { getChangedFields, getConflictDraft, originalFormOf, templateRefOf, type EditForm } from './state-machine';
import type { Section } from './state-machine';

interface InspectorPanelProps {
  selected: Node | null;
  selectedState: PresetState | undefined;
  isEditing: boolean;
  setIsEditing: (v: boolean) => void;
  onFocusSection: (section: import('./state-machine').Section) => void;
  children: React.ReactNode;
}

export function InspectorPanel({
  selected,
  selectedState,
  isEditing,
  setIsEditing,
  onFocusSection,
  children,
}: InspectorPanelProps) {
  const d = useMemo(() => (selected?.data as StrategyNodeData | undefined) ?? undefined, [selected]);
  if (!selected || !selectedState || !d) return null;

  return (
    <aside
      className="absolute right-3 top-3 w-[280px] rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover"
      aria-label="Selected node details"
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <Info className="h-4 w-4 text-purple-700" aria-hidden />
          <h3 className="text-heading-16 font-heading text-gray-1000">{d.label}</h3>
        </div>
        {!isEditing ? (
          <button
            type="button"
            onClick={() => setIsEditing(true)}
            className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
            aria-label="Edit state"
          >
            <Pencil className="h-4 w-4" aria-hidden />
          </button>
        ) : (
          <button
            type="button"
            onClick={() => setIsEditing(false)}
            className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
            aria-label="Cancel editing"
          >
            <X className="h-4 w-4" aria-hidden />
          </button>
        )}
      </div>
      {isEditing ? (
        <div className="mt-3 flex flex-col gap-4" onFocusCapture={() => onFocusSection('state')}>{children}</div>
      ) : (
        <ReadOnlyDetails d={d} selectedState={selectedState} />
      )}
    </aside>
  );
}

function ReadOnlyDetails({ d, selectedState }: { d: StrategyNodeData; selectedState: PresetState }) {
  const promptTemplateRef = templateRefOf(selectedState);
  return (
    <dl className="mt-2 flex flex-col gap-1 text-copy-13">
      <div className="flex justify-between">
        <dt className="text-gray-700">Kind</dt>
        <dd className="font-mono text-gray-1000">{d.stateKind}</dd>
      </div>
      <div className="flex justify-between">
        <dt className="text-gray-700">State id</dt>
        <dd className="font-mono text-gray-1000">{d.stateId}</dd>
      </div>
      {d.innerGraphId ? (
        <div className="flex justify-between">
          <dt className="text-gray-700">Inner graph</dt>
          <dd className="font-mono text-gray-1000">{d.innerGraphId}</dd>
        </div>
      ) : null}
      {d.convergeStrategy ? (
        <div className="flex justify-between">
          <dt className="text-gray-700">Converge</dt>
          <dd className="font-mono text-gray-1000">{d.convergeStrategy}</dd>
        </div>
      ) : null}
      {d.isInitial ? <div className="text-purple-700">Initial state</div> : null}
      {d.isTerminal ? <div className="text-gray-700">Terminal state</div> : null}
      {d.status ? (
        <div className="flex justify-between">
          <dt className="text-gray-700">Status</dt>
          <dd className="text-blue-700">{d.status}</dd>
        </div>
      ) : null}
      {selectedState.description ? <p className="mt-2 text-gray-900">{selectedState.description}</p> : null}
      {promptTemplateRef ? (
        <p className="mt-2 text-gray-700">
          Prompt: <span className="font-mono">{promptTemplateRef}</span>
        </p>
      ) : null}
    </dl>
  );
}

interface StrategyConflictModalProps {
  conflict: { currentRevision: number; section: Section } | null;
  form: EditForm;
  canonicalState: PresetState | undefined;
  promptTemplateRef: string | undefined;
  onUseCurrent: () => void;
  onReapply: () => void;
  onDismiss: () => void;
}

export function StrategyConflictModal({
  conflict,
  form,
  canonicalState,
  promptTemplateRef,
  onUseCurrent,
  onReapply,
  onDismiss,
}: StrategyConflictModalProps) {
  if (!conflict || !canonicalState) return null;
  return (
    <ConflictModal
      open
      currentRevision={conflict.currentRevision}
      draft={getConflictDraft(form)}
      canonicalState={canonicalState}
      promptTemplateRef={promptTemplateRef}
      changedFields={getChangedFields(form, originalFormOf(canonicalState))}
      onUseCurrent={onUseCurrent}
      onReapply={onReapply}
      onDismiss={onDismiss}
    />
  );
}
