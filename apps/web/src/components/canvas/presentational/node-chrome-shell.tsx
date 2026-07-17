/**
 * NodeChromeShell — shared presentational wrapper for canvas node chrome.
 *
 * This is the single source for the structural shell common to every React
 * Flow node kind (strategy state/group/join/terminal/inner, outline
 * volume/chapter/timeline, outline scene/beat). It renders the outer card
 * (`rounded-card` + `border` + `bg-canvas-node-fill` + `shadow-card` +
 * transition), the selection border toggle, an optional status overlay ring
 * (strategy surface), and an optional strategy accent stripe.
 *
 * Presentational boundary (architect invariant, R-V1108P1QC1-S002):
 *   This module MUST NOT import `@xyflow/react` — no `NodeProps`, `Handle`,
 *   or `Position`. App RF node wrappers adapt `NodeProps` → these props (the
 *   RF wrapper stays in `apps/web/src/components/canvas/*-nodes.tsx`); Design
 *   Studio fixtures consume `NodeChromeShell` directly with static props via
 *   the `@web-canvas/*` alias. Enforced by the boundary test alongside this
 *   module.
 *
 * Tokens: `canvas-node-fill`, `canvas-node-border(-selected)` (DESIGN.md).
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

export interface NodeChromeShellProps {
  /** Selection state — toggles `canvas-node-border-selected`. */
  selected?: boolean;
  /** Optional status overlay ring (strategy surface). */
  status?: NodeStatus;
  /** Optional strategy accent stripe (`border-l-canvas-strategy-accent`). */
  accent?: boolean;
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
 * card with the `canvas-node-*` token chrome, the selection border, an
 * optional status ring, and an optional accent stripe. Pure presentational —
 * props in, JSX out, no `@xyflow/react` dependency.
 */
export function NodeChromeShell({
  selected = false,
  status,
  accent = false,
  className,
  style,
  children,
}: NodeChromeShellProps) {
  return (
    <div
      className={cn(
        'min-w-canvas-node-default rounded-card border bg-canvas-node-fill px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected
          ? 'border-canvas-node-border-selected'
          : 'border-canvas-node-border',
        status ? NODE_STATUS_RING[status] : '',
        accent ? 'border-l-[3px] border-l-canvas-strategy-accent' : '',
        className,
      )}
      style={style}
    >
      {children}
    </div>
  );
}
