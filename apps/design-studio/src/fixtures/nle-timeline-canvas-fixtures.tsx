/**
 * Studio fixtures for NLE multi-track Timeline chrome (V1.128 P1 T1).
 *
 * Composes `@web-canvas/nle-timeline-chrome` — the same presentational extract
 * App Timeline hosts will adopt in T3. Demonstrates vertically centered band,
 * ≥2 labeled tracks, horizontal scrub/pan, and light/dark theme acceptance.
 *
 * Boundary: no `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 * no `useTranslation`. Static English product vocabulary only.
 */
import { type ReactNode } from 'react';

import {
  NLE_TIMELINE_DEMO_TRACKS,
  NleTimelineChrome,
} from '@web-canvas/nle-timeline-chrome'; // @web-canvas/nle-timeline-chrome - transitional until package promotion criteria met

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
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * NLE Timeline fixtures — multi-track band for AC-V1128-2a visual acceptance.
 */
export function NleTimelineCanvasFixtures() {
  return (
    <div data-testid="nle-timeline-canvas-fixtures">
      <NleTimelineBandFixtureFrame />
    </div>
  );
}
