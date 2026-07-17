/**
 * NodeChromeShell — shared presentational wrapper for canvas node chrome.
 *
 * This is the single source for the structural shell common to every React
 * Flow node kind (strategy state/group/join/terminal/inner, outline
 * volume/chapter/timeline, outline scene/beat). It renders the outer card
 * (`rounded-card` + `border` + `bg-canvas-node-fill` + v0.4 elevation recipe
 * + transition), the selection border + two-layer selection ring, an
 * optional status overlay ring (strategy surface), and an optional
 * per-surface accent spine.
 *
 * Presentational boundary (architect invariant, R-V1108P1QC1-S002):
 *   This module MUST NOT import `@xyflow/react` — no `NodeProps`, `Handle`,
 *   or `Position`. App RF node wrappers adapt `NodeProps` → these props (the
 *   RF wrapper stays in `apps/web/src/components/canvas/*-nodes.tsx`); Design
 *   Studio fixtures consume `NodeChromeShell` directly with static props via
 *   the `@web-canvas/*` alias. Enforced by the boundary test alongside this
 *   module.
 *
 * V1.121 v0.4 (P3 T2):
 *   - Elevation recipe — rest `shadow-card` (alias for `--shadow-elevation-1`)
 *     → hover `shadow-elevation-2` → dragging `shadow-elevation-4`. The
 *     dragging tier is selected via the `data-dragging` attribute so the RF
 *     wrapper just forwards the `dragging` prop without a class-name branch.
 *   - Two-layer selection ring — when `selected` AND no status overlay,
 *     a `ring-2 ring-canvas-node-border-selected ring-offset-2
 *     ring-offset-background-100` halo pairs with the existing selected
 *     border so selection is never color-only (Draft §4.4 #6). When a status
 *     overlay is active, the status ring wins the ring slot (more specific).
 *   - Per-surface accent spine — `accent` accepts a surface name
 *     (`'strategy' | 'outline' | 'worldkb'`) selecting the surface's spine
 *     token. `accent={true}` remains a synonym for `'strategy'` so existing
 *     call sites keep rendering the strategy spine unchanged.
 *
 * Tokens: `canvas-node-fill`, `canvas-node-border(-selected)`,
 * `canvas-{strategy,outline,worldkb}-accent` (DESIGN.md).
 * The status ring + dot use semantic colors (Draft §3.6 — canvas tokens cover
 * shared primitives only). Selection pairs `canvas-node-border-selected` with
 * the global focus ring so state is never color-only (Draft §4.4 #6).
 */
import type { CSSProperties, ReactNode } from 'react';

import { cn } from '@/lib/utils';

/**
 * Semantic status buckets driving the strategy node overlay ring + dot.
 * Kept free of `| undefined` so it can index the ring/dot maps directly; the
 * shell's `status` prop is optional (`status?: NodeStatus`) for the absent
 * case.
 */
export type NodeStatus = 'current' | 'running' | 'waiting' | 'error' | 'completed';

/** Status → selection ring class (semantic colors per Draft §3.6). */
export const NODE_STATUS_RING: Record<NodeStatus, string> = {
  current: 'ring-2 ring-blue-700',
  running: 'ring-2 ring-green-700',
  waiting: 'ring-2 ring-amber-700',
  error: 'ring-2 ring-red-700',
  completed: 'ring-2 ring-teal-700',
};

/** Status → status-dot fill class (pairs with the ring; Draft §3.6). */
export const NODE_STATUS_DOT: Record<NodeStatus, string> = {
  current: 'bg-blue-700',
  running: 'bg-green-700',
  waiting: 'bg-amber-700',
  error: 'bg-red-700',
  completed: 'bg-teal-700',
};

/**
 * Per-surface accent spine kind. `true` is accepted as a backward-compatible
 * synonym for `'strategy'` (existing call sites); the surface names let new
 * callers pick the outline / worldkb spine without touching the shell.
 */
export type NodeAccent = boolean | 'strategy' | 'outline' | 'worldkb';

