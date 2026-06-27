/**
 * Strategy canvas custom node types — rendering for the node kinds produced by
 * the graph adapter (canvas-strategy-surface.md Draft §3.2/§3.4).
 *
 * Node kinds:
 *   • strategy-state    — outer state-machine state
 *   • strategy-group    — inner-graph state (contains child nodes)
 *   • strategy-join     — Converge merge-point state
 *   • strategy-terminal — terminal state
 *   • strategy-inner    — inner-graph child step
 *
 * Status overlay (A3) is driven by `data.status` patched onto node data by the
 * canvas when session state arrives. Status uses existing semantic colors
 * (green/amber/red/teal) per Draft §3.6 — canvas tokens cover shared primitives
 * only. Selection pairs the `canvas-node-border-selected` token with the global
 * focus ring so state is not color-only (Draft §4.4 #6).
 */
import { memo } from 'react';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

/** Map an orchestration session status string to a semantic status bucket. */
export type NodeStatus = 'current' | 'running' | 'waiting' | 'error' | 'completed' | undefined;

function statusFromSession(status: string | undefined): NodeStatus {
  if (!status) return undefined;
  const s = status.toLowerCase();
  if (s.includes('error') || s.includes('fail')) return 'error';
  if (s.includes('pause')) return 'waiting';
  if (s.includes('wait')) return 'waiting';
  if (s.includes('complete') || s.includes('done') || s.includes('finish')) return 'completed';
  if (s.includes('run') || s.includes('active')) return 'running';
  return undefined;
}

/**
 * Resolve a node's effective status for the live overlay.
 *
 * The canvas patches `data.status` onto the current execution node with the
 * sentinel `'__current__'` (or one of the raw session status strings when the
 * overlay poll catches an in-flight update). All node types must route
 * through this helper so the `'__current__'` → `'current'` translation is
 * applied uniformly; otherwise inner-graph / join / terminal / inner-child
 * nodes silently drop the indicator at session start and during poll gaps.
 */
function effectiveStatus(rawStatus: string | undefined): NodeStatus {
  if (rawStatus === '__current__') return 'current';
  return statusFromSession(rawStatus);
}

const STATUS_RING: Record<NonNullable<NodeStatus>, string> = {
  current: 'ring-2 ring-blue-700',
  running: 'ring-2 ring-green-700',
  waiting: 'ring-2 ring-amber-700',
  error: 'ring-2 ring-red-700',
  completed: 'ring-2 ring-teal-700',
};

const STATUS_DOT: Record<NonNullable<NodeStatus>, string> = {
  current: 'bg-blue-700',
  running: 'bg-green-700',
  waiting: 'bg-amber-700',
  error: 'bg-red-700',
  completed: 'bg-teal-700',
};

const STATUS_LABEL: Record<NonNullable<NodeStatus>, string> = {
  current: 'Current',
  running: 'Running',
  waiting: 'Waiting',
  error: 'Error',
  completed: 'Completed',
};

interface NodeShellProps {
  selected: boolean;
  status: NodeStatus;
  accent?: boolean;
  children: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
}

function NodeShell({ selected, status, accent, children, className, style }: NodeShellProps) {
  return (
    <div
      className={[
        'min-w-[176px] rounded-card border bg-canvas-node-fill px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected ? 'border-canvas-node-border-selected' : 'border-canvas-node-border',
        status ? STATUS_RING[status] : '',
        accent ? 'border-l-[3px] border-l-canvas-strategy-accent' : '',
        className ?? '',
      ].join(' ')}
      style={style}
    >
      {children}
    </div>
  );
}

function NodeHeader({ label, status }: { label: string; status: NodeStatus }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      {status ? (
        <span className="flex items-center gap-1 text-label-12 text-gray-700">
          <span className={`inline-block h-2 w-2 rounded-pill ${STATUS_DOT[status]}`} aria-hidden />
          {STATUS_LABEL[status]}
        </span>
      ) : null}
    </div>
  );
}

function KindTag({ kind }: { kind: string }) {
  return (
    <span className="mt-0.5 inline-block rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
      {kind}
    </span>
  );
}

/** Outer state-machine state node. */
export const StrategyStateNode = memo(function StrategyStateNode({
  data,
  selected,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  const isCurrent = status !== undefined;
  return (
    <NodeShell selected={!!selected} status={status} accent>
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <KindTag kind={d.stateKind} />
      {d.description ? <p className="mt-1 text-copy-13 text-gray-900 line-clamp-2">{d.description}</p> : null}
      {d.isInitial ? <span className="mt-1 inline-block text-label-12 text-purple-700">Start</span> : null}
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      {isCurrent ? <span className="sr-only">Current execution node</span> : null}
    </NodeShell>
  );
});

/** Inner-graph group node (contains child steps). */
export const StrategyGroupNode = memo(function StrategyGroupNode({
  data,
  selected,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  return (
    <NodeShell
      selected={!!selected}
      status={status}
      accent
      className="min-w-[260px] min-h-[180px]"
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <p className="mt-0.5 text-copy-13 text-gray-700">Inner DAG · {d.innerGraphId}</p>
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeShell>
  );
});

/** Converge merge-point join node. */
export const StrategyJoinNode = memo(function StrategyJoinNode({
  data,
  selected,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  return (
    <NodeShell selected={!!selected} status={status} className="min-w-[176px]">
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <span className="mt-0.5 inline-block rounded-pill bg-[color-mix(in_srgb,var(--color-purple-700)_12%,transparent)] px-1.5 py-0.5 text-label-12 text-purple-1000">
        Join · {d.convergeStrategy ?? 'wait_for_all'}
      </span>
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeShell>
  );
});

/** Terminal state node. */
export const StrategyTerminalNode = memo(function StrategyTerminalNode({
  data,
  selected,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  return (
    <NodeShell selected={!!selected} status={status} className="min-w-[140px]">
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <span className="mt-0.5 inline-block text-label-12 text-gray-700">End</span>
    </NodeShell>
  );
});

/** Inner-graph child step node. */
export const StrategyInnerNode = memo(function StrategyInnerNode({
  data,
  selected,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  return (
    <NodeShell selected={!!selected} status={status} className="min-w-[150px]">
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeShell>
  );
});

export const strategyNodeTypes = {
  'strategy-state': StrategyStateNode,
  'strategy-group': StrategyGroupNode,
  'strategy-join': StrategyJoinNode,
  'strategy-terminal': StrategyTerminalNode,
  'strategy-inner': StrategyInnerNode,
} as const;
