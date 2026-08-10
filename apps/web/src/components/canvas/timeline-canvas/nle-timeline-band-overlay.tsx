/**
 * NLE Timeline band overlay — thin App host for `@web-canvas/nle-timeline-chrome`.
 *
 * Renders the presentational multi-track band centered over the existing React
 * Flow canvas. Pull-off affordances are intentionally omitted (Studio-only T2).
 * No new RF node types or DnD handlers — entity nodes remain selectable below.
 */
import { useMemo } from 'react';

import { NleTimelineChrome } from '../presentational/nle-timeline-chrome';
import type { Node } from '@xyflow/react';

import {
  projectWorkTimelineNodesToNleTracks,
  projectWorldTimelineNodesToNleTracks,
} from './nle-timeline-projection';

export type NleTimelineBandOverlayProps = {
  nodes: Node[];
  surface: 'world' | 'work';
  activeLayer: 'brief' | 'narrative' | 'moment';
  scrollAriaLabel: string;
};

export function NleTimelineBandOverlay({
  nodes,
  surface,
  activeLayer,
  scrollAriaLabel,
}: NleTimelineBandOverlayProps) {
  const { tracks, contentWidthPx } = useMemo(() => {
    if (surface === 'work') {
      // V1.156 P2 T2 — Work-Brief NLE band. Work-Brief projects the World
      // Timeline Brief nodes verbatim (`timeline-brief-era`), so the band
      // mirrors the World Timeline Brief tracks (dated / undated eras).
      if (activeLayer === 'brief') {
        return projectWorkTimelineNodesToNleTracks(nodes, 'brief');
      }
      if (activeLayer === 'moment') {
        return projectWorkTimelineNodesToNleTracks(nodes, 'moment');
      }
      return projectWorkTimelineNodesToNleTracks(nodes, 'narrative');
    }
    if (activeLayer === 'brief') {
      return projectWorldTimelineNodesToNleTracks(nodes, 'brief');
    }
    if (activeLayer === 'moment') {
      // V1.156 P5 — World-Moment NLE band. The World Timeline reuses the Work
      // Timeline Moment node types verbatim, so the World-Moment band
      // projects Scenes/Beats tracks instead of falling through to the
      // Narrative projection (which yields zero tracks for Moment nodes).
      return projectWorldTimelineNodesToNleTracks(nodes, 'moment');
    }
    return projectWorldTimelineNodesToNleTracks(nodes, 'narrative');
  }, [activeLayer, nodes, surface]);

  if (tracks.length === 0) {
    return null;
  }

  return (
    <div
      className="pointer-events-none absolute inset-x-0 top-1/2 z-[5] -translate-y-1/2 px-4"
      data-testid="nle-timeline-band-overlay"
    >
      {/* Display-only in App: keep pointer-events-none so RF pan/zoom/select
          reach nodes under the centered band. Horizontal scrub stays Studio. */}
      <div className="pointer-events-none mx-auto max-w-full">
        <NleTimelineChrome
          tracks={tracks}
          contentWidthPx={contentWidthPx}
          scrollAriaLabel={scrollAriaLabel}
          className="min-h-0 bg-transparent p-0"
          data-testid="nle-timeline-chrome-app"
        />
      </div>
    </div>
  );
}
