/**
 * Work Timeline canvas node types — V1.123 P2 Task 2/3 + V1.124 P0 T2.
 *
 * Three node kinds, all projecting from V1.72 `WorkOutline`:
 *   • work-timeline-narrative-event — Narrative event marker. Body chrome:
 *                                      `WorkTimelineNarrativeEventChrome`.
 *   • work-timeline-moment-scene    — Moment scene card. Body chrome:
 *                                      `WorkTimelineMomentSceneChrome`.
 *   • work-timeline-moment-beat     — Moment beat pin. Body chrome:
 *                                      `WorkTimelineMomentBeatChrome`.
 *
 * V1.124 P0 T2 — RF wrappers are thin App-local shells:
 *   `NodeChromeShell` + `Handle`s + presentational body extract + RF
 *   `selected`/`dragging`. Body chrome lives in
 *   `../presentational/timeline-node-chrome` (Studio-reachable as
 *   `@web-canvas/timeline-node-chrome`). i18n stays here.
 *
 * Surface spines: Narrative uses `accent="worldkb"` (Timeline-family
 * continuity); Moment uses `accent="outline"` (outline-derived). Layer
 * accents (narrative / moment) live on the body extract badges/icons.
 */
import { memo } from 'react';
import { useTranslation } from 'react-i18next';
import { Handle, Position, type NodeProps } from '@xyflow/react';

import { NodeChromeShell } from '../presentational/node-chrome-shell';
import {
  WorkTimelineMomentBeatChrome,
  WorkTimelineMomentSceneChrome,
  WorkTimelineNarrativeEventChrome,
} from '../presentational/timeline-node-chrome';
import { DirectedAxisSpine } from '../timeline-canvas/directed-axis-spine';
import type { WorkTimelineNodeData } from './work-timeline-canvas-adapter';

// ─── Narrative event node (Task 2) ─────────────────────────────────────────

/**
 * Narrative event node — projected onto the Work Timeline Narrative LR
 * when-axis. Body chrome extracted to `WorkTimelineNarrativeEventChrome`
 * (V1.124). i18n resolves chapter-anchor / no-chapter labels here.
 */
export const WorkTimelineNarrativeEventNode = memo(function WorkTimelineNarrativeEventNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  const chapterAnchor =
    d.realizesChapterId !== undefined
      ? t('workTimeline.narrativeEventNode.chapterBadge', {
          chapter: d.realizesChapterId,
          defaultValue: 'Ch. {{chapter}}',
        })
      : null;

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
      <WorkTimelineNarrativeEventChrome
        title={
          d.label ||
          t('workTimeline.narrativeEventNode.unnamed', {
            defaultValue: '(unnamed event)',
          })
        }
        eventId={d.eventId ?? ''}
        chapterAnchor={chapterAnchor}
        noChapterLabel={t('workTimeline.narrativeEventNode.noChapter', {
          defaultValue: 'No chapter anchor',
        })}
        description={d.description}
      />
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

// ─── Moment scene node (Task 3) ────────────────────────────────────────────

/**
 * Moment scene node — projected onto the Work Timeline Moment vertical
 * scene-stack. Body chrome extracted to `WorkTimelineMomentSceneChrome`
 * (V1.124). Manuscript-anchor badge resolved here via i18n.
 */
export const WorkTimelineMomentSceneNode = memo(function WorkTimelineMomentSceneNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  const manuscriptAnchorLabel = d.manuscriptAnchor
    ? t('workTimeline.momentSceneNode.anchor', {
        chapter: d.manuscriptAnchor.chapterId,
        scene: d.manuscriptAnchor.sceneId,
        defaultValue: 'Ch. {{chapter}} · {{scene}}',
      })
    : null;

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
      <WorkTimelineMomentSceneChrome
        title={
          d.label ||
          t('workTimeline.momentSceneNode.unnamed', {
            defaultValue: '(untitled scene)',
          })
        }
        sceneId={d.sceneId ?? ''}
        manuscriptAnchorLabel={manuscriptAnchorLabel}
        status={d.status ?? undefined}
      />
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
 * Body chrome extracted to `WorkTimelineMomentBeatChrome` (V1.124).
 */
export const WorkTimelineMomentBeatNode = memo(function WorkTimelineMomentBeatNode({
  data,
  selected,
  dragging,
}: NodeProps) {
  const d = data as WorkTimelineNodeData;
  const { t } = useTranslation('canvas');

  const manuscriptAnchorLabel = d.manuscriptAnchor
    ? t('workTimeline.momentBeatNode.anchor', {
        chapter: d.manuscriptAnchor.chapterId,
        scene: d.manuscriptAnchor.sceneId,
        beat: d.manuscriptAnchor.beatId,
        defaultValue: 'Ch. {{chapter}} · {{scene}} · {{beat}}',
      })
    : null;

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
      <WorkTimelineMomentBeatChrome
        title={
          d.label ||
          t('workTimeline.momentBeatNode.unnamed', {
            defaultValue: '(untitled beat)',
          })
        }
        manuscriptAnchorLabel={manuscriptAnchorLabel}
        status={d.status ?? undefined}
      />
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
 * Consumed by `createWorkTimelineCanvasAdapter` and forwarded to
 * `useCanvasSurface` (V1.114) so React Flow can dispatch by `node.type`.
 */
export const workTimelineNodeTypes = {
  'work-timeline-narrative-event': WorkTimelineNarrativeEventNode,
  'work-timeline-moment-scene': WorkTimelineMomentSceneNode,
  'work-timeline-moment-beat': WorkTimelineMomentBeatNode,
  'directedAxisSpine': DirectedAxisSpine,
} as const;
