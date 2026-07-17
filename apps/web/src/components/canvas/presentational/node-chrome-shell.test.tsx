/**
 * NodeChromeShell — boundary + rendering tests.
 *
 * Boundary test (architect invariant, R-V1108P1QC1-S002): the extract module
 * MUST NOT import `@xyflow/react`. App RF node wrappers adapt `NodeProps` →
 * `NodeChromeShell` props; Design Studio fixtures consume it directly. This
 * keeps the presentational chrome free of React Flow so RF upgrades cannot
 * pull the chrome (or the Studio gallery) into the RF dependency graph.
 *
 * Rendering tests pin the structural chrome contract shared by every canvas
 * node kind: selection border, status ring, accent stripe, and the fill/border
 * token classes consumed by both the App graph and the Design Studio fixtures.
 */
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import { NodeChromeShell, NODE_STATUS_DOT, NODE_STATUS_RING, type NodeStatus } from './node-chrome-shell';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const EXTRACT_SOURCE_PATH = path.resolve(__dirname, 'node-chrome-shell.tsx');

// ---------------------------------------------------------------------------
// Boundary — no @xyflow/react import in the extract module
// ---------------------------------------------------------------------------

describe('NodeChromeShell presentational boundary', () => {
  it('does not import @xyflow/react (no RF types leak into the chrome extract)', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    // Assert no import/require of @xyflow/react. The docstring legitimately
    // *mentions* `@xyflow/react` to describe the boundary — that is fine; only
    // an actual import would violate the invariant.
    expect(source).not.toMatch(/(?:import|require)\s*[^'"]*['"]@xyflow\/react['"]/);
  });

  it('does not import RF-specific symbols (NodeProps / Handle / Position)', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    // No import line should carry @xyflow/react (covers NodeProps/Handle/Position
    // which all come from that module).
    const importLines = source.split('\n').filter((l) => /^\s*import\b/.test(l));
    for (const line of importLines) {
      expect(line).not.toContain('@xyflow/react');
    }
  });
});

// ---------------------------------------------------------------------------
// Rendering — structural chrome contract
// ---------------------------------------------------------------------------

describe('NodeChromeShell rendering', () => {
  it('renders the base canvas-node chrome classes', () => {
    const { container } = render(<NodeChromeShell>body</NodeChromeShell>);
    const shell = container.firstElementChild as HTMLElement;
    expect(shell.className).toContain('rounded-card');
    expect(shell.className).toContain('bg-canvas-node-fill');
    expect(shell.className).toContain('shadow-card');
    // V1.121 P2 T4: default node width now consumes the registered P0
    // `min-w-canvas-node-default` utility (DESIGN.md
    // components.canvas.node-width.default) instead of the raw `min-w-[176px]`
    // arbitrary value — same rendered 176px, but token-named so P3 can lift
    // canvas node geometry in one place.
    expect(shell.className).toContain('min-w-canvas-node-default');
  });

  it('applies the default (unselected) border class', () => {
    const { container } = render(<NodeChromeShell>body</NodeChromeShell>);
    expect(container.firstElementChild!.className).toContain('border-canvas-node-border');
    expect(container.firstElementChild!.className).not.toContain('border-canvas-node-border-selected');
  });

  it('applies the selected border class when selected', () => {
    const { container } = render(<NodeChromeShell selected>body</NodeChromeShell>);
    expect(container.firstElementChild!.className).toContain('border-canvas-node-border-selected');
  });

  it('applies the status ring when status is set', () => {
    const { container } = render(
      <NodeChromeShell status="current">body</NodeChromeShell>,
    );
    expect(container.firstElementChild!.className).toContain(NODE_STATUS_RING.current);
  });

  it('omits the status ring when status is absent', () => {
    const { container } = render(<NodeChromeShell>body</NodeChromeShell>);
    expect(container.firstElementChild!.className).not.toContain('ring-blue-700');
  });

  it('applies the strategy accent stripe when accent is set', () => {
    const { container } = render(<NodeChromeShell accent>body</NodeChromeShell>);
    expect(container.firstElementChild!.className).toContain('border-l-canvas-strategy-accent');
  });

  it('merges className via cn so callers can override min-width', () => {
    const { container } = render(
      <NodeChromeShell className="min-w-canvas-node-outline-scene-beat">body</NodeChromeShell>,
    );
    const cls = container.firstElementChild!.className;
    // tailwind-merge deduplicates within the registered `min-w` class group
    // (packages/nexus-ui/src/lib/cn.ts): the caller's scene-beat utility wins,
    // the default `min-w-canvas-node-default` is dropped.
    expect(cls).toContain('min-w-canvas-node-outline-scene-beat');
    expect(cls).not.toContain('min-w-canvas-node-default');
  });

  it('passes inline style through (scene/beat fill + border tokens)', () => {
    const { container } = render(
      <NodeChromeShell
        style={{
          background: 'var(--color-canvas-outline-scene-fill)',
          borderColor: 'var(--color-canvas-outline-scene-border)',
        }}
      >
        body
      </NodeChromeShell>,
    );
    const shell = container.firstElementChild as HTMLElement;
    expect(shell.style.background).toContain('var(--color-canvas-outline-scene-fill)');
    expect(shell.style.borderColor).toContain('var(--color-canvas-outline-scene-border)');
  });

  it('renders children', () => {
    const { getByText } = render(
      <NodeChromeShell><span>node content</span></NodeChromeShell>,
    );
    expect(getByText('node content')).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Status maps — shared chrome contract (App header + Studio fixtures consume)
// ---------------------------------------------------------------------------

describe('NODE_STATUS ring + dot maps', () => {
  const STATUSES: NodeStatus[] = ['current', 'running', 'waiting', 'error', 'completed'];

  it('every NodeStatus has a ring class', () => {
    for (const s of STATUSES) {
      expect(NODE_STATUS_RING[s]).toMatch(/^ring-2 ring-/);
    }
  });

  it('every NodeStatus has a dot fill class', () => {
    for (const s of STATUSES) {
      expect(NODE_STATUS_DOT[s]).toMatch(/^bg-/);
    }
  });
});
