/**
 * Strategy canvas — the β-scope Strategy (Preset) write surface
 * (canvas-strategy-surface.md §3.2/§3.3/§3.5/§3.7).
 *
 * Composes: shared Canvas Shell (A1) + Strategy graph adapter (A2) + bounded
 * live overlay (A3) + Idea-input steering (A4) + editable side inspector (A6)
 * + conflict modal (A7) + graph-revision freshness indicator (A7) +
 * validation panel + accessibility alternate view (A8).
 *
 * UI label is "Strategy"; persisted identifiers remain "preset" (Draft §4.2).
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import { useEdgesState, useNodesState, type Edge, type Node } from '@xyflow/react';
import { AlertTriangle, Info, Pencil, Save, ScrollText, X } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { ConflictModal, type ConflictModalDraft, type ChangedField } from '@/components/canvas/conflict-modal';
import { IdeaInput, type IdeaArtifact } from '@/components/canvas/idea-input';
import { strategyNodeTypes } from '@/components/canvas/strategy-nodes';
import { StrategyAltView } from '@/components/canvas/strategy-alt-view';
import {
  useActiveSession,
  useDerivedCreatorId,
  usePatchStrategyPromptTemplate,
  usePatchStrategyState,
  usePatchStrategyTransition,
  usePresetGraph,
  usePresetSchedules,
  isStrategyConflictError,
  type PatchStrategyStateArgs,
  type PatchStrategyTransitionArgs,
} from '@/lib/canvas/use-strategy-data';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import type { PresetState } from '@/lib/canvas/preset-yaml';
import { ErrorState, LoadingState } from '@/components/ui/states';

export interface StrategyCanvasProps {
  presetId: string;
}

interface EditForm {
  label: string;
  description: string;
  nextTarget: string;
  promptBody: string;
}

export function StrategyCanvas({ presetId }: StrategyCanvasProps) {
  const graphQuery = usePresetGraph(presetId);
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const patchState = usePatchStrategyState();
  const patchTransition = usePatchStrategyTransition();
  const patchPrompt = usePatchStrategyPromptTemplate();

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [showAlt, setShowAlt] = useState(false);

  const [isEditing, setIsEditing] = useState(false);
  const [form, setForm] = useState<EditForm>({
    label: '',
    description: '',
    nextTarget: '',
    promptBody: '',
  });
  const [dirty, setDirty] = useState(false);
  const [conflict, setConflict] = useState<{ currentRevision: number } | null>(null);

  // Sync the built graph into React Flow state when the preset changes.
  useEffect(() => {
    if (graphQuery.data) {
      setNodes(graphQuery.data.graph.nodes as Node[]);
      setEdges(graphQuery.data.graph.edges as Edge[]);
    }
  }, [graphQuery.data, setNodes, setEdges]);

  // Bounded live overlay (A3): highlight the current node + propagate status.
  useEffect(() => {
    if (!activeSession) {
      setNodes((nds) => nds.map((n) => ({ ...n, data: { ...n.data, status: undefined } })));
      return;
    }
    const currentTask = activeSession.current_task_id;
    const sessionStatus = activeSession.status;
    setNodes((nds) =>
      nds.map((n) => {
        const data = n.data as StrategyNodeData;
        const isCurrent =
          currentTask !== undefined &&
          (n.id === currentTask || data.stateId === currentTask || n.id.startsWith(`${currentTask}::`));
        return {
          ...n,
          data: { ...data, status: isCurrent ? sessionStatus ?? '__current__' : undefined },
        };
      }),
    );
  }, [activeSession, setNodes]);

  const selected = useMemo(() => nodes.find((n) => n.selected) ?? null, [nodes]);

  const selectedState = useMemo<PresetState | undefined>(() => {
    if (!selected || !graphQuery.data) return undefined;
    const stateId = (selected.data as StrategyNodeData).stateId;
    return graphQuery.data.parsed.manifest.states.find((s) => s.id === stateId);
  }, [selected, graphQuery.data]);

  const baseRevision = graphQuery.data?.revision ?? 0;
  const promptTemplateRef = useMemo(() => templateRefOf(selectedState), [selectedState]);

  // Track the revision we are patching against. After each successful partial
  // patch the daemon bumps revision, so the next mutation must use the new
  // base. This prevents a single multi-field Save from conflicting with itself
  // (R-V171-P0-QC3-C3).
  //
  // A `useRef` (not state) is used so closures invoked from microtasks — e.g.
  // `graphQuery.refetch().then(handleSave)` inside `onReapply` — read the
  // post-refetch value without waiting for the next render to commit.
  // R-V171-GREPTILE-P1-1.
  const workingRevisionRef = useRef(baseRevision);
  useEffect(() => {
    workingRevisionRef.current = baseRevision;
  }, [baseRevision]);

  // Initialise the edit form when the user opens edit mode or selects a node.
  useEffect(() => {
    if (!isEditing || !selectedState) {
      setForm({ label: '', description: '', nextTarget: '', promptBody: '' });
      setDirty(false);
      return;
    }
    const next = selectedState.next;
    setForm({
      label: selectedState.id,
      description: selectedState.description ?? '',
      nextTarget: typeof next === 'string' ? next : '',
      promptBody: '',
    });
    setDirty(false);
  }, [isEditing, selectedState]);

  // Clear conflict when the graph is refetched.
  useEffect(() => {
    if (conflict && graphQuery.data && graphQuery.data.revision !== conflict.currentRevision) {
      setConflict(null);
    }
  }, [graphQuery.data, conflict]);

  const statusByState = useMemo(() => {
    const map: Record<string, string> = {};
    if (activeSession?.current_task_id) map[activeSession.current_task_id] = activeSession.status;
    return map;
  }, [activeSession]);

  const summaryText = useMemo(() => {
    const count = nodes.length;
    const edgeCount = edges.length;
    const sel = selected ? ` Selected: ${selected.id}.` : '';
    const live = activeSession
      ? ` Current node: ${activeSession.current_task_id ?? 'none'}. Session status: ${activeSession.status}.`
      : ' No active session.';
    return `Strategy graph: ${count} states, ${edgeCount} transitions.${live}${sel}`;
  }, [nodes.length, edges.length, selected, activeSession]);

  async function handleSave() {
    if (!selectedState || !graphQuery.data) return;

    const original = originalFormOf(selectedState);
    const renamedStateId = form.label !== original.label ? form.label : selectedState.id;

    try {
      // Read the latest base revision from the ref so closures invoked from
      // microtasks (e.g. `graphQuery.refetch().then(handleSave)` inside
      // `onReapply`) see the post-refetch value, not the pre-refetch render's
      // `workingRevision` state. R-V171-GREPTILE-P1-1.
      let currentRevision = workingRevisionRef.current;

      if (form.label !== original.label || form.description !== original.description) {
        const args: PatchStrategyStateArgs = {
          strategyId: presetId,
          stateId: selectedState.id,
          baseRevision: currentRevision,
        };
        if (form.label !== original.label) args.label = form.label;
        if (form.description !== original.description) args.description = form.description;
        const res = await patchState.mutateAsync(args);
        currentRevision = Number(res.new_revision);
      }

      if (form.nextTarget !== original.nextTarget && typeof selectedState.next === 'string') {
        const args: PatchStrategyTransitionArgs = {
          strategyId: presetId,
          sourceStateId: renamedStateId,
          baseRevision: currentRevision,
          oldTarget: original.nextTarget,
          newTarget: form.nextTarget,
          transitionKind: 'next',
        };
        const res = await patchTransition.mutateAsync(args);
        currentRevision = Number(res.new_revision);
      }

      if (form.promptBody && promptTemplateRef) {
        const res = await patchPrompt.mutateAsync({
          strategyId: presetId,
          stateId: selectedState.id,
          baseRevision: currentRevision,
          templateRef: promptTemplateRef,
          body: form.promptBody,
        });
        currentRevision = Number(res.new_revision);
      }

      workingRevisionRef.current = currentRevision;
      setIsEditing(false);
    } catch (error) {
      if (isStrategyConflictError(error)) {
        const currentRevision =
          typeof error.details === 'object' && error.details !== null
            ? (error.details as { current_revision?: number }).current_revision ?? 0
            : 0;
        setConflict({ currentRevision });
        // Fetch the canonical graph immediately so the modal can show the
        // server state alongside the user's draft (R-V171-P0-QC3-W2).
        void graphQuery.refetch();
      }
      // Other errors are surfaced by the mutation's onError toast.
    }
  }

  function originalFormOf(state: PresetState | undefined): EditForm {
    return {
      label: state?.id ?? '',
      description: state?.description ?? '',
      nextTarget: typeof state?.next === 'string' ? state.next : '',
      promptBody: '',
    };
  }

  function updateField<K extends keyof EditForm>(field: K, value: EditForm[K]) {
    setForm((prev) => ({ ...prev, [field]: value }));
    setDirty(true);
  }

  if (graphQuery.isLoading) return <LoadingState label="Loading Strategy…" />;
  if (graphQuery.isError) return <ErrorState description="Could not load the Strategy preset." onRetry={() => graphQuery.refetch()} />;

  const parsed = graphQuery.data?.parsed;
  const problems = parsed?.problems ?? [];
  const dangling = graphQuery.data?.graph.danglingTargets ?? [];

  const activeScheduleId = activeSession
    ? [...(schedules.data ?? [])].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]
        ?.schedule_id
    : undefined;

  const revisionStatus = conflict
    ? 'conflict'
    : dirty
      ? 'dirty'
      : 'clean';

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-heading-20 font-heading text-gray-1000">Strategy</h2>
            <RevisionBadge revision={baseRevision} status={revisionStatus} />
          </div>
          <p className="text-copy-13 text-gray-700">
            Preset <span className="font-mono">{presetId}</span> as a state-machine graph. Select a state to edit it; the revision badge shows graph freshness.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setShowAlt((v) => !v)}
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

      {showAlt && parsed ? (
        <StrategyAltView parsed={parsed} statusByState={statusByState} />
      ) : (
        <CanvasShell
          nodes={nodes}
          edges={edges}
          nodeTypes={strategyNodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          summaryText={summaryText}
          ariaLabel="Strategy state-machine graph"
        >
          {/* Side inspector (editable at β) */}
          <InspectorOverlay
            selected={selected}
            selectedState={selectedState}
            isEditing={isEditing}
            setIsEditing={setIsEditing}
            form={form}
            onChange={updateField}
            dirty={dirty}
            onSave={handleSave}
            saving={patchState.isPending || patchTransition.isPending || patchPrompt.isPending}
            promptTemplateRef={promptTemplateRef}
          />
          {/* Validation panel */}
          <ValidationPanel problems={problems} dangling={dangling} />
        </CanvasShell>
      )}

      {conflict ? (
        <ConflictModal
          open
          currentRevision={conflict.currentRevision}
          draft={getConflictDraft(form)}
          canonicalState={selectedState}
          promptTemplateRef={promptTemplateRef}
          changedFields={getChangedFields(form, originalFormOf(selectedState))}
          onUseCurrent={() => {
            setConflict(null);
            setIsEditing(false);
            void graphQuery.refetch();
          }}
          onReapply={() => {
            setConflict(null);
            void graphQuery.refetch().then(() => handleSave());
          }}
          onDismiss={() => setConflict(null)}
        />
      ) : null}

      <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
        <IdeaInput
          presetId={presetId}
          creatorId={creatorId}
          scheduleId={activeScheduleId}
          onArtifact={(a) => setArtifacts((prev) => [a, ...prev].slice(0, 12))}
        />
        <ArtifactsList artifacts={artifacts} />
      </div>
    </div>
  );
}

