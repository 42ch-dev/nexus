/**
 * CanvasShell Timeline prominence — V1.123 P3 Task 2.
 *
 * Pins the visual prominence treatment locked by:
 *   - `iterations/v1.123/specs/three-layer-product-spec.md` (Timeline is the
 *     central instrument; Canvas shell visually distinguishes it).
 *   - `iterations/v1.123/specs/three-layer-architecture.md` §5 + §8
 *     (Timeline accent token; ordering distinction vs other surfaces).
 *   - Plan `2026-07-18-v1.123-timeline-first-ia-deepening.md` Task 2.
 *
 * Coverage:
 *   - `CanvasShellTimelineBadge` (presentational) renders with the Timeline
 *     accent color + ARIA label, exposing `data-testid` + a small accent dot
 *     for sighted users.
 *   - `canvas-nav-commands.tsx` registers `go.timeline` (always available —
 *     one-click reachability from the command palette).
 *
 * The badge is exported as a standalone presentational component so the
 * prominence contract can be tested without mounting the full React Flow
 * chrome. The orchestrator-level wiring (Timeline canvas passes
 * `surfaceKind="timeline"` to `CanvasShell`, which renders the badge as an
 * overlay) is verified at the per-surface canvas level.
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ReactElement } from 'react';
import { MemoryRouter } from 'react-router';

import { CanvasShellTimelineBadge } from '../canvas-shell';
import { CanvasNavCommands } from '../canvas-nav-commands';
import {
  clearCommands,
  getCommands,
} from '@/lib/canvas/command-registry';

// ─── Badge presentational (no React Flow mount) ────────────────────────────

describe('CanvasShellTimelineBadge — V1.123 P3 Task 2 visual prominence', () => {
  it('renders the Timeline label with the accent color', () => {
    render(
      <CanvasShellTimelineBadge
        label="Timeline"
        ariaLabel="Timeline surface — central instrument"
      />,
    );

    const badge = screen.getByTestId('canvas-shell-timeline-badge');
    expect(badge).toBeInTheDocument();
    expect(badge).toHaveTextContent('Timeline');
    expect(badge).toHaveAttribute('aria-label', 'Timeline surface — central instrument');
    // The accent color resolves to the Timeline accent CSS variable so the
    // badge reads as part of the Timeline surface family (mirrors the
    // per-surface accent pattern: strategy=purple, outline=amber,
    // worldkb=teal, timeline=blue).
    expect(badge.style.color).toBe('var(--color-canvas-timeline-accent)');
  });

  it('renders a small accent dot for sighted users', () => {
    render(
      <CanvasShellTimelineBadge label="Timeline" ariaLabel="Timeline surface" />,
    );

    const badge = screen.getByTestId('canvas-shell-timeline-badge');
    // The dot is the visual prominence cue; it carries the accent background
    // and aria-hidden so screen readers don't double-announce (the badge
    // label + aria-label are the SR surface).
    const dot = badge.querySelector('[aria-hidden="true"]');
    expect(dot).not.toBeNull();
    expect((dot as HTMLElement).style.backgroundColor).toBe(
      'var(--color-canvas-timeline-accent)',
    );
  });

  it('uses a status role so assistive tech announces the surface context', () => {
    render(
      <CanvasShellTimelineBadge label="Timeline" ariaLabel="Timeline surface" />,
    );
    const badge = screen.getByTestId('canvas-shell-timeline-badge');
    // role="status" + aria-label lets SR users perceive the Timeline surface
    // context without sighted-only cues (the accent dot is aria-hidden).
    expect(badge).toHaveAttribute('role', 'status');
  });
});

// ─── Command palette registration (always-available global entry) ──────────

function renderInRouter(ui: ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

describe('CanvasNavCommands — go.timeline registration (V1.123 P3 Task 2)', () => {
  beforeEach(() => {
    clearCommands();
  });
  afterEach(() => {
    clearCommands();
  });

  it('registers go.timeline targeting /timeline', () => {
    renderInRouter(<CanvasNavCommands />);

    const cmd = getCommands().find((c) => c.id === 'go.timeline');
    expect(cmd).toBeDefined();
    expect(cmd?.labelKey).toBe('go.timeline.label');
    expect(cmd?.groupKey).toBe('group.navigate');
  });

  it('keeps go.timeline always available (no workId / worldId gating)', () => {
    // Unlike `go.outline` (workId-gated) and `go.world-kb` (worldId-gated),
    // `go.timeline` targets `/timeline` — a primary-nav route with no path
    // params. The command MUST be reachable from any route (one-click
    // reachability per AC-V1123-16).
    renderInRouter(<CanvasNavCommands />);

    const cmd = getCommands().find((c) => c.id === 'go.timeline');
    expect(cmd).toBeDefined();
    // `available` omitted → always available. If present, must return true.
    expect(cmd?.available?.() ?? true).toBe(true);
  });

  it('preserves existing canvas nav commands (additive only)', () => {
    // V1.111 P0 T4 + V1.123 P2 T5 registrations MUST remain alongside the
    // new global Timeline entry (V1.122 / V1.123 P2 regression).
    renderInRouter(<CanvasNavCommands />);

    const ids = new Set(getCommands().map((c) => c.id));
    expect(ids.has('go.strategy')).toBe(true);
    expect(ids.has('go.outline')).toBe(true);
    expect(ids.has('go.world-kb')).toBe(true);
    expect(ids.has('go.work-timeline')).toBe(true);
    expect(ids.has('go.timeline')).toBe(true);
  });
});
