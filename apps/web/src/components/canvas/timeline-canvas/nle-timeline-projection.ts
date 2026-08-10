/**
 * NLE Timeline projection — maps React Flow entity nodes to presentational
 * multi-track lanes for the V1.128 App adoption overlay.
 *
 * App-local glue (not `@web-canvas/*`): reads RF node positions/types from the
 * existing timeline adapters without changing projection or wire contracts.
 * The directed-axis spine decoration node is excluded — the NLE band replaces
 * its visual role in the canvas host.
 */
import type { Node } from '@xyflow/react';

import type {
  NleTimelineClip,
  NleTimelineTrack,
  NleTimelineTrackAccent,
} from '../presentational/nle-timeline-chrome';

const DEFAULT_CLIP_WIDTH_PX = 160;
const CONTENT_PADDING_PX = 120;
const MIN_CONTENT_WIDTH_PX = 800;

/** World Timeline when-axis Y (dated entities). */
const WORLD_WHEN_AXIS_Y = 0;

type TrackRule = {
  id: string;
  label: string;
  accent: NleTimelineTrackAccent;
  match: (node: Node) => boolean;
};

function clipLabel(node: Node): string {
  const data = node.data as Record<string, unknown>;
  if (typeof data.canonical_name === 'string' && data.canonical_name.length > 0) {
    return data.canonical_name;
  }
  if (typeof data.label === 'string' && data.label.length > 0) {
    return data.label;
  }
  return node.id;
}

function nodeToClip(node: Node, widthPx = DEFAULT_CLIP_WIDTH_PX): NleTimelineClip {
  return {
    id: node.id,
    label: clipLabel(node),
    startPx: Math.max(0, node.position.x),
    widthPx,
  };
}

function projectWithRules(
  nodes: Node[],
  rules: TrackRule[],
): { tracks: NleTimelineTrack[]; contentWidthPx: number } {
  const entityNodes = nodes.filter((node) => node.type !== 'directedAxisSpine');
  const tracks: NleTimelineTrack[] = [];

  for (const rule of rules) {
    const clips = entityNodes.filter(rule.match).map((node) => nodeToClip(node));
    if (clips.length > 0) {
      tracks.push({
        id: rule.id,
        label: rule.label,
        accent: rule.accent,
        clips,
      });
    }
  }

  let maxX = 0;
  for (const track of tracks) {
    for (const clip of track.clips) {
      maxX = Math.max(maxX, clip.startPx + clip.widthPx);
    }
  }

  return {
    tracks,
    contentWidthPx: Math.max(MIN_CONTENT_WIDTH_PX, maxX + CONTENT_PADDING_PX),
  };
}

/** World Timeline — Brief ↔ Narrative layer tracks from adapter nodes. */
export function projectWorldTimelineNodesToNleTracks(
  nodes: Node[],
  layer: 'brief' | 'narrative',
): { tracks: NleTimelineTrack[]; contentWidthPx: number } {
  if (layer === 'brief') {
    return projectWithRules(nodes, [
      {
        id: 'brief',
        label: 'Brief',
        accent: 'brief',
        match: (node) =>
          node.type === 'timeline-brief-era' && node.position.y === WORLD_WHEN_AXIS_Y,
      },
      {
        id: 'brief-undated',
        label: 'Undated',
        accent: 'brief',
        match: (node) =>
          node.type === 'timeline-brief-era' && node.position.y !== WORLD_WHEN_AXIS_Y,
      },
    ]);
  }

  return projectWithRules(nodes, [
    {
      id: 'narrative',
      label: 'Narrative',
      accent: 'narrative',
      match: (node) =>
        (node.type === 'timeline-event' || node.type === 'timeline-compute-result') &&
        node.position.y === WORLD_WHEN_AXIS_Y,
    },
    {
      id: 'undated',
      label: 'Undated',
      accent: 'narrative',
      match: (node) =>
        (node.type === 'timeline-event' || node.type === 'timeline-compute-result') &&
        node.position.y !== WORLD_WHEN_AXIS_Y,
    },
    {
      id: 'context',
      label: 'Context',
      accent: 'moment',
      match: (node) => node.type === 'timeline-key-block',
    },
  ]);
}

/**
 * Work Timeline — Brief | Narrative | Moment layer tracks from adapter nodes.
 *
 * V1.156 P2 T2 — the Brief layer reuses the World Timeline Brief projection
 * verbatim (`timeline-brief-era` nodes), so the Brief band mirrors the World
 * Timeline Brief tracks (dated eras on the when-axis + undated cluster).
 */
export function projectWorkTimelineNodesToNleTracks(
  nodes: Node[],
  layer: 'brief' | 'narrative' | 'moment',
): { tracks: NleTimelineTrack[]; contentWidthPx: number } {
  if (layer === 'brief') {
    return projectWithRules(nodes, [
      {
        id: 'brief',
        label: 'Brief',
        accent: 'brief',
        match: (node) =>
          node.type === 'timeline-brief-era' && node.position.y === WORLD_WHEN_AXIS_Y,
      },
      {
        id: 'brief-undated',
        label: 'Undated',
        accent: 'brief',
        match: (node) =>
          node.type === 'timeline-brief-era' && node.position.y !== WORLD_WHEN_AXIS_Y,
      },
    ]);
  }
  if (layer === 'narrative') {
    const split = projectWithRules(nodes, [
      {
        id: 'anchored',
        label: 'Chapter-anchored',
        accent: 'narrative',
        match: (node) =>
          node.type === 'work-timeline-narrative-event' &&
          (node.data as { realizesChapterId?: number }).realizesChapterId != null,
      },
      {
        id: 'unanchored',
        label: 'Unanchored',
        accent: 'narrative',
        match: (node) =>
          node.type === 'work-timeline-narrative-event' &&
          (node.data as { realizesChapterId?: number }).realizesChapterId == null,
      },
    ]);

    if (split.tracks.length >= 2) {
      return split;
    }

    return projectWithRules(nodes, [
      {
        id: 'narrative',
        label: 'Narrative',
        accent: 'narrative',
        match: (node) => node.type === 'work-timeline-narrative-event',
      },
    ]);
  }

  return projectWithRules(nodes, [
    {
      id: 'scenes',
      label: 'Scenes',
      accent: 'moment',
      match: (node) => node.type === 'work-timeline-moment-scene',
    },
    {
      id: 'beats',
      label: 'Beats',
      accent: 'moment',
      match: (node) => node.type === 'work-timeline-moment-beat',
    },
  ]);
}

/** Strip decoration spine nodes before passing entity nodes to React Flow. */
export function filterTimelineEntityNodes(nodes: Node[]): Node[] {
  return nodes.filter((node) => node.type !== 'directedAxisSpine');
}
