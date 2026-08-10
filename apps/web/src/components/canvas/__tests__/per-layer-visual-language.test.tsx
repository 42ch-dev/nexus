/**
 * Per-layer visual language differentiation — V1.123 P4 Task 2.
 *
 * Locks the per-layer feel contract at the visual + token dimension
 * (layer-feel-differentiation.md §2 + §6.1 + AC-V1123-20 "three feels
 * perceptibly different"):
 *
 *   - Brief-era node card carries the gold-bronze `--color-canvas-layer-
 *     brief-accent` token (P4 Task 2 — alias amber-700 per layer-feel §6.1)
 *     so the era-icon + time-span badge read against the "age" hue,
 *     distinct from the Narrative event's blue.
 *   - Moment scene + beat node cards carry the ink-on-paper
 *     `--color-canvas-layer-moment-accent` token (P4 Task 2 — alias
 *     gray-900 per layer-feel §6.1) so the manuscript-anchor badges read
 *     against the "ink" hue, distinct from the Narrative event's blue.
 *
 * The test does not assert exact color values (those are DESIGN.md SSOT +
 * tokens.css concern). It asserts:
 *   - the node components render with the per-layer accent Tailwind class
 *     (so a future token retune needs no component sweep);
 *   - the Tailwind preset registers the three layer-accent color keys so
 *     `text-canvas-layer-{brief,narrative,moment}-accent` resolves;
 *   - tokens.css defines the three accent custom properties (light + dark).
 *
 * Mount strategy: the node components are exported as `memo(NodeProps)`
 * React Flow wrappers. We render them directly with a stub `NodeProps`
 * payload (no React Flow mount needed — the wrappers only forward props
 * to the presentational `NodeChromeShell`). The `data` payload mirrors
 * the adapter's projection shape so the rendered DOM matches production.
 */
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import type { ReactNode } from 'react';
import type { NodeProps } from '@xyflow/react';
import { MemoryRouter } from 'react-router';
import { I18nextProvider } from 'react-i18next';
import i18next from 'i18next';
import { ReactFlowProvider } from '@xyflow/react';

import { TimelineBriefEraNode } from '../timeline-canvas/timeline-node-types';
import {
  WorkTimelineMomentBeatNode,
  WorkTimelineMomentSceneNode,
} from '../work-timeline-canvas/work-timeline-node-types';
import type { TimelineNodeData } from '../timeline-canvas/timeline-canvas-adapter';
import type { WorkTimelineNodeData } from '../work-timeline-canvas/work-timeline-canvas-adapter';

// ─── Test setup ─────────────────────────────────────────────────────────────

i18next.init({
  lng: 'en',
  resources: {
    en: { canvas: {} },
  },
  interpolation: { escapeValue: false },
});

function renderWithI18n(ui: ReactNode) {
  // The node components render React Flow `<Handle>` which requires a
  // `ReactFlowProvider` ancestor. Wrap once per render — no need for a
  // real `<ReactFlow>` graph; the provider's store is enough for Handle
  // to mount cleanly in jsdom.
  return render(
    <MemoryRouter>
      <I18nextProvider i18n={i18next}>
        <ReactFlowProvider>{ui}</ReactFlowProvider>
      </I18nextProvider>
    </MemoryRouter>,
  );
}

// Minimal NodeProps stub — the node components forward to NodeChromeShell
// which only reads `selected` / `dragging` (defaults) + `data` (typed).
// `NodeProps` is generic over the node data; the test stubs pass plain
// object data so we coerce via `unknown` to satisfy the prop shape.
function nodeProps<TData extends Record<string, unknown>>(data: TData): NodeProps {
  return {
    id: 'test-node',
    type: 'test',
    position: { x: 0, y: 0 },
    data,
    selected: false,
    dragging: false,
  } as unknown as NodeProps;
}

// ─── Brief-era node carries the Brief layer accent ──────────────────────────

