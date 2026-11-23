/**
 *
 * Consolidates the per-layer dagre layout contract for all three Timeline
 * family layers (Brief + Narrative on World Timeline; Narrative + Moment on
 * Work Timeline) so a single test file owns the "three-layer feel" layout
 * dimension (layer-feel-differentiation.md §2 + §8 product bar: "three
 * feels perceptibly different").
 *
 * Layer contract under test (layer-feel-differentiation.md §2.1 + §2.2 +
 * §2.3 + §2.4 + Plan `2026-07-18-v1.123-three-layer-zoom-experience.md`
 * Task 1):
 *
 *   | Layer     | Direction | rankSep      | nodeSep      |
 *   |-----------|-----------|--------------|--------------|
 *   | Brief     | LR        | wide (240)   | tight (40)   |
 *   | Narrative | LR        | V1.122 (80)* | V1.122 (80)* |
 *   | Moment    | TB        | tight (60)   | tight (30)   |
 *
 *   * Narrative inherits `useAutoLayout`'s internal defaults (80/80) by
 *     leaving `rankSep` / `nodeSep` undefined — V1.122 regression preserved.
 *
 * Why this file exists separately from `brief-feel-differentiation.test.tsx`
 * and `moment-feel-differentiation.test.tsx`:
 *   - P1 + P2 shipped per-surface feel tests (Brief on World; Moment on
 *     Work). P4 Task 1's contribution is the **consolidated three-layer
 *     contract** so a screenshot pack + cross-surface visual review can
 *     verify the three feels differentiate without spanning three test
 *     files. The per-surface tests stay green; this file adds the
 *     cross-surface assertion.
 *
 * The implementation already exists in the P1 + P2 adapters; Task 1's
 * contribution here is the **consolidated acceptance test** that P4 QC
 * references when signing AC-V1123-20's "three feels perceptibly different"
 * bar.
 */
import { describe, expect, it } from 'vitest';

import {
  createTimelineCanvasAdapter,
  type TimelineCanvasAdapterContext,
} from '../timeline-canvas/timeline-canvas-adapter';
import {
  createWorkTimelineCanvasAdapter,
  type WorkTimelineCanvasAdapterContext,
} from '../work-timeline-canvas/work-timeline-canvas-adapter';
import type { CanvasSurfaceLayoutOptions } from '../canvas-surface-adapter';

const worldCtxRef: { current: TimelineCanvasAdapterContext } = {
  current: { worldId: 'world-1' },
};

const workCtxRef: { current: WorkTimelineCanvasAdapterContext } = {
  current: { workId: 'work-1' },
};

