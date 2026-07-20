/**
 * Studio fixtures for NLE multi-track Timeline chrome (V1.128 P1 T1/T2).
 *
 * Composes `@web-canvas/nle-timeline-chrome` — the same presentational extract
 * App Timeline hosts will adopt in T3. Demonstrates vertically centered band,
 * ≥2 labeled tracks, horizontal scrub/pan, pull-off affordance (T2 local
 * state only), and light/dark theme acceptance.
 *
 * Boundary: no `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 * no `useTranslation`. Static English product vocabulary only.
 * Pull-off interaction is fixture state only — does not ship in App (T3).
 */
import { useMemo, useState, type ReactNode } from 'react';

import {
  NLE_TIMELINE_DEMO_TRACKS,
  NleTimelineChrome,
  type NleTimelineClip,
} from '@web-canvas/nle-timeline-chrome'; // @web-canvas/nle-timeline-chrome - transitional until package promotion criteria met

const PULL_OFF_DEMO_CLIP_ID = 'ev-1';

/* ------------------------------------------------------------------ */
/*  Shared fixture frame                                                */
/* ------------------------------------------------------------------ */

function FixtureFrame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  NLE Timeline band — multi-track + horizontal scrub                    */
/* ------------------------------------------------------------------ */

function NleTimelineBandFixtureFrame() {
  return (
    <FixtureFrame
      title="NLE Timeline — multi-track band"
      description="Vertically centered Timeline band with Brief, Narrative, and Moment lanes. Pan horizontally along the shared time ruler to scrub across eras, events, and scenes. Composes NleTimelineChrome from the shared extract — no React Flow, no daemon data."
      testId="nle-timeline-fixture-band"
    >
      <div
        className="overflow-hidden rounded-card border border-gray-alpha-300"
        data-testid="nle-timeline-fixture-host"
      >
        <NleTimelineChrome
          tracks={NLE_TIMELINE_DEMO_TRACKS}
          contentWidthPx={1600}
          playheadPx={420}
          scrollAriaLabel="NLE Timeline scrub — pan horizontally along time"
        />
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Pull-off affordance — Studio local state (AC-V1128-2b)                */
/* ------------------------------------------------------------------ */

function NleTimelinePullOffFixtureFrame() {
  const [detachedClip, setDetachedClip] = useState<NleTimelineClip | null>(null);
  const [detachedTrackLabel, setDetachedTrackLabel] = useState<string | null>(null);

  const tracks = useMemo(() => {
    if (!detachedClip) {
      return NLE_TIMELINE_DEMO_TRACKS;
    }
    return NLE_TIMELINE_DEMO_TRACKS.map((track) => ({
      ...track,
      clips: track.clips.filter((clip) => clip.id !== detachedClip.id),
    }));
  }, [detachedClip]);

  const detachableClipIds = detachedClip
    ? undefined
    : new Set([PULL_OFF_DEMO_CLIP_ID]);

  const handleDetach = (trackId: string, clip: NleTimelineClip) => {
    const trackLabel =
      NLE_TIMELINE_DEMO_TRACKS.find((track) => track.id === trackId)?.label ??
      trackId;
    setDetachedTrackLabel(trackLabel);
    setDetachedClip(clip);
  };

  const handleReset = () => {
    setDetachedClip(null);
    setDetachedTrackLabel(null);
  };

  return (
    <FixtureFrame
      title="NLE Timeline — pull-off affordance"
      description='Detach "The Crossing" from the Narrative lane onto the canvas area above. Pull-off uses fixture-local React state only — no React Flow DnD, no persistence, and this interaction does not ship in App Timeline (V1.128 T3).'
      testId="nle-timeline-fixture-pull-off"
    >
      <div
        className="flex flex-col overflow-hidden rounded-card border border-gray-alpha-300"
        data-testid="nle-timeline-pull-off-demo"
      >
        <div
          className="relative min-h-[180px] flex-1 border-b border-gray-alpha-300 bg-canvas-surface p-4"
          data-testid="nle-pull-off-canvas"
        >
          <p className="text-label-12 text-gray-500">
            Canvas area — detached timeline items land here
          </p>
          {detachedClip ? (
            <div
              className="absolute left-16 top-14 max-w-xs rounded-card border border-canvas-layer-narrative-accent/60 bg-canvas-node-fill px-4 py-3 shadow-elevation-2"
              data-testid="nle-pull-off-detached-item"
            >
              <p className="text-copy-13 font-medium text-gray-1000">
                {detachedClip.label}
              </p>
              <p className="mt-1 text-label-12 text-canvas-layer-narrative-accent">
                Detached from {detachedTrackLabel} lane
              </p>
            </div>
          ) : (
            <p
              className="mt-6 text-copy-13 text-gray-700"
              data-testid="nle-pull-off-canvas-hint"
            >
              Use Detach on &quot;The Crossing&quot; in the Narrative lane below.
            </p>
          )}
        </div>

        <NleTimelineChrome
          tracks={tracks}
          contentWidthPx={1600}
          playheadPx={420}
          scrollAriaLabel="NLE Timeline scrub — pull-off demo"
          detachableClipIds={detachableClipIds}
          onClipDetach={handleDetach}
          className="min-h-[220px] shrink-0 p-2"
          data-testid="nle-timeline-chrome-pull-off"
        />

        {detachedClip ? (
          <div className="border-t border-gray-alpha-200 bg-background-100 px-4 py-2">
            <button
              type="button"
              className="rounded-md px-3 py-1.5 text-label-14 font-medium text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
              onClick={handleReset}
              data-testid="nle-pull-off-reset"
            >
              Reset demo
            </button>
          </div>
        ) : null}
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * NLE Timeline fixtures — multi-track band + pull-off for AC-V1128-2a/2b.
 */
export function NleTimelineCanvasFixtures() {
  return (
    <div data-testid="nle-timeline-canvas-fixtures">
      <NleTimelineBandFixtureFrame />
      <NleTimelinePullOffFixtureFrame />
    </div>
  );
}