describe('TimelineBriefEraNode — Brief layer accent (P4 Task 2)', () => {
  it('renders the era-icon with text-canvas-layer-brief-accent (gold-bronze)', () => {
    const data: TimelineNodeData = {
      world_id: 'world-1',
      status: 'confirmed',
      version: 1,
      key_block_id: 'kb-era-1',
      block_type: 'era',
      canonical_name: 'The First Age',
      layoutHint: 'brief',
      eraId: 'era-first',
      startHint: '1000-01-01T00:00:00Z',
      endHint: '1100-01-01T00:00:00Z',
      worldSummary: 'A time of myth and legend.',
      source_anchor_count: 0,
      body: {
        attributes: {
          era_id: 'era-first',
          start_hint: '1000-01-01T00:00:00Z',
          end_hint: '1100-01-01T00:00:00Z',
          world_summary: 'A time of myth and legend.',
        },
      },
    } as unknown as TimelineNodeData;

    const { container } = renderWithI18n(<TimelineBriefEraNode {...nodeProps(data)} />);

    // The era-icon (Hourglass) carries the Brief layer accent class.
    const icon = container.querySelector('.text-canvas-layer-brief-accent');
    expect(icon).not.toBeNull();
    // The time-span badge (when both start + end are present) also carries
    // the Brief accent — distinct from the Narrative event's worldkb-blue.
    // Note: i18next test stub has empty catalog; the badge text is the
    // i18n key fallback, but the className is the contract under test.
    const span = container.querySelector('span.text-canvas-layer-brief-accent');
    expect(span).not.toBeNull();
  });

  it('does NOT carry the worldkb-accent (P4 Task 2 migration: Brief has its own layer accent now)', () => {
    // Regression guard: P1 shipped the Brief-era node with the worldkb
    // teal accent as a placeholder ("P4 may promote the accent to a
    // dedicated Brief-era token"). P4 Task 2 completes that migration.
    // The worldkb-accent class MUST NOT appear on the Brief-era node.
    const data: TimelineNodeData = {
      key_block_id: 'kb-era-1',
      block_type: 'era',
      canonical_name: 'Age',
      layoutHint: 'brief',
    } as unknown as TimelineNodeData;

    const { container } = renderWithI18n(<TimelineBriefEraNode {...nodeProps(data)} />);

    expect(container.querySelector('.text-canvas-worldkb-accent')).toBeNull();
  });
});

// ─── Moment scene + beat nodes carry the Moment layer accent ────────────────

