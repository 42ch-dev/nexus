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
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import {
  NodeChromeShell,
  NODE_STATUS_DOT,
  type NodeStatus,
} from './presentational/node-chrome-shell';

// Re-export the shared status type + chrome extract so historical import
// sites (`NodeStatus` from this barrel) keep resolving. The canonical home is
// now the presentational extract.
export type { NodeStatus };

function statusFromSession(status: string | undefined): NodeStatus | undefined {
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
function effectiveStatus(rawStatus: string | undefined): NodeStatus | undefined {
  if (rawStatus === '__current__') return 'current';
  return statusFromSession(rawStatus);
}

/**
 * i18n keys for the strategy status overlay label. The status ring + dot
 * classes live in the shared `NodeChromeShell` extract
 * (`NODE_STATUS_RING` / `NODE_STATUS_DOT`); only the human-readable label is
 * App-owned (resolved via `t()` at render time).
 */
const STATUS_LABEL_KEYS: Record<NodeStatus, string> = {
  current: 'strategy.node.status.current',
  running: 'strategy.node.status.running',
  waiting: 'strategy.node.status.waiting',
  error: 'strategy.node.status.error',
  completed: 'strategy.node.status.completed',
};

function NodeHeader({ label, status }: { label: string; status: NodeStatus | undefined }) {
  const { t } = useTranslation('canvas');
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      {status ? (
        <span className="flex items-center gap-1 text-label-12 text-gray-700">
          <span className={`inline-block h-2 w-2 rounded-pill ${NODE_STATUS_DOT[status]}`} aria-hidden />
          {t(STATUS_LABEL_KEYS[status])}
        </span>
      ) : null}
    </div>
  );
}

function KindTag({ kind }: { kind: string }) {
  const { t } = useTranslation('canvas');
  return (
    <span className="mt-0.5 inline-block rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
      {t(`strategy.node.kind.${kind}`, { defaultValue: kind })}
    </span>
  );
}

/** Outer state-machine state node. */
export const StrategyStateNode = memo(function StrategyStateNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const { t } = useTranslation('canvas');
  const status = effectiveStatus(d.status);
  const isCurrent = status !== undefined;
  return (
    <NodeChromeShell
      selected={!!selected}
      status={status}
      accent="strategy"
      dragging={dragging}
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <KindTag kind={d.stateKind} />
      {d.description ? <p className="mt-1 text-copy-13 text-gray-900 line-clamp-2">{d.description}</p> : null}
      {d.isInitial ? <span className="mt-1 inline-block text-label-12 text-purple-700">{t('strategy.node.start')}</span> : null}
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      {isCurrent ? <span className="sr-only">{t('strategy.node.currentExecutionNode')}</span> : null}
    </NodeChromeShell>
  );
});

/** Inner-graph group node (contains child steps). */
export const StrategyGroupNode = memo(function StrategyGroupNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const { t } = useTranslation('canvas');
  const status = effectiveStatus(d.status);
  return (
    <NodeChromeShell
      selected={!!selected}
      status={status}
      accent="strategy"
      dragging={dragging}
      className="min-w-canvas-node-strategy-root min-h-[180px]"
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <p className="mt-0.5 text-copy-13 text-gray-700">
        {t('strategy.node.innerGraph', { id: d.innerGraphId })}
      </p>
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

/** Converge merge-point join node. */
export const StrategyJoinNode = memo(function StrategyJoinNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const { t } = useTranslation('canvas');
  const status = effectiveStatus(d.status);
  return (
    <NodeChromeShell
      selected={!!selected}
      status={status}
      accent="strategy"
      dragging={dragging}
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <span className="mt-0.5 inline-block rounded-pill bg-[color-mix(in_srgb,var(--color-purple-700)_12%,transparent)] px-1.5 py-0.5 text-label-12 text-purple-1000">
        {t('strategy.node.join', { strategy: d.convergeStrategy ?? 'wait_for_all' })}
      </span>
      <Handle type="source" position={Position.Bottom} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

/** Terminal state node. */
export const StrategyTerminalNode = memo(function StrategyTerminalNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const { t } = useTranslation('canvas');
  const status = effectiveStatus(d.status);
  return (
    <NodeChromeShell
      selected={!!selected}
      status={status}
      accent="strategy"
      dragging={dragging}
      className="min-w-canvas-node-strategy-primary"
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <span className="mt-0.5 inline-block text-label-12 text-gray-700">{t('strategy.node.end')}</span>
    </NodeChromeShell>
  );
});

/** Inner-graph child step node. */
export const StrategyInnerNode = memo(function StrategyInnerNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as StrategyNodeData;
  const status = effectiveStatus(d.status);
  return (
    <NodeChromeShell
      selected={!!selected}
      status={status}
      accent="strategy"
      dragging={dragging}
      className="min-w-canvas-node-strategy-secondary"
    >
      <Handle type="target" position={Position.Left} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
      <NodeHeader label={d.label} status={status} />
      <Handle type="source" position={Position.Right} className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port" />
    </NodeChromeShell>
  );
});

export const strategyNodeTypes = {
  'strategy-state': StrategyStateNode,
  'strategy-group': StrategyGroupNode,
  'strategy-join': StrategyJoinNode,
  'strategy-terminal': StrategyTerminalNode,
  'strategy-inner': StrategyInnerNode,
} as const;
