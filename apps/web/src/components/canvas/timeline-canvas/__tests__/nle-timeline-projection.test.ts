/**
 * NLE Timeline projection — unit tests (V1.128 P1 T3).
 */
import { describe, expect, it } from 'vitest';
import type { Node } from '@xyflow/react';

import {
  filterTimelineEntityNodes,
  projectWorldTimelineNodesToNleTracks,
  projectWorkTimelineNodesToNleTracks,
} from '../nle-timeline-projection';

function node(
  id: string,
  type: string,
  x: number,
  y: number,
  data: Record<string, unknown> = {},
): Node {
  return {
    id,
    type,
    position: { x, y },
    data,
  };
}

describe('projectWorldTimelineNodesToNleTracks', () => {
  it('maps Brief-era nodes to Brief + Undated tracks', () => {
    const nodes = [
      node('era-1', 'timeline-brief-era', 40, 0, { canonical_name: 'First Age' }),
      node('era-2', 'timeline-brief-era', 360, 220, { canonical_name: 'Undated Era' }),
      node('spine', 'directedAxisSpine', 0, -8, {}),
    ];

    const { tracks } = projectWorldTimelineNodesToNleTracks(nodes, 'brief');
    expect(tracks).toHaveLength(2);
    expect(tracks[0]?.label).toBe('Brief');
    expect(tracks[0]?.clips[0]?.label).toBe('First Age');
    expect(tracks[1]?.label).toBe('Undated');
    expect(tracks[1]?.clips[0]?.label).toBe('Undated Era');
  });

  it('maps Narrative events and Context entities to separate tracks', () => {
    const nodes = [
      node('ev-1', 'timeline-event', 120, 0, { canonical_name: 'The Crossing' }),
      node('ev-2', 'timeline-event', 400, 220, { canonical_name: 'Undated Event' }),
      node('kb-1', 'timeline-key-block', 80, -220, { canonical_name: 'Kael' }),
    ];

    const { tracks } = projectWorldTimelineNodesToNleTracks(nodes, 'narrative');
    expect(tracks.map((track) => track.label)).toEqual([
      'Narrative',
      'Undated',
      'Context',
    ]);
  });
});

describe('projectWorkTimelineNodesToNleTracks', () => {
  it('maps Work-Brief era nodes to Brief + Undated tracks (V1.156 P2 T2)', () => {
    // Work-Brief projects the World Timeline Brief nodes verbatim
    // (`timeline-brief-era`) — the band must mirror the World Timeline
    // Brief tracks (dated eras on the when-axis + undated cluster).
    const nodes = [
      node('era-1', 'timeline-brief-era', 40, 0, { canonical_name: 'First Age' }),
      node('era-2', 'timeline-brief-era', 360, 220, { canonical_name: 'Undated Era' }),
      node('spine', 'directedAxisSpine', 0, -8, {}),
    ];

    const { tracks } = projectWorkTimelineNodesToNleTracks(nodes, 'brief');
    expect(tracks).toHaveLength(2);
    expect(tracks[0]?.label).toBe('Brief');
    expect(tracks[0]?.clips[0]?.label).toBe('First Age');
    expect(tracks[1]?.label).toBe('Undated');
    expect(tracks[1]?.clips[0]?.label).toBe('Undated Era');
  });

  it('maps narrative events to a single Narrative track when all share anchor state', () => {
    const nodes = [
      node('wt-ev-1', 'work-timeline-narrative-event', 40, 0, {
        label: 'Inciting',
        realizesChapterId: 1,
      }),
      node('wt-ev-2', 'work-timeline-narrative-event', 320, 0, {
        label: 'Midpoint',
        realizesChapterId: 2,
      }),
    ];

    const { tracks } = projectWorkTimelineNodesToNleTracks(nodes, 'narrative');
    expect(tracks).toHaveLength(1);
    expect(tracks[0]?.label).toBe('Narrative');
    expect(tracks[0]?.clips).toHaveLength(2);
  });

  it('maps Moment scenes and beats to separate tracks', () => {
    const nodes = [
      node('sc-1', 'work-timeline-moment-scene', 40, 0, { label: 'Opening Scene' }),
      node('bt-1', 'work-timeline-moment-beat', 56, 120, { label: 'Hook Beat' }),
    ];

    const { tracks } = projectWorkTimelineNodesToNleTracks(nodes, 'moment');
    expect(tracks.map((track) => track.label)).toEqual(['Scenes', 'Beats']);
  });
});

describe('filterTimelineEntityNodes', () => {
  it('removes directed-axis spine decoration nodes', () => {
    const nodes = [
      node('ev-1', 'timeline-event', 0, 0, {}),
      node('spine', 'directedAxisSpine', 0, -8, {}),
    ];
    expect(filterTimelineEntityNodes(nodes).map((n) => n.id)).toEqual(['ev-1']);
  });
});
