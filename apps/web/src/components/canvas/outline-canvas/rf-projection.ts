/**
 * Outline canvas — pure React Flow graph projection (V1.108 P0).
 *
 * Converts the canonical {@link WorkOutline} + {@link ChapterSummary}[] read
 * models into React Flow `Node[]` / `Edge[]` arrays with a **deterministic
 * lane/layered layout** (same class as the World KB grid pattern —
 * `canvas-strategy-surface.md` §3.3 surface 2; primary spec § Layout).
 *
 * Lanes:
 *   • volumes   — left column
 *   • chapters  — center column (ordered by volume assignment then chapter no)
 *   • timeline  — right column (ordered by outline declaration)
 *
 * Edge kinds:
 *   • contains       — volume → chapter (from `outline.volumes[].chapter_ids`)
 *   • realizes_event — chapter → timeline-event (when `realizes_chapter_id` set)
 *   • foreshadows    — event → event (from `outline.foreshadows[]`)
 *
 * Pure: no React, no side effects, no mutation of inputs. The layout engine
 * (dagre/elk) is deferred to a future iteration — this is a pragmatic
 * deterministic grid, matching the World KB approach.
 */
import type { Edge, Node } from '@xyflow/react';
import type {
  ChapterStatus,
  ChapterSummary,
  WorkOutline,
} from '@42ch/nexus-contracts';

import { chapterDisplayTitle } from './graph-projection';

// ---------------------------------------------------------------------------
// Node data payloads (UI-only; wire DTOs in @42ch/nexus-contracts remain SSOT)
// ---------------------------------------------------------------------------

/** React Flow node data for a Volume lane node. */
export interface OutlineVolumeNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  workId: string;
  volumeId: number;
  label: string;
  chapterCount: number;
}

/** React Flow node data for a Chapter card node. */
export interface OutlineChapterNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  workId: string;
  chapterId: number;
  /** `null` when the chapter is unassigned (no volume claims it). */
  volumeId: number | null;
  title: string;
  slug: string | null;
  status: ChapterStatus;
  plannedWordCount: number;
  actualWordCount: number | null;
}

/** React Flow node data for a Timeline Event lane node. */
export interface OutlineTimelineEventNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  workId: string;
  eventId: string;
  title: string;
  description: string | null;
  /** Chapter this event realizes, if any. */
  realizesChapterId: number | null;
}

/** Edge data payload for outline structural / temporal edges. */
export interface OutlineEdgeData {
  /** React Flow requires an index signature on edge data. */
  [key: string]: unknown;
  relation: 'contains' | 'realizes_event' | 'foreshadows';
}

// ---------------------------------------------------------------------------
// Layout constants — deterministic lane/layered grid (World KB pattern)
// ---------------------------------------------------------------------------

const ORIGIN_X = 40;
const ORIGIN_Y = 40;
const LANE_X = 320;
const ROW_Y = 120;

/** Column offset per lane kind (volumes → chapters → timeline). */
const VOLUME_LANE = 0;
const CHAPTER_LANE = 1;
const TIMELINE_LANE = 2;

// ---------------------------------------------------------------------------
// Stable node ids (prefixed by kind so they never collide)
// ---------------------------------------------------------------------------

export function volumeNodeId(volumeId: number): string {
  return `volume:${volumeId}`;
}
export function chapterNodeId(chapterId: number): string {
  return `chapter:${chapterId}`;
}
export function eventNodeId(eventId: string): string {
  return `event:${eventId}`;
}

// ---------------------------------------------------------------------------
// Projection: outline + chapters → { nodes, edges }
// ---------------------------------------------------------------------------

export interface OutlineGraphProjection {
  nodes: Node[];
  edges: Edge[];
}

/**
 * Project a full outline into React Flow nodes + edges.
 *
 * Deterministic: the same `(outline, chapters)` pair always yields identical
 * node positions, ids, and edge sets. Inputs are never mutated.
 */
