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
      if (activeLayer === 'moment') {
        return projectWorkTimelineNodesToNleTracks(nodes, 'moment');
      }
      return projectWorkTimelineNodesToNleTracks(nodes, 'narrative');
    }
    if (activeLayer === 'brief') {
      return projectWorldTimelineNodesToNleTracks(nodes, 'brief');
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
