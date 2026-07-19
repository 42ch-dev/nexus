/**
 * Studio fixtures for Work Timeline node chrome (V1.124 P0 T4).
 *
 * Composes the same presentational extract App RF wrappers use:
 *   - `@web-canvas/node-chrome-shell` (card shell + spine accent)
 *   - `@web-canvas/timeline-node-chrome` (Narrative event / Moment scene / Moment beat)
 *
 * Boundary (studio-timeline-fixture-boundaries.md §4.4–§4.6 + F1–F9):
 *   No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 *   no `useTranslation`. Static English product vocabulary only.
 *   Layer breadcrumb is out of scope (P2).
 *
 * Spines: Narrative → `accent="worldkb"`; Moment scene/beat → `accent="outline"`.
 * Layer accents live inside the extract (Narrative → narrative-accent;
 * Moment → moment-accent). Moment = scene + beat (both frames required).
 */
import { type ReactNode } from 'react';

import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // @web-canvas/node-chrome-shell - transitional until package promotion criteria met
import {
  WorkTimelineMomentBeatChrome,
  WorkTimelineMomentSceneChrome,
  WorkTimelineNarrativeEventChrome,
} from '@web-canvas/timeline-node-chrome'; // @web-canvas/timeline-node-chrome - transitional until package promotion criteria met

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

function VariantChip({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-label-12 font-medium text-gray-500">{label}</span>
      {children}
    </div>
  );
}

