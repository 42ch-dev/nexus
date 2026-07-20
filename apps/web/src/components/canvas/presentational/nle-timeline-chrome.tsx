/**
 * NleTimelineChrome — presentational NLE-style multi-track Timeline band.
 *
 * Renders a vertically centered, horizontally scrollable multi-lane timeline
 * for dogfood visual verification in Design Studio and thin App adoption
 * (V1.128 P1). Track labels sit in a fixed left column; ruler + clip lanes
 * share one horizontal scroll region for scrub/pan along the time axis.
 *
 * Presentational boundary (architect lock, V1.128 P1):
 *   This module MUST NOT import the React Flow package. App RF hosts swap this
 *   chrome into existing Timeline surfaces; Studio fixtures consume it via
 *   `@web-canvas/nle-timeline-chrome`.
 *
 * Tokens: `canvas-surface`, `canvas-layer-*-accent`, `canvas-node-fill`,
 * `background-100`, `gray-alpha-*` (existing DESIGN palette — no new hues).
 */
import type { CSSProperties } from 'react';

import { cn } from '@/lib/utils';

/** Layer accent driving track label + clip chrome. */
export type NleTimelineTrackAccent = 'brief' | 'narrative' | 'moment';

/** One clip block positioned along the horizontal time axis (px offsets). */
export type NleTimelineClip = {
  id: string;
  label: string;
  /** Distance from the timeline origin in pixels. */
  startPx: number;
  /** Clip width in pixels. */
  widthPx: number;
};

/** One labeled track lane with positioned clips. */
export type NleTimelineTrack = {
  id: string;
  label: string;
  accent: NleTimelineTrackAccent;
  clips: NleTimelineClip[];
};

/** Tick mark on the time ruler. */
export type NleTimelineRulerTick = {
  positionPx: number;
  label: string;
};

export type NleTimelineChromeProps = {
  tracks: NleTimelineTrack[];
  /** Total scrollable width of the time axis in pixels. */
  contentWidthPx?: number;
  /** Lane height per track in pixels. */
  laneHeightPx?: number;
  /** Optional playhead position along the time axis (px). */
  playheadPx?: number;
  /** Ruler tick marks; defaults to evenly spaced markers when omitted. */
  rulerTicks?: NleTimelineRulerTick[];
  /** Accessible label for the horizontal scrub region. */
  scrollAriaLabel?: string;
  className?: string;
  'data-testid'?: string;
};

const TRACK_ACCENT_LABEL: Record<NleTimelineTrackAccent, string> = {
  brief: 'text-canvas-layer-brief-accent',
  narrative: 'text-canvas-layer-narrative-accent',
  moment: 'text-canvas-layer-moment-accent',
};

const TRACK_ACCENT_CLIP_BORDER: Record<NleTimelineTrackAccent, string> = {
  brief: 'border-canvas-layer-brief-accent/60',
  narrative: 'border-canvas-layer-narrative-accent/60',
  moment: 'border-canvas-layer-moment-accent/60',
};

const TRACK_ACCENT_CLIP_FILL: Record<NleTimelineTrackAccent, string> = {
  brief: 'bg-canvas-layer-brief-accent/10',
  narrative: 'bg-canvas-layer-narrative-accent/10',
  moment: 'bg-canvas-layer-moment-accent/10',
};

const DEFAULT_CONTENT_WIDTH = 1600;
const DEFAULT_LANE_HEIGHT = 44;
const RULER_HEIGHT = 28;
const LABEL_COLUMN_WIDTH = 112;

function defaultRulerTicks(contentWidthPx: number): NleTimelineRulerTick[] {
  const step = 200;
  const ticks: NleTimelineRulerTick[] = [];
  for (let px = 0; px <= contentWidthPx; px += step) {
    ticks.push({ positionPx: px, label: `T${px / step}` });
  }
  return ticks;
}

function NleTimelineRuler({
  ticks,
  contentWidthPx,
}: {
  ticks: NleTimelineRulerTick[];
  contentWidthPx: number;
}) {
  return (
    <div
      className="relative shrink-0 border-b border-gray-alpha-200 bg-background-100"
      style={{ width: contentWidthPx, height: RULER_HEIGHT }}
      data-testid="nle-timeline-ruler"
    >
      {ticks.map((tick) => (
        <div
          key={`${tick.positionPx}-${tick.label}`}
          className="absolute top-0 flex flex-col items-center"
          style={{ left: tick.positionPx }}
        >
          <div className="h-2 w-px bg-gray-alpha-400" />
          <span className="mt-0.5 text-label-12 text-gray-500">{tick.label}</span>
        </div>
      ))}
    </div>
  );
}

function NleTimelineClipBlock({
  clip,
  accent,
  laneHeightPx,
}: {
  clip: NleTimelineClip;
  accent: NleTimelineTrackAccent;
  laneHeightPx: number;
}) {
  const style: CSSProperties = {
    left: clip.startPx,
    width: clip.widthPx,
    top: 6,
    height: laneHeightPx - 12,
  };

  return (
    <div
      className={cn(
        'absolute flex items-center overflow-hidden rounded-md border px-2',
        'bg-canvas-node-fill text-copy-13 text-gray-1000 shadow-card',
        TRACK_ACCENT_CLIP_BORDER[accent],
        TRACK_ACCENT_CLIP_FILL[accent],
      )}
      style={style}
      data-testid={`nle-timeline-clip-${clip.id}`}
      title={clip.label}
    >
      <span className="truncate">{clip.label}</span>
    </div>
  );
}