export function projectOutlineGraph(
  outline: WorkOutline,
  chapters: ChapterSummary[],
): OutlineGraphProjection {
  const volumeNodes = layoutVolumeNodes(outline);
  const chapterNodes = layoutChapterNodes(outline, chapters);
  const eventNodes = layoutTimelineEventNodes(outline);

  const containsEdges = deriveContainsEdges(outline);
  const realizesEdges = deriveRealizesEdges(outline);
  const foreshadowEdges = deriveForeshadowEdges(outline);

  const allNodes = [...volumeNodes, ...chapterNodes, ...eventNodes];
  const allEdges = [...containsEdges, ...realizesEdges, ...foreshadowEdges];

  // I-QC1-002 — filter out dangling edges whose source or target node does not
  // exist. Chapter data is paginated (20/page) and the orchestrator auto-fetches
  // remaining pages, but this guard ensures the projection never emits a
  // dangling edge if a chapter referenced by the outline is absent from the
  // loaded pages or the chapters list entirely.
  const nodeIds = new Set(allNodes.map((n) => n.id));
  const edges = allEdges.filter(
    (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
  );

  return { nodes: allNodes, edges };
}

// ---------------------------------------------------------------------------
// Node layout
// ---------------------------------------------------------------------------

/** Volume lane nodes — one per volume, stacked top-to-bottom. */
export function layoutVolumeNodes(outline: WorkOutline): Node<OutlineVolumeNodeData>[] {
  return outline.volumes.map((volume, index) => {
    const data: OutlineVolumeNodeData = {
      workId: outline.work_id,
      volumeId: volume.volume_id,
      label: volume.label,
      chapterCount: volume.chapter_ids.length,
    };
    return {
      id: volumeNodeId(volume.volume_id),
      type: 'outline-volume',
      position: { x: ORIGIN_X + VOLUME_LANE * LANE_X, y: ORIGIN_Y + index * ROW_Y },
      data,
    };
  });
}

/**
 * Chapter lane nodes — ordered by volume declaration then chapter number.
 *
 * Unassigned chapters (no volume claims them) are appended after all assigned
 * chapters so the canvas mirrors the panel's "Unassigned" bucket conceptually.
 */
export function layoutChapterNodes(
  outline: WorkOutline,
  chapters: ChapterSummary[],
): Node<OutlineChapterNodeData>[] {
  const chapterById = new Map<number, ChapterSummary>();
  for (const c of chapters) chapterById.set(c.chapter, c);

  const assignedIds = new Set<number>();
  const ordered: { chapter: ChapterSummary; volumeId: number | null }[] = [];

  // Assigned chapters in volume-declaration order.
  for (const volume of outline.volumes) {
    for (const chapterId of volume.chapter_ids) {
      const c = chapterById.get(chapterId);
      if (c) {
        ordered.push({ chapter: c, volumeId: volume.volume_id });
        assignedIds.add(chapterId);
      }
    }
  }

  // Unassigned chapters (sorted by chapter number for stability).
  // Copy before sort — never mutate the caller's array (purity contract).
  for (const c of [...chapters].sort((a, b) => a.chapter - b.chapter)) {
    if (!assignedIds.has(c.chapter)) {
      ordered.push({ chapter: c, volumeId: null });
    }
  }

  return ordered.map(({ chapter, volumeId }, index) => {
    const data: OutlineChapterNodeData = {
      workId: outline.work_id,
      chapterId: chapter.chapter,
      volumeId,
      title: chapterDisplayTitle(chapter, outline.chapter_titles as Record<string, string> | undefined),
      slug: chapter.slug ?? null,
      status: chapter.status,
      plannedWordCount: chapter.planned_word_count,
      actualWordCount: chapter.actual_word_count ?? null,
    };
    return {
      id: chapterNodeId(chapter.chapter),
      type: 'outline-chapter',
      position: { x: ORIGIN_X + CHAPTER_LANE * LANE_X, y: ORIGIN_Y + index * ROW_Y },
      data,
    };
  });
}

/** Timeline event lane nodes — one per event, in declaration order. */
export function layoutTimelineEventNodes(outline: WorkOutline): Node<OutlineTimelineEventNodeData>[] {
  return outline.timeline_events.map((event, index) => {
    const data: OutlineTimelineEventNodeData = {
      workId: outline.work_id,
      eventId: event.event_id,
      title: event.title,
      description: event.description ?? null,
      realizesChapterId: event.realizes_chapter_id ?? null,
    };
    return {
      id: eventNodeId(event.event_id),
      type: 'outline-timeline-event',
      position: { x: ORIGIN_X + TIMELINE_LANE * LANE_X, y: ORIGIN_Y + index * ROW_Y },
      data,
    };
  });
}

// ---------------------------------------------------------------------------
// Edge derivation
// ---------------------------------------------------------------------------

/** Volume → Chapter containment edges (from `outline.volumes[].chapter_ids`). */
export function deriveContainsEdges(outline: WorkOutline): Edge<OutlineEdgeData>[] {
  const edges: Edge<OutlineEdgeData>[] = [];
  for (const volume of outline.volumes) {
    for (const chapterId of volume.chapter_ids) {
      const data: OutlineEdgeData = { relation: 'contains' };
      edges.push({
        id: `contains:${volume.volume_id}:${chapterId}`,
        source: volumeNodeId(volume.volume_id),
        target: chapterNodeId(chapterId),
        type: 'smoothstep',
        data,
      });
    }
  }
  return edges;
}

/** Chapter → Timeline Event realization edges (when `realizes_chapter_id` set). */
export function deriveRealizesEdges(outline: WorkOutline): Edge<OutlineEdgeData>[] {
  const edges: Edge<OutlineEdgeData>[] = [];
  for (const event of outline.timeline_events) {
    if (event.realizes_chapter_id === undefined || event.realizes_chapter_id === null) continue;
    const data: OutlineEdgeData = { relation: 'realizes_event' };
    edges.push({
      id: `realizes:${event.event_id}`,
      source: chapterNodeId(event.realizes_chapter_id),
      target: eventNodeId(event.event_id),
      type: 'smoothstep',
      data,
    });
  }
  return edges;
}

/** Event → Event foreshadow edges (from `outline.foreshadows[]`). */
export function deriveForeshadowEdges(outline: WorkOutline): Edge<OutlineEdgeData>[] {
  return outline.foreshadows.map((link) => {
    const data: OutlineEdgeData = { relation: 'foreshadows' };
    return {
      id: `foreshadow:${link.source_event_id}:${link.target_event_id}`,
      source: eventNodeId(link.source_event_id),
      target: eventNodeId(link.target_event_id),
      type: 'straight',
      data,
      // Foreshadow edges consume the canvas-outline-foreshadow-edge token
      // (FB-C1-006). Not selectable in the T1 minimum; focusable for a11y.
      selectable: false,
      focusable: true,
      style: { stroke: 'var(--color-canvas-outline-foreshadow-edge)' },
    };
  });
}

// ---------------------------------------------------------------------------
// Selection resolution (graph click → inspector entity)
// ---------------------------------------------------------------------------

/**
 * Resolve the selected chapter id from React Flow node selection state.
 *
 * When a user clicks a graph node, React Flow sets `n.selected = true` on the
 * clicked node and clears other selections via `onNodesChange`. This helper
 * maps that selection to an outline chapter id so the orchestrator can drive
 * the chapter inspector from graph clicks (FB-C1-003), mirroring the
 * `world-kb-canvas.tsx` graph-click → inspector pattern.
 *
 *   • outline-chapter node        → its `chapterId`
 *   • outline-timeline-event node → its `realizesChapterId` (may be `null`)
 *   • outline-volume node         → `null` (structural; no chapter selection)
 *   • no selection                → `null`
 *
 * Returns `null` when the selection does not resolve to a chapter. Callers
 * must treat `null` as "do not update `selectedChapterId`" so clicking an
 * unattached event or a volume node does not clear the current inspector
 * selection.
 */
export function selectedChapterIdFromNodes(nodes: Node[]): number | null {
  const selected = nodes.find((n) => n.selected);
  if (!selected) return null;
  if (selected.type === 'outline-chapter') {
    return (selected.data as OutlineChapterNodeData).chapterId;
  }
  if (selected.type === 'outline-timeline-event') {
    return (selected.data as OutlineTimelineEventNodeData).realizesChapterId;
  }
  return null;
}

// ---------------------------------------------------------------------------
// SR summary helper
// ---------------------------------------------------------------------------

/**
 * Human-readable graph summary for the canvas screen-reader region.
 * Mirrors the World KB `graphSummary` helper.
 */
export function outlineGraphSummary(
  outline: WorkOutline | undefined,
  chapterCount: number,
): string {
  if (!outline) return 'Outline graph not loaded.';
  const volumeCount = outline.volumes.length;
  const eventCount = outline.timeline_events.length;
  const foreshadowCount = outline.foreshadows.length;
  return [
    `Outline graph: ${volumeCount} ${volumeCount === 1 ? 'volume' : 'volumes'},`,
    `${chapterCount} ${chapterCount === 1 ? 'chapter' : 'chapters'},`,
    `${eventCount} timeline ${eventCount === 1 ? 'event' : 'events'},`,
    `${foreshadowCount} ${foreshadowCount === 1 ? 'foreshadow link' : 'foreshadow links'}.`,
  ].join(' ');
}
