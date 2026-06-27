/**
 * Strategy canvas — the α-scope Strategy (Preset) surface
 * (canvas-strategy-surface.md Draft §3.2/§3.3/§3.7).
 *
 * Composes: shared Canvas Shell (A1) + Strategy graph adapter (A2) + bounded
 * live overlay (A3) + Idea-input steering (A4) + read-only side inspector +
 * validation panel + accessibility alternate view (A8).
 *
 * Read + overlay + steer only — no structured node edits (V1.71). UI label is
 * "Strategy"; persisted identifiers remain "preset" (Draft §4.2).
 * `wire_contracts_changed: FALSE`.
 */
import { useEffect, useMemo, useState } from 'react';
import { useEdgesState, useNodesState, type Edge, type Node } from '@xyflow/react';
// `useCallback` is not needed: React Flow tracks selection via node.selected
// patches delivered through onNodesChange.
import { AlertTriangle, Info, ScrollText } from 'lucide-react';

import { CanvasShell } from '@/components/canvas/canvas-shell';
import { IdeaInput, type IdeaArtifact } from '@/components/canvas/idea-input';
import { strategyNodeTypes } from '@/components/canvas/strategy-nodes';
import { StrategyAltView } from '@/components/canvas/strategy-alt-view';
import {
  useActiveSession,
  useDerivedCreatorId,
  usePresetGraph,
  usePresetSchedules,
} from '@/lib/canvas/use-strategy-data';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { ErrorState, LoadingState } from '@/components/ui/states';

export interface StrategyCanvasProps {
  presetId: string;
}

export function StrategyCanvas({ presetId }: StrategyCanvasProps) {
  const graphQuery = usePresetGraph(presetId);
  const activeSession = useActiveSession(presetId);
  const schedules = usePresetSchedules(presetId);
  const creatorId = useDerivedCreatorId(presetId);

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);
  const [artifacts, setArtifacts] = useState<IdeaArtifact[]>([]);
  const [showAlt, setShowAlt] = useState(false);

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

  const selected = useMemo(
    () => nodes.find((n) => n.selected) ?? null,
    [nodes],
  );

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

  if (graphQuery.isLoading) return <LoadingState label="Loading Strategy…" />;
  if (graphQuery.isError) return <ErrorState description="Could not load the Strategy preset." onRetry={() => graphQuery.refetch()} />;

  const parsed = graphQuery.data?.parsed;
  const problems = parsed?.problems ?? [];
  const dangling = graphQuery.data?.graph.danglingTargets ?? [];
  // Pick the schedule most likely to back the active session. The wire schema
// does not expose a direct session ↔ schedule link (SessionSummary and
// ScheduleSummary both lack the cross-reference id), so the best heuristic
// is "most recently updated schedule for this preset, if any active session
// exists." Without an active session, return undefined so Steer/Resume
// disable cleanly (live banner hidden, helper text explains why).
const activeScheduleId = activeSession
  ? [...(schedules.data ?? [])].sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0]
      ?.schedule_id
  : undefined;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-heading-20 font-heading text-gray-1000">Strategy</h2>
          <p className="text-copy-13 text-gray-700">
            Preset <span className="font-mono">{presetId}</span> as a state-machine graph. Steer execution with an Idea — Nexus owns the prose.
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
          {/* Side inspector (read-only at α) */}
          <InspectorOverlay selected={selected} />
          {/* Validation panel (read-only) */}
          <ValidationPanel problems={problems} dangling={dangling} />
        </CanvasShell>
      )}

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

function InspectorOverlay({ selected }: { selected: Node | null }) {
  if (!selected) return null;
  const d = selected.data as StrategyNodeData;
  return (
    <aside
      className="absolute right-3 top-3 w-[260px] rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover"
      aria-label="Selected node details"
    >
      <div className="flex items-center gap-2">
        <Info className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="text-heading-16 font-heading text-gray-1000">{d.label}</h3>
      </div>
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between"><dt className="text-gray-700">Kind</dt><dd className="font-mono text-gray-1000">{d.stateKind}</dd></div>
        <div className="flex justify-between"><dt className="text-gray-700">State id</dt><dd className="font-mono text-gray-1000">{d.stateId}</dd></div>
        {d.innerGraphId ? <div className="flex justify-between"><dt className="text-gray-700">Inner graph</dt><dd className="font-mono text-gray-1000">{d.innerGraphId}</dd></div> : null}
        {d.convergeStrategy ? <div className="flex justify-between"><dt className="text-gray-700">Converge</dt><dd className="font-mono text-gray-1000">{d.convergeStrategy}</dd></div> : null}
        {d.isInitial ? <div className="text-purple-700">Initial state</div> : null}
        {d.isTerminal ? <div className="text-gray-700">Terminal state</div> : null}
        {d.status ? <div className="flex justify-between"><dt className="text-gray-700">Status</dt><dd className="text-blue-700">{d.status}</dd></div> : null}
      </dl>
      {d.description ? <p className="mt-2 text-copy-13 text-gray-900">{d.description}</p> : null}
      <p className="mt-2 text-copy-13 text-gray-700">Read-only at α. Node-granular edits arrive in V1.71.</p>
    </aside>
  );
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