function VariantMatrix({
  testId,
  children,
}: {
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="flex flex-wrap gap-6 rounded-card bg-canvas-surface p-6"
      data-testid={testId}
    >
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.4 Work Timeline — Narrative event                                */
/* ------------------------------------------------------------------ */

/**
 * Narrative event frame — Work-scoped event name + chapter-anchor badge
 * (or no-chapter pill) + optional description. Surface spine is worldkb;
 * layer accent is canvas-layer-narrative-accent on Flag + chapter badge.
 * Variants: with/without chapter anchor; with/without description;
 * selected/dragging (boundary §4.4 F3).
 */
function NarrativeEventFixtureFrame() {
  return (
    <FixtureFrame
      title="Work Timeline — Narrative event"
      description="Narrative layer event cards on the Work Timeline. Surface spine is worldkb; Flag icon and chapter-anchor badge use canvas-layer-narrative-accent. Composes NodeChromeShell + WorkTimelineNarrativeEventChrome from the shared extract — no Handles, no parallel badge CSS."
      testId="work-timeline-fixture-narrative-event"
    >
      <VariantMatrix testId="work-timeline-narrative-event-matrix">
        <VariantChip label="Chapter anchor + description">
          <NodeChromeShell accent="worldkb">
            <WorkTimelineNarrativeEventChrome
              title="The Crossing"
              eventId="ev-crossing"
              chapterAnchor="Ch. 3"
              noChapterLabel="No chapter anchor"
              description="Kael leaves the Hearthstone road and crosses into the Ashen Gate."
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="No chapter anchor">
          <NodeChromeShell accent="worldkb">
            <WorkTimelineNarrativeEventChrome
              title="Loose Rumor"
              eventId="ev-rumor"
              chapterAnchor={null}
              noChapterLabel="No chapter anchor"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Anchor · no description">
          <NodeChromeShell accent="worldkb">
            <WorkTimelineNarrativeEventChrome
              title="Silent Accord"
              eventId="ev-accord"
              chapterAnchor="Ch. 7"
              noChapterLabel="No chapter anchor"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="worldkb" selected>
            <WorkTimelineNarrativeEventChrome
              title="The Crossing"
              eventId="ev-crossing"
              chapterAnchor="Ch. 3"
              noChapterLabel="No chapter anchor"
              description="Selected Narrative event — selection ring from NodeChromeShell."
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="worldkb" dragging>
            <WorkTimelineNarrativeEventChrome
              title="Midpoint Reversal"
              eventId="ev-midpoint"
              chapterAnchor="Ch. 5"
              noChapterLabel="No chapter anchor"
              description="Dragging Narrative event card."
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.5 Work Timeline — Moment scene                                   */
/* ------------------------------------------------------------------ */

/**
 * Moment scene frame — scene name + scene-id + optional manuscript-anchor
 * + optional status. Surface spine is outline; layer accent is
 * canvas-layer-moment-accent on BookMarked icon + anchor badge.
 * Variants: with/without manuscript anchor; status chip; selected/dragging
 * (boundary §4.5 F3).
 */
function MomentSceneFixtureFrame() {
  return (
    <FixtureFrame
      title="Work Timeline — Moment scene"
      description="Moment layer scene cards on the Work Timeline. Surface spine is outline (outline-derived Work surface); BookMarked icon and manuscript-anchor badge use canvas-layer-moment-accent. Composes NodeChromeShell + WorkTimelineMomentSceneChrome — same extract as App RF WorkTimelineMomentSceneNode."
      testId="work-timeline-fixture-moment-scene"
    >
      <VariantMatrix testId="work-timeline-moment-scene-matrix">
        <VariantChip label="Manuscript anchor + status">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentSceneChrome
              title="Opening at the Gate"
              sceneId="sc-1"
              manuscriptAnchorLabel="Ch. 1 · sc-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="No manuscript anchor">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentSceneChrome
              title="Unanchored Scene"
              sceneId="sc-loose"
              manuscriptAnchorLabel={null}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Anchor · no status">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentSceneChrome
              title="Council Chamber"
              sceneId="sc-2"
              manuscriptAnchorLabel="Ch. 3 · sc-2"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="outline" selected>
            <WorkTimelineMomentSceneChrome
              title="Opening at the Gate"
              sceneId="sc-1"
              manuscriptAnchorLabel="Ch. 1 · sc-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="outline" dragging>
            <WorkTimelineMomentSceneChrome
              title="Council Chamber"
              sceneId="sc-2"
              manuscriptAnchorLabel="Ch. 3 · sc-2"
              status="revised"
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.6 Work Timeline — Moment beat                                    */
/* ------------------------------------------------------------------ */

/**
 * Moment beat frame — beat label + optional manuscript-anchor + optional
 * status. Surface spine is outline; layer accent is
 * canvas-layer-moment-accent on Milestone icon + anchor badge.
 * Variants: with/without manuscript anchor; selected/dragging
 * (boundary §4.6 F3). Both scene + beat frames are required (Moment = pair).
 */
function MomentBeatFixtureFrame() {
  return (
    <FixtureFrame
      title="Work Timeline — Moment beat"
      description="Moment layer beat cards on the Work Timeline. Surface spine is outline; Milestone icon and manuscript-anchor badge use canvas-layer-moment-accent. Composes NodeChromeShell + WorkTimelineMomentBeatChrome — same extract as App RF WorkTimelineMomentBeatNode. Ships with Moment scene (do not scene-only)."
      testId="work-timeline-fixture-moment-beat"
    >
      <VariantMatrix testId="work-timeline-moment-beat-matrix">
        <VariantChip label="Manuscript anchor">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentBeatChrome
              title="Hook Beat"
              manuscriptAnchorLabel="Ch. 1 · sc-1 · bt-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="No manuscript anchor">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentBeatChrome
              title="Loose Beat"
              manuscriptAnchorLabel={null}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Anchor · no status">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentBeatChrome
              title="Turn Beat"
              manuscriptAnchorLabel="Ch. 3 · sc-2 · bt-2"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="outline" selected>
            <WorkTimelineMomentBeatChrome
              title="Hook Beat"
              manuscriptAnchorLabel="Ch. 1 · sc-1 · bt-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="outline" dragging>
            <WorkTimelineMomentBeatChrome
              title="Turn Beat"
              manuscriptAnchorLabel="Ch. 3 · sc-2 · bt-2"
              status="revised"
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.7 V1.126 P1 — Moment directed axis spine (static SVG sample)      */
/* ------------------------------------------------------------------ */

/**
 * Static SVG spine sample for the Moment layer's chapter-scoped micro-axis.
 * Density-encoded per ND-A1: segment length proportional to scene count.
 * Chapter labels sit above each segment; scene ticks inside.
 */
const ACCENT_MOMENT = 'var(--color-canvas-layer-moment-accent)';

function MomentSpineSample() {
  return (
    <svg width={420} height={48} className="block" aria-hidden>
      <line x1={20} y1={24} x2={140} y2={24} stroke={ACCENT_MOMENT} strokeWidth={3} strokeLinecap="round" />
      <text x={80} y={42} textAnchor="middle" fill={ACCENT_MOMENT} fontSize={9} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Ch. 1</text>
      <line x1={25} y1={21} x2={25} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={55} y1={21} x2={55} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={85} y1={21} x2={85} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={115} y1={21} x2={115} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />

      <line x1={140} y1={24} x2={260} y2={24} stroke={ACCENT_MOMENT} strokeWidth={3} strokeLinecap="round" />
      <text x={200} y={42} textAnchor="middle" fill={ACCENT_MOMENT} fontSize={9} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Ch. 2</text>
      <line x1={160} y1={21} x2={160} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={200} y1={21} x2={200} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={240} y1={21} x2={240} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />

      <line x1={260} y1={24} x2={400} y2={24} stroke={ACCENT_MOMENT} strokeWidth={3} strokeLinecap="round" />
      <polygon points={`395,24 380,16 380,32`} fill={ACCENT_MOMENT} />
      <text x={330} y={42} textAnchor="middle" fill={ACCENT_MOMENT} fontSize={9} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Ch. 3</text>
      <line x1={280} y1={21} x2={280} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={320} y1={21} x2={320} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={360} y1={21} x2={360} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
      <line x1={380} y1={21} x2={380} y2={27} stroke={ACCENT_MOMENT} strokeWidth={1} strokeLinecap="round" opacity={0.7} />
    </svg>
  );
}

function MomentSpineFixtureFrame() {
  return (
    <FixtureFrame
      title="V1.126 P1 — Moment directed axis spine"
      description="Moment layer chapter-scoped micro-axis on the Work Timeline. Gray segments are chapter-length; density-encoded per ND-A1 (segment length ∝ scene count) — a deliberate rhythm break from Brief+Narrative's time-span convention. Scene ticks sit inside each segment. Arrow head at the rightmost segment."
      testId="work-timeline-fixture-moment-spine"
    >
      <VariantMatrix testId="work-timeline-moment-spine-matrix">
        <VariantChip label="Moment layer spine">
          <div className="flex flex-col gap-2 rounded-card border border-gray-alpha-300 bg-canvas-surface p-4">
            <span className="text-label-12 font-medium" style={{ color: ACCENT_MOMENT }}>
              Moment — chapter micro-segments
            </span>
            <MomentSpineSample />
          </div>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Work Timeline fixtures — Narrative event / Moment scene / Moment beat /
 * Moment spine covering boundary §4.4–§4.7 variant matrices.
 * Presentational-only; no daemon, no RF, no contracts, no i18n.
 * Moment = scene + beat (both required).
 */
export function WorkTimelineCanvasFixtures() {
  return (
    <div data-testid="work-timeline-canvas-fixtures">
      <NarrativeEventFixtureFrame />
      <MomentSceneFixtureFrame />
      <MomentBeatFixtureFrame />
      <MomentSpineFixtureFrame />
    </div>
  );
}