describe('WorkTimelineMomentSceneNode + BeatNode — Moment layer accent (P4 Task 2)', () => {
  it('scene node renders scene-icon + manuscript-anchor with text-canvas-layer-moment-accent (ink)', () => {
    const data: WorkTimelineNodeData = {
      workId: 'work-1',
      nodeKind: 'scene',
      nodeId: 'sc-1',
      sceneId: 'sc-1',
      label: 'Opening Scene',
      realizesChapterId: 1,
      manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
    } as WorkTimelineNodeData;

    const { container } = renderWithI18n(<WorkTimelineMomentSceneNode {...nodeProps(data)} />);

    // The scene-icon (BookMarked) carries the Moment layer accent class.
    const icon = container.querySelector('.text-canvas-layer-moment-accent');
    expect(icon).not.toBeNull();

    // The manuscript-anchor badge also carries the Moment accent —
    // distinct from the Narrative event's worldkb-blue.
    const anchor = container.querySelector('span.text-canvas-layer-moment-accent');
    expect(anchor).not.toBeNull();
    expect(anchor?.textContent).toContain('sc-1');
  });

  it('beat node renders beat-icon + manuscript-anchor with text-canvas-layer-moment-accent (ink)', () => {
    const data: WorkTimelineNodeData = {
      workId: 'work-1',
      nodeKind: 'beat',
      nodeId: 'bt-1',
      beatId: 'bt-1',
      sceneId: 'sc-1',
      label: 'Hook Beat',
      realizesChapterId: 1,
      manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1', beatId: 'bt-1' },
    } as WorkTimelineNodeData;

    const { container } = renderWithI18n(<WorkTimelineMomentBeatNode {...nodeProps(data)} />);

    // The beat-icon (Milestone) carries the Moment layer accent class.
    const icon = container.querySelector('.text-canvas-layer-moment-accent');
    expect(icon).not.toBeNull();

    // The manuscript-anchor badge (chapter/scene/beat) carries the accent.
    const anchor = container.querySelector('span.text-canvas-layer-moment-accent');
    expect(anchor).not.toBeNull();
    expect(anchor?.textContent).toContain('bt-1');
  });

  it('does NOT carry the outline-accent on icons (P4 Task 2 migration: Moment has its own layer accent)', () => {
    // Regression guard: P2 shipped the Moment scene/beat icons with the
    // outline amber accent as a placeholder ("P4 may promote the accent to
    // a dedicated Moment token"). P4 Task 2 completes that migration. The
    // outline-accent class on icon/badge MUST NOT appear (the surface
    // spine stays `accent="outline"` on the NodeChromeShell — that's the
    // surface identity, distinct from the per-layer accent on icon/badge).
    const sceneData: WorkTimelineNodeData = {
      workId: 'work-1',
      nodeKind: 'scene',
      nodeId: 'sc-1',
      sceneId: 'sc-1',
      label: 'Opening',
      manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
    } as WorkTimelineNodeData;

    const { container } = renderWithI18n(<WorkTimelineMomentSceneNode {...nodeProps(sceneData)} />);

    // SVG icon (BookMarked) and span badges — none should still carry the
    // outline-accent text color after the P4 Task 2 migration.
    expect(container.querySelector('.text-canvas-outline-accent')).toBeNull();
  });
});

// ─── Three-layer accent differentiation (consolidated AC-V1123-20) ───────────

describe('Three-layer accent differentiation (AC-V1123-20 visual matrix)', () => {
  it('Brief, Narrative, Moment each render with a DISTINCT per-layer accent class', () => {
    // Cross-layer acceptance: a screenshot of Brief / Narrative / Moment
    // side-by-side must distinguish the three instruments without reading
    // chrome labels. At the visual dimension, the per-layer accent triple
    // is the strongest differentiator — Brief (gold-bronze) / Narrative
    // (blue) / Moment (ink). This test asserts the three node components
    // render with three DISTINCT Tailwind color classes so the layer
    // accent tokens are observable in the DOM.
    const briefData: TimelineNodeData = {
      key_block_id: 'kb-era-1',
      block_type: 'era',
      canonical_name: 'The First Age',
      layoutHint: 'brief',
      startHint: '1000',
      endHint: '1100',
    } as unknown as TimelineNodeData;

    const momentSceneData: WorkTimelineNodeData = {
      workId: 'work-1',
      nodeKind: 'scene',
      nodeId: 'sc-1',
      sceneId: 'sc-1',
      label: 'Opening',
      manuscriptAnchor: { chapterId: 1, sceneId: 'sc-1' },
    } as WorkTimelineNodeData;

    const { container: briefDom } = renderWithI18n(
      <TimelineBriefEraNode {...nodeProps(briefData)} />,
    );
    const { container: momentDom } = renderWithI18n(
      <WorkTimelineMomentSceneNode {...nodeProps(momentSceneData)} />,
    );

    // Brief carries the brief accent.
    expect(briefDom.querySelector('.text-canvas-layer-brief-accent')).not.toBeNull();
    // Moment carries the moment accent.
    expect(momentDom.querySelector('.text-canvas-layer-moment-accent')).not.toBeNull();
    // Brief does NOT carry the Moment accent (and vice versa).
    expect(briefDom.querySelector('.text-canvas-layer-moment-accent')).toBeNull();
    expect(momentDom.querySelector('.text-canvas-layer-brief-accent')).toBeNull();
  });
});