function RevisionBadge({ revision, status }: { revision: number; status: 'clean' | 'dirty' | 'conflict' }) {
  const color =
    status === 'conflict'
      ? 'border-canvas-write-conflict text-canvas-write-conflict bg-canvas-write-conflict/10'
      : status === 'dirty'
        ? 'border-canvas-write-dirty text-canvas-write-dirty bg-canvas-write-dirty/10'
        : 'border-gray-alpha-400 text-gray-700 bg-background-100';
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-pill border px-2 py-0.5 text-label-12 ${color}`}
      title={status === 'conflict' ? 'Revision conflict — refetch before editing' : undefined}
    >
      {status === 'conflict' ? <AlertTriangle className="h-3 w-3" aria-hidden /> : null}
      rev {revision}
    </span>
  );
}

function templateRefOf(state: PresetState | undefined): string | undefined {
  if (!state) return undefined;
  // Primary: an `acp_prompt` enter task names the template directly.
  const task = state.enter?.find((e) => e.kind === 'acp_prompt');
  if (task?.name) return task.name;
  // Fallback: states may wire a prompt via a `context_update` hook whose
  // `template_file` points at the same bundle-relative prompt path.
  return state.context_update?.template_file;
}

interface InspectorOverlayProps {
  selected: Node | null;
  selectedState: PresetState | undefined;
  isEditing: boolean;
  setIsEditing: (v: boolean) => void;
  form: EditForm;
  onChange: <K extends keyof EditForm>(field: K, value: EditForm[K]) => void;
  dirty: boolean;
  onSave: () => void;
  saving: boolean;
  promptTemplateRef: string | undefined;
}

function InspectorOverlay({
  selected,
  selectedState,
  isEditing,
  setIsEditing,
  form,
  onChange,
  dirty,
  onSave,
  saving,
  promptTemplateRef,
}: InspectorOverlayProps) {
  if (!selected || !selectedState) return null;
  const d = selected.data as StrategyNodeData;
  const hasScalarNext = typeof selectedState.next === 'string';

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
          <div className="flex gap-1">
            <button
              type="button"
              onClick={onSave}
              disabled={!dirty || saving}
              className="rounded-control p-1 text-canvas-write-success hover:bg-gray-alpha-100 disabled:text-gray-500"
              aria-label="Save changes"
            >
              <Save className="h-4 w-4" aria-hidden />
            </button>
            <button
              type="button"
              onClick={() => setIsEditing(false)}
              className="rounded-control p-1 text-gray-700 hover:bg-gray-alpha-100"
              aria-label="Cancel editing"
            >
              <X className="h-4 w-4" aria-hidden />
            </button>
          </div>
        )}
      </div>

      {isEditing ? (
        <div className="mt-3 flex flex-col gap-2">
          <label className="flex flex-col gap-1 text-copy-13">
            <span className="text-gray-700">Label / state id</span>
            <input
              type="text"
              value={form.label}
              onChange={(e) => onChange('label', e.target.value)}
              className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
            />
          </label>
          <label className="flex flex-col gap-1 text-copy-13">
            <span className="text-gray-700">Description</span>
            <textarea
              value={form.description}
              onChange={(e) => onChange('description', e.target.value)}
              rows={3}
              className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
            />
          </label>
          {hasScalarNext ? (
            <label className="flex flex-col gap-1 text-copy-13">
              <span className="text-gray-700">Transition target</span>
              <input
                type="text"
                value={form.nextTarget}
                onChange={(e) => onChange('nextTarget', e.target.value)}
                className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
              />
            </label>
          ) : null}
          {promptTemplateRef ? (
            <label className="flex flex-col gap-1 text-copy-13">
              <span className="text-gray-700">Prompt template</span>
              <span className="text-copy-13-mono text-gray-700">{promptTemplateRef}</span>
              <textarea
                value={form.promptBody}
                onChange={(e) => onChange('promptBody', e.target.value)}
                rows={4}
                placeholder="Enter new prompt body…"
                className="rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-gray-1000 focus:border-blue-700"
              />
            </label>
          ) : null}
        </div>
      ) : (
        <dl className="mt-2 flex flex-col gap-1 text-copy-13">
          <div className="flex justify-between"><dt className="text-gray-700">Kind</dt><dd className="font-mono text-gray-1000">{d.stateKind}</dd></div>
          <div className="flex justify-between"><dt className="text-gray-700">State id</dt><dd className="font-mono text-gray-1000">{d.stateId}</dd></div>
          {d.innerGraphId ? <div className="flex justify-between"><dt className="text-gray-700">Inner graph</dt><dd className="font-mono text-gray-1000">{d.innerGraphId}</dd></div> : null}
          {d.convergeStrategy ? <div className="flex justify-between"><dt className="text-gray-700">Converge</dt><dd className="font-mono text-gray-1000">{d.convergeStrategy}</dd></div> : null}
          {d.isInitial ? <div className="text-purple-700">Initial state</div> : null}
          {d.isTerminal ? <div className="text-gray-700">Terminal state</div> : null}
          {d.status ? <div className="flex justify-between"><dt className="text-gray-700">Status</dt><dd className="text-blue-700">{d.status}</dd></div> : null}
        </dl>
      )}
      {!isEditing && d.description ? <p className="mt-2 text-copy-13 text-gray-900">{d.description}</p> : null}
      {promptTemplateRef && !isEditing ? (
        <p className="mt-2 text-copy-13 text-gray-700">Prompt: <span className="font-mono">{promptTemplateRef}</span></p>
      ) : null}
    </aside>
  );
}

function getConflictDraft(form: EditForm): ConflictModalDraft {
  return {
    label: form.label,
    description: form.description,
    nextTarget: form.nextTarget,
    promptBody: form.promptBody,
  };
}

function getChangedFields(form: EditForm, original: EditForm): ChangedField[] {
  const changed: ChangedField[] = [];
  if (form.label !== original.label) changed.push('label');
  if (form.description !== original.description) changed.push('description');
  if (form.nextTarget !== original.nextTarget) changed.push('nextTarget');
  if (form.promptBody && form.promptBody !== original.promptBody) changed.push('promptBody');
  return changed;
}

function ValidationPanel({ problems, dangling }: { problems: string[]; dangling: string[] }) {
  if (problems.length === 0 && dangling.length === 0) return null;
  return (
    <div
      className="absolute bottom-3 left-3 max-w-[360px] rounded-card border border-amber-700/40 bg-background-100 p-2 text-copy-13 shadow-popover"
      role="status"
    >
      <div className="flex items-center gap-1.5 text-amber-1000">
        <AlertTriangle className="h-4 w-4" aria-hidden />
        <span className="font-semibold">Validation notes</span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5 text-gray-900">
        {problems.map((p, i) => <li key={`p${i}`}>{p}</li>)}
        {dangling.map((d, i) => <li key={`d${i}`} className="text-amber-1000">Dangling transition: {d}</li>)}
      </ul>
    </div>
  );
}

function ArtifactsList({ artifacts }: { artifacts: IdeaArtifact[] }) {
  return (
    <section
      aria-label="Steering artifacts"
      className="rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-card"
    >
      <div className="flex items-center gap-2">
        <ScrollText className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="text-heading-16 font-heading text-gray-1000">Steering artifacts</h3>
      </div>
      {artifacts.length === 0 ? (
        <p className="mt-2 text-copy-13 text-gray-700">Ideas you send appear here so you can trace why Nexus did something.</p>
      ) : (
        <ul className="mt-2 flex flex-col gap-1.5">
          {artifacts.map((a) => (
            <li key={a.id} className="rounded-control border border-gray-alpha-300 px-2 py-1.5 text-copy-13">
              <span className="mr-1.5 rounded-pill bg-purple-700/10 px-1.5 py-0.5 text-label-12 text-purple-1000">{a.verb}</span>
              <span className="text-gray-1000">{a.text}</span>
              {a.target ? <span className="ml-1 font-mono text-gray-700">→ {a.target.slice(0, 8)}</span> : null}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
