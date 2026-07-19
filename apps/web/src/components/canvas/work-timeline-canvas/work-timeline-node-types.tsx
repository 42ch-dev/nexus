/**
 * Work Timeline canvas node types — V1.123 P2 Task 2 (Narrative event node) +
 * Task 3 (Moment scene + beat nodes).
 *
 * Three node kinds, all projecting from V1.72 `WorkOutline`:
 *   • work-timeline-narrative-event — Task 2 Narrative event marker
 *                                      (`outline.timeline_events[]`
 *                                      projected onto the Narrative LR
 *                                      when-axis). Renders the event's
 *                                      title + event-id pill + optional
 *                                      chapter-anchor badge + optional
 *                                      description. Distinct from the
 *                                      V1.122 World Timeline event node:
 *                                      no source_anchor_count, no
 *                                      `body.attributes.occurred_at` —
 *                                      the Work Timeline is Work-scoped,
 *                                      not World-scoped.
 *   • work-timeline-moment-scene    — Task 3 Moment scene card
 *                                      (V1.108 `OutlineSceneNodeData`
 *                                      fixture; vertical scene-stack per
 *                                      layer-feel §2.4). Manuscript-anchor
 *                                      badge mandatory when anchor data
 *                                      exists.
 *   • work-timeline-moment-beat     — Task 3 Moment beat pin (V1.108
 *                                      `OutlineBeatNodeData` fixture).
 *                                      Manuscript-anchor badge mandatory.
 *
 * Chrome tokens: reuses V1.121 `canvas-node-fill` / `canvas-node-border`
 * via `NodeChromeShell`. The Work Timeline surface is Work-scoped; it
 * adopts the existing `worldkb` accent spine (teal-700 per DESIGN.md
 * §Canvas Surface) for the Narrative layer — same as V1.122 World Timeline
 * for visual continuity (Timeline-family surfaces share the accent spine).
 * The Moment layer reuses the `outline` accent spine (amber-700) for the
 * scene/beat cards — Work Timeline Moment is Work-scoped outline-projection,
 * so the outline spine signals "Outline-derived data" to the author.
 *
 * V1.123 P4 Task 2 — per-layer feel accent migration (layer-feel-
 * differentiation.md §6.1 + AC-V1123-20): the Moment scene + beat nodes now
 * carry the dedicated `--color-canvas-layer-moment-accent` token
 * (ink-on-paper manuscript tone — alias gray-900 per layer-feel §6.1)
 * instead of the outline amber spine. This is the per-LAYER accent within
 * the Work Timeline surface — the scene-icon + beat-pin + manuscript-anchor
 * badges all read against the ink-on-paper hue so a screenshot of Moment
 * vs Narrative reads as a different instrument without reading chrome
 * labels. The card's surface spine stays `accent="outline"` because the
 * Work Timeline surface identity is still outline-derived (the layer accent
 * is an INTRA-surface differentiator, not a surface identity override).
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';
import { BookMarked, Flag, Milestone } from 'lucide-react';

import { NodeChromeShell } from '../presentational/node-chrome-shell';
import type { WorkTimelineNodeData } from './work-timeline-canvas-adapter';
// ─── Narrative event node (Task 2) ─────────────────────────────────────────

/**
 * Narrative event node — projected onto the Work Timeline Narrative LR
 * when-axis. Renders the event's title + event-id pill + optional
 * chapter-anchor badge + optional description.
 *
 * Visual differentiation from the V1.122 World Timeline event node
 * (`TimelineEventNode`): the Work Timeline event node carries a
 * chapter-anchor badge (not an `occurred_at` time-string badge) and no
 * source-anchor count (Work Timeline is Work-scoped, not World-scoped).
 * The `Flag` icon (literary narrative tone via `text-canvas-worldkb-accent`)
 * leads the card so a screenshot of Work Timeline vs World Timeline reads
 * as a different instrument.
 *
 * i18n: `t('workTimeline.narrativeEventNode.*')` with `defaultValue` fallback
 * so the node renders even before the en/zh-CN catalogs gain the keys
 * (Task 4/5 will add the catalog entries — Task 2 ships the node with
 * inline `defaultValue`s per V1.112 i18n discipline).
 */
export const WorkTimelineNarrativeEventNode = memo(function WorkTimelineNarrativeEventNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="worldkb"
      aria-label={t('workTimeline.narrativeEventNode.aria', {
        name: d.label,
        defaultValue: 'Work timeline event: {{name}}',
      })}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <div className="flex items-center gap-2">
        <Flag
          className="h-4 w-4 flex-shrink-0 text-canvas-worldkb-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={d.label}
        >
          {d.label || t('workTimeline.narrativeEventNode.unnamed', {
            defaultValue: '(unnamed event)',
          })}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {d.eventId}
        </span>
        {d.realizesChapterId !== undefined ? (
          <span className="rounded-pill border border-canvas-worldkb-accent/30 bg-canvas-worldkb-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-worldkb-accent">
            {t('workTimeline.narrativeEventNode.chapterBadge', {
              chapter: d.realizesChapterId,
              defaultValue: 'Ch. {{chapter}}',
            })}
          </span>
        ) : (
          <span className="rounded-pill border border-gray-alpha-400 bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {t('workTimeline.narrativeEventNode.noChapter', {
              defaultValue: 'No chapter anchor',
            })}
          </span>
        )}
      </div>
      {d.description ? (
        <p
          className="mt-1 line-clamp-2 text-label-12 text-gray-700"
          title={d.description}
        >
          {d.description}
        </p>
      ) : null}
      <Handle
        type="source"
        position={Position.Right}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? (
        <span className="sr-only">
          {t('workTimeline.narrativeEventNode.selected', {
            defaultValue: 'Selected work timeline event',
          })}
        </span>
      ) : null}
    </NodeChromeShell>
  );
});

