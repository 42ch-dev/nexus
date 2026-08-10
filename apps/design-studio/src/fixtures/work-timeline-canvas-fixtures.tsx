/**
 * Studio fixtures for Work Timeline node chrome (V1.124 P0 T4; V1.156 P2 T2
 * Brief layer + Brief empty-state).
 *
 * Composes the same presentational extract App RF wrappers use:
 *   - `@web-canvas/node-chrome-shell` (card shell + spine accent)
 *   - `@web-canvas/timeline-node-chrome` (Narrative event / Moment scene /
 *     Moment beat / Brief-era body)
 *
 * Boundary (studio-timeline-fixture-boundaries.md §4.4–§4.9 + F1–F9):
 *   No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 *   no `useTranslation`. Static English product vocabulary only.
 *   Layer breadcrumb is out of scope (P2).
 *
 * Spines: Narrative → `accent="worldkb"`; Moment scene/beat → `accent="outline"`;
 * Brief → `accent="worldkb"` (same `timeline-brief-era` node type the World
 * surface uses — Work-Brief feel ≡ World-Brief feel).
 * Layer accents live inside the extract (Narrative → narrative-accent;
 * Moment → moment-accent; Brief → brief-accent). Moment = scene + beat
 * (both frames required).
 *
 * V1.156 — Work-Brief is a READ-only projection of the bound World's Brief
 * (PD-2): era entities (`block_type=era`) from the bound World's KB graph
 * (V1.73) render as `TimelineBriefEraChrome` on the worldkb spine; the
 * empty-state frame mirrors the app's honest `BriefEmptyState` panel copy.
 */
import { type ReactNode } from 'react';

import { Button } from '@42ch/nexus-ui';
import { EmptyState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // @web-canvas/node-chrome-shell - transitional until package promotion criteria met
import {
  TimelineBriefEraChrome,
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
/*  §4.8 Work Timeline — Brief layer (V1.156 P2 T2)                     */
/* ------------------------------------------------------------------ */

/**
 * Brief layer frame — era markers on the Work Timeline Brief axis. Work-
 * Brief is a READ-only projection of the bound World's Brief (PD-2): era
 * entities (`block_type=era`) from the bound World's KB graph (V1.73) are
 * projected onto the same `timeline-brief-era` node type + `TimelineBriefEraChrome`
 * the World surface uses — Work-Brief feel ≡ World-Brief feel (same carrier,
 * same worldkb surface spine, same brief-accent badges). The Work does NOT
 * gain an authored Brief; no Work-owned Brief write flow exists.
 */
function BriefLayerFixtureFrame() {
  return (
    <FixtureFrame
      title="Work Timeline — Brief layer"
      description="Brief layer era markers on the Work Timeline (V1.156). Bound-World era data (`block_type=era` from the bound World's KB graph — V1.73, no new route) renders as TimelineBriefEraChrome on the worldkb spine: Work-Brief feel ≡ World-Brief feel. Brief remains World spine; the Work gains no authored Brief (PD-2)."
      testId="work-timeline-fixture-brief-layer"
    >
      <VariantMatrix testId="work-timeline-brief-layer-matrix">
        <VariantChip label="Full span + summary">
          <NodeChromeShell accent="worldkb">
            <TimelineBriefEraChrome
              title="The First Age"
              blockTypeLabel="Era"
              timeSpan="Year 0 → Year 412"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-first"
              worldSummary="Founding myths and the first knowledge entry lineages of the bound World."
              sourceAnchorLabel="3 source anchors"
              version={2}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Start-only">
          <NodeChromeShell accent="worldkb">
            <TimelineBriefEraChrome
              title="Age of Crossing"
              blockTypeLabel="Era"
              timeSpan="Year 412 →"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-crossing"
              sourceAnchorLabel="1 source anchor"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="End-only">
          <NodeChromeShell accent="worldkb">
            <TimelineBriefEraChrome
              title="Twilight Compact"
              blockTypeLabel="Era"
              timeSpan="→ Year 900"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Temporal unknown">
          <NodeChromeShell accent="worldkb">
            <TimelineBriefEraChrome
              title="Uncharted Brief"
              blockTypeLabel="Era"
              timeSpan={null}
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="worldkb" selected>
            <TimelineBriefEraChrome
              title="The First Age"
              blockTypeLabel="Era"
              timeSpan="Year 0 → Year 412"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-first"
              worldSummary="Selected Brief-era card — selection ring from NodeChromeShell."
              sourceAnchorLabel="3 source anchors"
              version={2}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="worldkb" dragging>
            <TimelineBriefEraChrome
              title="Age of Crossing"
              blockTypeLabel="Era"
              timeSpan="Year 412 → Year 700"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-crossing"
              sourceAnchorLabel="2 source anchors"
              version={3}
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.9 Work Timeline — Brief empty-state (V1.156 P2 T2)               */
/* ------------------------------------------------------------------ */

/**
 * Brief empty-state frame — honest panel when the active layer is Brief
 * but the projection has zero nodes (no bound World; or the bound World's
 * KB graph has no `block_type=era` entities, PD-2). Copy + testids mirror
 * the app's `BriefEmptyState` verbatim: no "create Brief" CTA because the
 * Work does NOT own Brief authoring — the escape hatch returns to
 * Narrative.
 */
function BriefEmptyStateFixtureFrame() {
  return (
    <FixtureFrame
      title="Work Timeline — Brief empty-state"
      description="Honest Brief-layer empty state when no World is bound or the bound World's graph has no era entities (PD-2). World-shape context comes from the bound World's Brief; the panel says exactly that and offers a CTA back to Narrative — there is NO 'create Brief' CTA."
      testId="work-timeline-fixture-brief-empty"
    >
      <div
        data-testid="work-timeline-brief-empty-state"
        className="rounded-card border border-gray-alpha-400 bg-background-100"
      >
        <EmptyState
          title="No world-shape context yet"
          description="World-shape context appears here when this Work is bound to a World with era markers. Brief is a read-only projection of the bound World’s Brief."
          action={
            <Button
              type="button"
              variant="primary"
              data-testid="work-timeline-brief-empty-cta"
            >
              Switch to Narrative
            </Button>
          }
        />
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Work Timeline fixtures — Narrative event / Moment scene / Moment beat /
 * Moment spine / Brief layer / Brief empty-state covering boundary
 * §4.4–§4.9 variant matrices. Presentational-only; no daemon, no RF, no
 * contracts, no i18n. Moment = scene + beat (both required).
 */
export function WorkTimelineCanvasFixtures() {
  return (
    <div data-testid="work-timeline-canvas-fixtures">
      <NarrativeEventFixtureFrame />
      <MomentSceneFixtureFrame />
      <MomentBeatFixtureFrame />
      <MomentSpineFixtureFrame />
      <BriefLayerFixtureFrame />
      <BriefEmptyStateFixtureFrame />
    </div>
  );
}