describe('Per-layer layout differentiation (V1.123 P4 Task 1)', () => {
  it('Brief layer (World Timeline) emits a horizontal era-sweep layout: LR + wide rankSep + tight nodeSep', () => {
    // The exact values (240/40) are tuning knobs; the contract is:
    //   - direction === 'LR' (horizontal reading)
    //   - rankSep > V1.122 Narrative default (wide inter-rank spacing so
    //     the era sweep reads as sparse landmarks)
    //   - nodeSep < V1.122 Narrative default (tight intra-rank spacing so
    //     the temporal-unknown era cluster stays compact)
    //   - hasSuppliedPositions === true (the supplied era positions win on
    //     first open; these values only kick in on explicit relayout)
    const adapter = createTimelineCanvasAdapter(worldCtxRef, 'brief');
    const opts: CanvasSurfaceLayoutOptions | undefined = adapter.layoutOptions;

    expect(opts).toBeDefined();
    expect(opts?.direction).toBe('LR');
    expect(opts?.rankSep).toBe(240);
    expect(opts?.nodeSep).toBe(40);
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Narrative layer (World Timeline) preserves V1.122 baseline (LR + undefined rankSep/nodeSep)', () => {
    const adapter = createTimelineCanvasAdapter(worldCtxRef, 'narrative');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('LR');
    expect(opts?.rankSep).toBeUndefined();
    expect(opts?.nodeSep).toBeUndefined();
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Narrative layer (Work Timeline) inherits the same V1.122 LR baseline', () => {
    const adapter = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('LR');
    expect(opts?.rankSep).toBeUndefined();
    expect(opts?.nodeSep).toBeUndefined();
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Moment layer (Work Timeline) emits a vertical scene-stack: TB + tight rankSep + tight nodeSep', () => {
    const adapter = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('TB');
    expect(opts?.rankSep).toBe(60);
    expect(opts?.nodeSep).toBe(30);
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Brief vs Narrative (World Timeline) differentiate on direction + density knobs', () => {
    const brief = createTimelineCanvasAdapter(worldCtxRef, 'brief');
    const narrative = createTimelineCanvasAdapter(worldCtxRef, 'narrative');

    expect(brief.layoutOptions?.direction).toBe('LR');
    expect(narrative.layoutOptions?.direction).toBe('LR');

    const briefRank = brief.layoutOptions?.rankSep ?? 0;
    const narrativeRank = narrative.layoutOptions?.rankSep ?? 80;
    expect(briefRank).toBeGreaterThan(narrativeRank);

    const briefNode = brief.layoutOptions?.nodeSep ?? 0;
    const narrativeNode = narrative.layoutOptions?.nodeSep ?? 80;
    expect(briefNode).toBeLessThan(narrativeNode);
  });

  it('Narrative vs Moment (Work Timeline) differentiate on direction (LR vs TB) + density', () => {
    const narrative = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const moment = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');

    expect(narrative.layoutOptions?.direction).toBe('LR');
    expect(moment.layoutOptions?.direction).toBe('TB');

    expect(moment.layoutOptions?.rankSep).toBeDefined();
    expect(moment.layoutOptions?.nodeSep).toBeDefined();
    expect(narrative.layoutOptions?.rankSep).toBeUndefined();
    expect(narrative.layoutOptions?.nodeSep).toBeUndefined();
  });

  it('Three-layer differentiation matrix holds across both Timeline surfaces', () => {
    const brief = createTimelineCanvasAdapter(worldCtxRef, 'brief');
    const worldNarrative = createTimelineCanvasAdapter(worldCtxRef, 'narrative');
    const workNarrative = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const moment = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');

    expect(worldNarrative.layoutOptions?.direction).toBe(
      workNarrative.layoutOptions?.direction,
    );
    expect(worldNarrative.layoutOptions?.rankSep).toBeUndefined();
    expect(workNarrative.layoutOptions?.rankSep).toBeUndefined();

    // The strongest differentiator is Brief/Narrative density (LR/LR with
    // different spacing) + Moment direction (TB). A screenshot reads each
    // as a distinct instrument.
    expect(brief.layoutOptions?.direction).toBe('LR');
    expect(worldNarrative.layoutOptions?.direction).toBe('LR');
    expect(workNarrative.layoutOptions?.direction).toBe('LR');
    expect(moment.layoutOptions?.direction).toBe('TB');

    const briefRank = brief.layoutOptions?.rankSep ?? 80;
    const narrativeRank = worldNarrative.layoutOptions?.rankSep ?? 80;
    const momentRank = moment.layoutOptions?.rankSep ?? 80;
    expect(briefRank).toBeGreaterThan(narrativeRank);
    expect(momentRank).toBeLessThan(narrativeRank);
  });

  it('Layer adapter construction is referentially stable across renders (V1.114 §3.3.1)', () => {
    const a1 = createTimelineCanvasAdapter(worldCtxRef, 'brief').layoutOptions;
    const a2 = createTimelineCanvasAdapter(worldCtxRef, 'brief').layoutOptions;
    expect(a1).toEqual(a2);

    const m1 = createWorkTimelineCanvasAdapter(workCtxRef, 'moment').layoutOptions;
    const m2 = createWorkTimelineCanvasAdapter(workCtxRef, 'moment').layoutOptions;
    expect(m1).toEqual(m2);
  });
});