function NleTimelineLane({
  track,
  contentWidthPx,
  laneHeightPx,
}: {
  track: NleTimelineTrack;
  contentWidthPx: number;
  laneHeightPx: number;
}) {
  return (
    <div
      className="relative shrink-0 border-b border-gray-alpha-200 bg-canvas-surface/50 last:border-b-0"
      style={{ width: contentWidthPx, height: laneHeightPx }}
      data-testid={`nle-timeline-lane-${track.id}`}
    >
      {track.clips.map((clip) => (
        <NleTimelineClipBlock
          key={clip.id}
          clip={clip}
          accent={track.accent}
          laneHeightPx={laneHeightPx}
        />
      ))}
    </div>
  );
}

function NleTimelinePlayhead({ positionPx }: { positionPx: number }) {
  return (
    <div
      className="pointer-events-none absolute top-0 z-10 w-px bg-red-600"
      style={{ left: positionPx, height: '100%' }}
      data-testid="nle-timeline-playhead"
      aria-hidden
    >
      <div className="absolute -left-1 top-0 h-2 w-2 rounded-full bg-red-600" />
    </div>
  );
}

/**
 * Vertically centered NLE multi-track Timeline band with horizontal scrub/pan.
 */
export function NleTimelineChrome({
  tracks,
  contentWidthPx = DEFAULT_CONTENT_WIDTH,
  laneHeightPx = DEFAULT_LANE_HEIGHT,
  playheadPx,
  rulerTicks,
  scrollAriaLabel = 'Timeline scrub area',
  className,
  'data-testid': dataTestId = 'nle-timeline-chrome',
}: NleTimelineChromeProps) {
  const ticks = rulerTicks ?? defaultRulerTicks(contentWidthPx);
  const bandHeight = RULER_HEIGHT + tracks.length * laneHeightPx;

  return (
    <div
      className={cn(
        'flex min-h-[280px] w-full items-center justify-center bg-canvas-surface p-4',
        className,
      )}
      data-testid={dataTestId}
    >
      <div
        className="flex w-full max-w-full flex-col overflow-hidden rounded-card border border-gray-alpha-200 bg-background-100 shadow-card"
        style={{ maxHeight: 'min(360px, 55vh)', height: bandHeight }}
        data-testid="nle-timeline-band"
      >
        <div className="flex min-h-0 flex-1">
          {/* Fixed track label column */}
          <div
            className="flex shrink-0 flex-col border-r border-gray-alpha-200 bg-background-100"
            style={{ width: LABEL_COLUMN_WIDTH }}
            data-testid="nle-timeline-labels"
          >
            <div
              className="shrink-0 border-b border-gray-alpha-200"
              style={{ height: RULER_HEIGHT }}
              aria-hidden
            />
            {tracks.map((track) => (
              <div
                key={track.id}
                className="flex items-center border-b border-gray-alpha-200 px-3 last:border-b-0"
                style={{ height: laneHeightPx }}
                data-testid={`nle-timeline-label-${track.id}`}
              >
                <span
                  className={cn(
                    'truncate text-label-12 font-medium',
                    TRACK_ACCENT_LABEL[track.accent],
                  )}
                >
                  {track.label}
                </span>
              </div>
            ))}
          </div>

          {/* Shared horizontal scroll region — ruler + lanes pan together */}
          <div
            className="min-w-0 flex-1 overflow-x-auto overflow-y-hidden focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2 focus-visible:ring-offset-background-100"
            data-testid="nle-timeline-scroll"
            tabIndex={0}
            role="region"
            aria-label={scrollAriaLabel}
          >
            <div className="relative" style={{ width: contentWidthPx }}>
              <NleTimelineRuler ticks={ticks} contentWidthPx={contentWidthPx} />
              {tracks.map((track) => (
                <NleTimelineLane
                  key={track.id}
                  track={track}
                  contentWidthPx={contentWidthPx}
                  laneHeightPx={laneHeightPx}
                />
              ))}
              {playheadPx != null ? (
                <NleTimelinePlayhead positionPx={playheadPx} />
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Static demo tracks for Studio fixtures and smoke tests. */
export const NLE_TIMELINE_DEMO_TRACKS: NleTimelineTrack[] = [
  {
    id: 'brief',
    label: 'Brief',
    accent: 'brief',
    clips: [
      { id: 'era-1', label: 'The First Age', startPx: 40, widthPx: 280 },
      { id: 'era-2', label: 'Age of Crossing', startPx: 360, widthPx: 320 },
      { id: 'era-3', label: 'Twilight Compact', startPx: 720, widthPx: 240 },
    ],
  },
  {
    id: 'narrative',
    label: 'Narrative',
    accent: 'narrative',
    clips: [
      { id: 'ev-1', label: 'The Crossing', startPx: 120, widthPx: 160 },
      { id: 'ev-2', label: 'Midpoint Reversal', startPx: 480, widthPx: 180 },
      { id: 'ev-3', label: 'Silent Accord', startPx: 880, widthPx: 140 },
      { id: 'ev-4', label: 'Final Stand', startPx: 1120, widthPx: 200 },
    ],
  },
  {
    id: 'moment',
    label: 'Moment',
    accent: 'moment',
    clips: [
      { id: 'sc-1', label: 'Opening Scene', startPx: 200, widthPx: 120 },
      { id: 'sc-2', label: 'Hook Beat', startPx: 340, widthPx: 100 },
      { id: 'sc-3', label: 'Climax Scene', startPx: 960, widthPx: 160 },
    ],
  },
];