/**
 * Accent → Tailwind spine classes. Centralized so the surface → token map is
 * one place to read; P3 T2 introduced the outline + worldkb entries to match
 * DESIGN.md §Canvas Surface ("strategy = purple-700, outline = amber-700,
 * World KB = teal-700").
 */
const ACCENT_SPINE_CLASSES: Record<Exclude<NodeAccent, boolean>, string> = {
  strategy: 'border-l-[3px] border-l-canvas-strategy-accent',
  outline: 'border-l-[3px] border-l-canvas-outline-accent',
  worldkb: 'border-l-[3px] border-l-canvas-worldkb-accent',
};

export interface NodeChromeShellProps {
  /** Selection state — toggles `canvas-node-border-selected` + two-layer ring. */
  selected?: boolean;
  /** Optional status overlay ring (strategy surface). */
  status?: NodeStatus;
  /**
   * Optional per-surface accent spine. `true` is a backward-compatible
   * synonym for `'strategy'` (existing strategy call sites render the
   * strategy spine unchanged).
   */
  accent?: NodeAccent;
  /**
   * Dragging state — when true, switches the elevation recipe from
   * `shadow-card` (rest = elevation-1) to `shadow-elevation-4` so the node
   * visibly lifts while the author is repositioning it (DESIGN.md §Elevation
   * v0.4 recipe). RF node wrappers forward `NodeProps.dragging`.
   */
  dragging?: boolean;
  /**
   * Extra classes merged via `cn()` (tailwind-merge). Use to override the
   * default `min-w-canvas-node-default` (e.g. scene/beat pass
   * `min-w-canvas-node-outline-scene-beat`) or add node-specific sizing.
   */
  className?: string;
  /**
   * Inline style overrides. Scene/beat nodes pass `background` (fill token)
   * and `borderColor` (border token, undefined when selected so the selected
   * class wins); outline volume/timeline pass their pin/fill tokens here.
   * Inline style wins over the base `bg-canvas-node-fill` / border classes.
   */
  style?: CSSProperties;
  children?: ReactNode;
}

/**
 * Shared presentational wrapper for canvas node chrome. Renders the outer
 * card with the `canvas-node-*` token chrome, the v0.4 elevation recipe
 * (rest → hover → dragging), the selection border + two-layer ring, an
 * optional status ring, and an optional per-surface accent spine. Pure
 * presentational — props in, JSX out, no `@xyflow/react` dependency.
 */
export function NodeChromeShell({
  selected = false,
  status,
  accent = false,
  dragging = false,
  className,
  style,
  children,
}: NodeChromeShellProps) {
  // `accent={true}` (legacy boolean) → 'strategy' so existing call sites
  // keep rendering the strategy spine verbatim.
  const accentKind: Exclude<NodeAccent, boolean> | undefined =
    accent === true ? 'strategy' : accent === false ? undefined : accent;

  return (
    <div
      data-dragging={dragging ? 'true' : undefined}
      className={cn(
        // Base structural chrome. `shadow-card` is the v0.4 elevation-1 alias
        // (DESIGN.md §Elevation). Hover lifts to elevation-2; the
        // `data-[dragging=true]` variant lifts further to elevation-4 when
        // the RF wrapper forwards the dragging prop.
        'min-w-canvas-node-default rounded-card border bg-canvas-node-fill px-3 py-2 shadow-card transition-shadow duration-state ease-standard motion-reduce:transition-none hover:shadow-elevation-2 data-[dragging=true]:shadow-elevation-4',
        selected
          ? 'border-canvas-node-border-selected'
          : 'border-canvas-node-border',
        // Two-layer selection ring — only when no status overlay is active
        // (status ring wins the ring slot when present). Pairs a 2px
        // selected-color ring with a 2px background-color offset so the
        // selection reads as a halo, not just a border-color flip.
        selected && !status
          ? 'ring-2 ring-canvas-node-border-selected ring-offset-2 ring-offset-background-100'
          : '',
        status ? NODE_STATUS_RING[status] : '',
        accentKind ? ACCENT_SPINE_CLASSES[accentKind] : '',
        className,
      )}
      style={style}
    >
      {children}
    </div>
  );
}
