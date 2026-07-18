/**
 * Per-layer layout differentiation — V1.123 P4 Task 1.
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

// ─── Fixtures ───────────────────────────────────────────────────────────────

const worldCtxRef: { current: TimelineCanvasAdapterContext } = {
  current: { worldId: 'world-1' },
};

const workCtxRef: { current: WorkTimelineCanvasAdapterContext } = {
  current: { workId: 'work-1' },
};

/**
 * Light stub of a `WorldKbGraphResponse` so adapter construction does not
 * blow up on a missing graph. The graph itself is irrelevant for layout
 * option tests — we never call `projectGraph` here, only read the adapter's
 * `layoutOptions`.
 */
// const EMPTY_WORLD_GRAPH = { ... }; // (removed — not referenced after
// the layout-options tests switched to reading `adapter.layoutOptions`
// directly without invoking projectGraph; kept the comment to document
// why no fixture is needed at this level.)

// ─── Three-layer layout differentiation (P4 Task 1) ─────────────────────────

describe('Per-layer layout differentiation (V1.123 P4 Task 1)', () => {
  it('Brief layer (World Timeline) emits a horizontal era-sweep layout: LR + wide rankSep + tight nodeSep', () => {
    // layer-feel §2.2 + Plan Task 1 spec: Brief = "horizontal era sweep".
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
    // layer-feel §2.3: "Narrative is the normative baseline — balanced L→R
    // event axis + Context clusters off-axis." Narrative MUST NOT carry
    // layer-specific rankSep/nodeSep; it leaves them undefined so
    // `useAutoLayout`'s internal defaults (80/80) apply — V1.122 regression.
    const adapter = createTimelineCanvasAdapter(worldCtxRef, 'narrative');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('LR');
    expect(opts?.rankSep).toBeUndefined();
    expect(opts?.nodeSep).toBeUndefined();
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Narrative layer (Work Timeline) inherits the same V1.122 LR baseline', () => {
    // layer-feel §2.3 + architect §7.3: Narrative is the shared V1.122
    // baseline across BOTH Timeline surfaces. The Work Timeline Narrative
    // adapter MUST NOT carry Moment-specific layout knobs — direction is LR
    // and rankSep/nodeSep stay undefined so dagre's internal defaults apply.
    const adapter = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('LR');
    expect(opts?.rankSep).toBeUndefined();
    expect(opts?.nodeSep).toBeUndefined();
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Moment layer (Work Timeline) emits a vertical scene-stack: TB + tight rankSep + tight nodeSep', () => {
    // layer-feel §2.4 + Plan Task 1 spec: Moment = "vertical scene-stack
    // (T→B preferred); scene/beat cards + manuscript-anchor badges."
    // The exact values (60/30) are tuning knobs; the contract is:
    //   - direction === 'TB' (vertical reading — DIFFERENT from Brief /
    //     Narrative LR so a screenshot reads as a different instrument)
    //   - rankSep + nodeSep both present and tight (dense manuscript stack)
    //   - hasSuppliedPositions === true (supplied scene/beat positions win
    //     on first open; these values only kick in on explicit relayout)
    const adapter = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');
    const opts = adapter.layoutOptions;

    expect(opts?.direction).toBe('TB');
    expect(opts?.rankSep).toBe(60);
    expect(opts?.nodeSep).toBe(30);
    expect(opts?.hasSuppliedPositions).toBe(true);
  });

  it('Brief vs Narrative (World Timeline) differentiate on direction + density knobs', () => {
    // AC-V1123-20 cross-layer bar: a screenshot pack must distinguish Brief
    // from Narrative without reading chrome labels. The layout dimension of
    // that contract: Brief carries EXPLICIT rankSep/nodeSep (sweep era
    // spacing); Narrative leaves them undefined (baseline). Both share LR
    // direction (Timeline family reading direction) — differentiation is on
    // density, not axis.
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
    // AC-V1123-20 cross-layer bar: Narrative (LR) and Moment (TB) carry
    // DIFFERENT layout directions so a screenshot reads as a different
    // instrument. This is the strongest layout-dimension differentiator in
    // the three-layer contract — direction change, not just spacing change.
    const narrative = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const moment = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');

    expect(narrative.layoutOptions?.direction).toBe('LR');
    expect(moment.layoutOptions?.direction).toBe('TB');

    // Moment carries explicit tight rankSep/nodeSep; Narrative leaves them
    // undefined (V1.122 baseline).
    expect(moment.layoutOptions?.rankSep).toBeDefined();
    expect(moment.layoutOptions?.nodeSep).toBeDefined();
    expect(narrative.layoutOptions?.rankSep).toBeUndefined();
    expect(narrative.layoutOptions?.nodeSep).toBeUndefined();
  });

  it('Three-layer differentiation matrix holds across both Timeline surfaces', () => {
    // Cross-surface acceptance: Brief / Narrative / Moment each present a
    // distinct (direction, density) pair so the caller's "三层不一样的感受"
    // mandate holds at the layout dimension. This is the consolidated
    // AC-V1123-20 assertion P4 QC references when signing the three-layer
    // feel contract.
    const brief = createTimelineCanvasAdapter(worldCtxRef, 'brief');
    const worldNarrative = createTimelineCanvasAdapter(worldCtxRef, 'narrative');
    const workNarrative = createWorkTimelineCanvasAdapter(workCtxRef, 'narrative');
    const moment = createWorkTimelineCanvasAdapter(workCtxRef, 'moment');

    // Two Narrative adapters (World + Work) share the SAME V1.122 baseline
    // contract — Narrative is the shared familiar layer across surfaces.
    expect(worldNarrative.layoutOptions?.direction).toBe(
      workNarrative.layoutOptions?.direction,
    );
    expect(worldNarrative.layoutOptions?.rankSep).toBeUndefined();
    expect(workNarrative.layoutOptions?.rankSep).toBeUndefined();

    // Three-layer direction matrix: Brief=LR, Narrative=LR, Moment=TB.
    // The strongest differentiator is Brief/Narrative density (LR/LR with
    // different spacing) + Moment direction (TB). A screenshot reads each
    // as a distinct instrument.
    expect(brief.layoutOptions?.direction).toBe('LR');
    expect(worldNarrative.layoutOptions?.direction).toBe('LR');
    expect(workNarrative.layoutOptions?.direction).toBe('LR');
    expect(moment.layoutOptions?.direction).toBe('TB');

    // Three-layer density matrix: Brief=wide rankSep (240), Narrative=baseline
    // (80 default), Moment=tight rankSep (60). Three distinct density buckets.
    const briefRank = brief.layoutOptions?.rankSep ?? 80;
    const narrativeRank = worldNarrative.layoutOptions?.rankSep ?? 80;
    const momentRank = moment.layoutOptions?.rankSep ?? 80;
    expect(briefRank).toBeGreaterThan(narrativeRank);
    expect(momentRank).toBeLessThan(narrativeRank);
  });

  it('Layer adapter construction is referentially stable across renders (V1.114 §3.3.1)', () => {
    // Adapter stability invariant: the same ctxRef + same active layer
    // MUST return adapters whose layoutOptions are shape-equal (factory
    // re-runs are cheap; consumer-side `useMemo([activeLayer], ...)` keys
    // only on layer change).
    const a1 = createTimelineCanvasAdapter(worldCtxRef, 'brief').layoutOptions;
    const a2 = createTimelineCanvasAdapter(worldCtxRef, 'brief').layoutOptions;
    expect(a1).toEqual(a2);

    const m1 = createWorkTimelineCanvasAdapter(workCtxRef, 'moment').layoutOptions;
    const m2 = createWorkTimelineCanvasAdapter(workCtxRef, 'moment').layoutOptions;
    expect(m1).toEqual(m2);
  });
});