// ─── Node type registry ────────────────────────────────────────────────────

// ─── Moment scene node (Task 3) ────────────────────────────────────────────

/**
 * Moment scene node — projected onto the Work Timeline Moment vertical
 * scene-stack. Renders the scene's title + scene-id pill + manuscript-anchor
 * badge (mandatory when anchor data exists per layer-feel §2.4) + optional
 * scene status chip.
 *
 * Visual differentiation from the Narrative event node (layer-feel §2.4):
 * the `BookMarked` icon (literary manuscript tone via
 * `text-canvas-layer-moment-accent`) leads the card; the manuscript-anchor
 * badge is prominent (chapter/scene link). The card reads as "scene-level
 * reading distance" — denser and manuscript-anchored, distinct from the
 * Narrative LR event timeline. The Moment layer accent is the dedicated
 * `--color-canvas-layer-moment-accent` token (P4 Task 2 — alias gray-900
 * ink-on-paper per layer-feel §6.1).
 */
export const WorkTimelineMomentSceneNode = memo(function WorkTimelineMomentSceneNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="outline"
      aria-label={t('workTimeline.momentSceneNode.aria', {
        name: d.label,
        defaultValue: 'Scene: {{name}}',
      })}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <div className="flex items-center gap-2">
        <BookMarked
          className="h-4 w-4 flex-shrink-0 text-canvas-layer-moment-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={d.label}
        >
          {d.label || t('workTimeline.momentSceneNode.unnamed', {
            defaultValue: '(untitled scene)',
          })}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {d.sceneId}
        </span>
        {d.manuscriptAnchor ? (
          <span className="rounded-pill border border-canvas-layer-moment-accent/30 bg-canvas-layer-moment-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-moment-accent">
            {t('workTimeline.momentSceneNode.anchor', {
              chapter: d.manuscriptAnchor.chapterId,
              scene: d.manuscriptAnchor.sceneId,
              defaultValue: 'Ch. {{chapter}} · {{scene}}',
            })}
          </span>
        ) : null}
        {d.status ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {d.status}
          </span>
        ) : null}
      </div>
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? (
        <span className="sr-only">
          {t('workTimeline.momentSceneNode.selected', {
            defaultValue: 'Selected scene',
          })}
        </span>
      ) : null}
    </NodeChromeShell>
  );
});

// ─── Moment beat node (Task 3) ─────────────────────────────────────────────

/**
 * Moment beat node — child of a Scene card in the vertical scene-stack.
 * Renders the beat's title + beat-id pill + manuscript-anchor badge
 * (chapter/scene/beat link) + optional beat status chip.
 *
 * The `Milestone` icon (beat-pin tone via `text-canvas-layer-moment-accent`)
 * leads the card; the manuscript-anchor badge is prominent. Reads as
 * "beat precision — inside the scene", distinct from the Scene card
 * (one level up) and the Narrative event (one layer up). The Moment layer
 * accent is the dedicated `--color-canvas-layer-moment-accent` token
 * (P4 Task 2 — alias gray-900 ink-on-paper).
 */
export const WorkTimelineMomentBeatNode = memo(function WorkTimelineMomentBeatNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  return (
    <NodeChromeShell
      selected={selected}
      dragging={dragging}
      accent="outline"
      aria-label={t('workTimeline.momentBeatNode.aria', {
        name: d.label,
        defaultValue: 'Beat: {{name}}',
      })}
    >
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      <div className="flex items-center gap-2">
        <Milestone
          className="h-3.5 w-3.5 flex-shrink-0 text-canvas-layer-moment-accent"
          aria-hidden
        />
        <span
          className="truncate font-heading text-copy-13 font-medium text-gray-1000"
          title={d.label}
        >
          {d.label || t('workTimeline.momentBeatNode.unnamed', {
            defaultValue: '(untitled beat)',
          })}
        </span>
      </div>
      <div className="mt-0.5 flex flex-wrap items-center gap-1">
        {d.manuscriptAnchor ? (
          <span className="rounded-pill border border-canvas-layer-moment-accent/30 bg-canvas-layer-moment-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-moment-accent">
            {t('workTimeline.momentBeatNode.anchor', {
              chapter: d.manuscriptAnchor.chapterId,
              scene: d.manuscriptAnchor.sceneId,
              beat: d.manuscriptAnchor.beatId,
              defaultValue: 'Ch. {{chapter}} · {{scene}} · {{beat}}',
            })}
          </span>
        ) : null}
        {d.status ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12 text-gray-700">
            {d.status}
          </span>
        ) : null}
      </div>
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2.5 !w-2.5 !border-canvas-port !bg-canvas-port"
      />
      {selected ? (
        <span className="sr-only">
          {t('workTimeline.momentBeatNode.selected', {
            defaultValue: 'Selected beat',
          })}
        </span>
      ) : null}
    </NodeChromeShell>
  );
});

// ─── Node type registry (Task 2 + Task 3) ──────────────────────────────────

/**
 * Node type registry for the Work Timeline surface.
 *
 * Task 2 registered the Narrative event node; Task 3 adds the Moment
 * scene + beat nodes. Task 6 (feel differentiation) will refine the
 * visuals without changing the registry shape. The registry is consumed
 * by `createWorkTimelineCanvasAdapter` and forwarded to `useCanvasSurface`
 * (V1.114) so React Flow can dispatch by `node.type`.
 */
export const workTimelineNodeTypes = {
  'work-timeline-narrative-event': WorkTimelineNarrativeEventNode,
  'work-timeline-moment-scene': WorkTimelineMomentSceneNode,
  'work-timeline-moment-beat': WorkTimelineMomentBeatNode,
} as const;
