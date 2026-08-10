/**
 * Studio fixtures for World Timeline node chrome (V1.124 P0 T3; V1.156 P1 T2
 * Moment layer + Moment empty-state).
 *
 * Composes the same presentational extract App RF wrappers use:
 *   - `@web-canvas/node-chrome-shell` (card shell + worldkb spine)
 *   - `@web-canvas/timeline-node-chrome` (Brief-era / Event / KeyBlock /
 *     Moment scene / Moment beat body)
 *
 * Boundary (studio-timeline-fixture-boundaries.md §4.1–§4.6 + F1–F9):
 *   No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 *   no `useTranslation`. Static English product vocabulary only.
 *   Layer breadcrumb is out of scope (P2).
 *
 * Surface spine: `accent="worldkb"`. Layer accents live inside the extract
 * (Brief → brief-accent; Event → narrative-accent; KeyBlock → none).
 *
 * V1.156 — World Timeline Moment is a READ/projection layer (PD-3): scenes
 * come from the V1.108 `sceneBeatFixture` slot (Design Studio / tests inject
 * the payload; DR-26 tracks the future wire extension), projected onto the
 * same `work-timeline-moment-scene` / `work-timeline-moment-beat` node types
 * as the Work surface — World-Moment feel ≡ Work-Moment feel (V1.123
 * layer-feel §2.4). The node chrome is identical to the Work fixture's
 * Moment frames (incl. `accent="outline"` — the App reuses the Work node
 * components verbatim); the empty-state frame mirrors the app's honest
 * `MomentEmptyState` panel copy.
 */
import { type ReactNode } from 'react';

import { Button } from '@42ch/nexus-ui';
import { EmptyState } from '@web-ui/states'; // transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // @web-canvas/node-chrome-shell - transitional until package promotion criteria met
import {
  TimelineBriefEraChrome,
  TimelineEventChrome,
  TimelineKeyBlockChrome,
  WorkTimelineMomentBeatChrome,
  WorkTimelineMomentSceneChrome,
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
/*  §4.1 World Timeline — Brief-era                                     */
/* ------------------------------------------------------------------ */

/**
 * Brief-era frame — era name + time-span (start→end / start-only / end-only /
 * temporal-unknown) + optional world-summary + source meta. Variants cover
 * selected/dragging and with/without world-summary (boundary §4.1 F3).
 */
function BriefEraFixtureFrame() {
  return (
    <FixtureFrame
      title="World Timeline — Brief-era"
      description="Brief layer era markers on the World Timeline. Surface spine is worldkb; layer accent is canvas-layer-brief-accent on the Hourglass icon and time-span badge. Composes NodeChromeShell + TimelineBriefEraChrome from the shared extract — no Handles, no parallel badge CSS."
      testId="timeline-fixture-brief-era"
    >
      <VariantMatrix testId="timeline-brief-era-matrix">
        <VariantChip label="Full span + summary">
          <NodeChromeShell accent="worldkb">
            <TimelineBriefEraChrome
              title="The First Age"
              blockTypeLabel="Era"
              timeSpan="Year 0 → Year 412"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-first"
              worldSummary="Founding myths and the first knowledge entry lineages of the World."
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
/*  §4.2 World Timeline — Event (Narrative)                             */
/* ------------------------------------------------------------------ */

/**
 * Event frame — canonical name + temporal signal (or temporal-unknown) +
 * source-anchor count. Layer accent is canvas-layer-narrative-accent on the
 * dated badge (V1.123 P4 / extract migration). Variants: dated vs unknown,
 * source 0 vs N, selected/dragging (boundary §4.2 F3).
 */
function EventFixtureFrame() {
  return (
    <FixtureFrame
      title="World Timeline — Event"
      description="Narrative layer Event cards on the World Timeline. Surface spine is worldkb; dated temporal badges use canvas-layer-narrative-accent. Composes NodeChromeShell + TimelineEventChrome — same extract as App RF TimelineEventNode."
      testId="timeline-fixture-event"
    >
      <VariantMatrix testId="timeline-event-matrix">
        <VariantChip label="Dated + sources">
          <NodeChromeShell accent="worldkb">
            <TimelineEventChrome
              title="The Crossing"
              blockTypeLabel="Event"
              occurredAtHint="Year 412 · spring"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="4 source anchors"
              version={3}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Temporal unknown">
          <NodeChromeShell accent="worldkb">
            <TimelineEventChrome
              title="Unanchored Incident"
              blockTypeLabel="Event"
              occurredAtHint={null}
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dated · no sources">
          <NodeChromeShell accent="worldkb">
            <TimelineEventChrome
              title="Silent Accord"
              blockTypeLabel="Event"
              occurredAtHint="Year 700"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="worldkb" selected>
            <TimelineEventChrome
              title="The Crossing"
              blockTypeLabel="Event"
              occurredAtHint="Year 412 · spring"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="4 source anchors"
              version={3}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="worldkb" dragging>
            <TimelineEventChrome
              title="Midpoint Reversal"
              blockTypeLabel="Event"
              occurredAtHint="Year 550"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="2 source anchors"
              version={2}
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.3 World Timeline — KeyBlock Context cluster                      */
/* ------------------------------------------------------------------ */

/**
 * KeyBlock Context cluster — entity name + BlockType pill + source meta.
 * Distinct from Event by absence of temporal/era chrome. Variants prove
 * cluster diversity with ≥2 BlockType labels (boundary §4.3 F3).
 */
function KeyBlockFixtureFrame() {
  return (
    <FixtureFrame
      title="World Timeline — KeyBlock Context cluster"
      description="Context-cluster KeyBlock cards on the World Timeline (not when-axis Events). Surface spine is worldkb; no dedicated layer badge — identity is the BlockType pill alone. Composes NodeChromeShell + TimelineKeyBlockChrome."
      testId="timeline-fixture-key-block"
    >
      <VariantMatrix testId="timeline-key-block-matrix">
        <VariantChip label="Character">
          <NodeChromeShell accent="worldkb">
            <TimelineKeyBlockChrome
              title="Kael Veynor"
              blockTypeLabel="Character"
              sourceAnchorLabel="5 source anchors"
              version={4}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Organization">
          <NodeChromeShell accent="worldkb">
            <TimelineKeyBlockChrome
              title="Hearthstone Covenant"
              blockTypeLabel="Organization"
              sourceAnchorLabel="2 source anchors"
              version={2}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Location · no sources">
          <NodeChromeShell accent="worldkb">
            <TimelineKeyBlockChrome
              title="Ashen Gate"
              blockTypeLabel="Location"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected">
          <NodeChromeShell accent="worldkb" selected>
            <TimelineKeyBlockChrome
              title="Kael Veynor"
              blockTypeLabel="Character"
              sourceAnchorLabel="5 source anchors"
              version={4}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging">
          <NodeChromeShell accent="worldkb" dragging>
            <TimelineKeyBlockChrome
              title="Hearthstone Covenant"
              blockTypeLabel="Organization"
              sourceAnchorLabel="2 source anchors"
              version={2}
            />
          </NodeChromeShell>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.4 V1.126 P1 — Directed axis spine (static SVG samples)            */
/* ------------------------------------------------------------------ */

/**
 * Static SVG spine samples for the three layer-differentiated directed
 * center axes. Presentational-only — no RF types, no daemon data.
 * Brief: thick L-to-R arrow with gradient era ticks.
 * Narrative: thin connecting line + discrete tick marks.
 * Moment: chapter-scoped micro-segments (density-encoded per ND-A1).
 */
const ACCENT_BRIEF = 'var(--color-canvas-layer-brief-accent)';
const ACCENT_NARRATIVE = 'var(--color-canvas-layer-narrative-accent)';

function BriefSpineSample() {
  return (
    <svg width={400} height={48} className="block" aria-hidden>
      <defs>
        <linearGradient id="studio-brief-grad" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0%" stopColor={ACCENT_BRIEF} stopOpacity="0.4" />
          <stop offset="100%" stopColor={ACCENT_BRIEF} stopOpacity="1" />
        </linearGradient>
      </defs>
      <line x1={20} y1={24} x2={340} y2={24} stroke="url(#studio-brief-grad)" strokeWidth={4} strokeLinecap="round" />
      <polygon points={`360,24 340,16 340,32`} fill={ACCENT_BRIEF} />
      <line x1={20} y1={18} x2={20} y2={30} stroke={ACCENT_BRIEF} strokeWidth={2} strokeLinecap="round" />
      <text x={20} y={44} textAnchor="middle" fill={ACCENT_BRIEF} fontSize={10} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Era 1</text>
      <line x1={180} y1={18} x2={180} y2={30} stroke={ACCENT_BRIEF} strokeWidth={2} strokeLinecap="round" />
      <text x={180} y={44} textAnchor="middle" fill={ACCENT_BRIEF} fontSize={10} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Era 2</text>
      <line x1={340} y1={18} x2={340} y2={30} stroke={ACCENT_BRIEF} strokeWidth={2} strokeLinecap="round" />
      <text x={340} y={44} textAnchor="middle" fill={ACCENT_BRIEF} fontSize={10} fontFamily="var(--font-sans, ui-sans-serif, system-ui)">Era 3</text>
    </svg>
  );
}

function NarrativeSpineSample() {
  return (
    <svg width={400} height={40} className="block" aria-hidden>
      <line x1={20} y1={20} x2={380} y2={20} stroke={ACCENT_NARRATIVE} strokeWidth={1.5} strokeLinecap="round" strokeOpacity={0.6} />
      {[60, 120, 180, 240, 300, 360].map((x, i) => (
        <line key={i} x1={x} y1={16} x2={x} y2={24} stroke={ACCENT_NARRATIVE} strokeWidth={1.5} strokeLinecap="round" />
      ))}
      {[60, 180, 300].map((x, i) => (
        <text key={i} x={x} y={34} textAnchor="middle" fill={ACCENT_NARRATIVE} fontSize={9} fontFamily="var(--font-sans, ui-sans-serif, system-ui)" opacity={0.7}>tick</text>
      ))}
    </svg>
  );
}

function DirectedAxisFixtureFrame() {
  return (
    <FixtureFrame
      title="V1.126 P1 — Directed axis spine"
      description="Layer-differentiated directed center axis on the World Timeline. Brief (amber) = thick era-spanning arrow with gradient ticks at era boundaries. Narrative (blue) = thin discrete event-pin axis with fine-grained tick marks. Each layer reads at a glance as a different visual rhythm — not just token-color-different (ND-7)."
      testId="timeline-fixture-directed-axis"
    >
      <VariantMatrix testId="timeline-directed-axis-matrix">
        <VariantChip label="Brief layer spine">
          <div className="flex flex-col gap-2 rounded-card border border-gray-alpha-300 bg-canvas-surface p-4">
            <span className="text-label-12 font-medium" style={{ color: ACCENT_BRIEF }}>
              Brief — era-spanning arrow
            </span>
            <BriefSpineSample />
          </div>
        </VariantChip>
        <VariantChip label="Narrative layer spine">
          <div className="flex flex-col gap-2 rounded-card border border-gray-alpha-300 bg-canvas-surface p-4">
            <span className="text-label-12 font-medium" style={{ color: ACCENT_NARRATIVE }}>
              Narrative — discrete event-pin axis
            </span>
            <NarrativeSpineSample />
          </div>
        </VariantChip>
      </VariantMatrix>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4.5 World Timeline — Moment layer (V1.156 P1 T2)                   */
/* ------------------------------------------------------------------ */

/**
 * Moment layer frame — scene + beat cards on the World Timeline Moment
 * axis. Moment is a READ/projection layer (PD-3): the adapter consumes the
 * V1.108 `sceneBeatFixture` slot (Design Studio / tests inject the payload;
 * DR-26 tracks the future wire extension) and projects onto the same
 * `work-timeline-moment-scene` / `work-timeline-moment-beat` node types as
 * the Work surface — World-Moment feel ≡ Work-Moment feel (V1.123
 * layer-feel §2.4). Node chrome + `accent="outline"` match the App's
 * re-used Work node components verbatim; the frame documents the
 * World-surface projection semantics.
 */
function MomentLayerFixtureFrame() {
  return (
    <FixtureFrame
      title="World Timeline — Moment layer"
      description="Moment layer scene/beat cards on the World Timeline (V1.156). Scene-precision is fixture-driven today: the `sceneBeatFixture` prop flows orchestrator → adapter → Moment projection (same carrier as the Work surface — scenes stack by chapter, beats inside their scene). Surface accent is outline (the App reuses the Work Moment node components verbatim); BookMarked/Milestone icons + manuscript-anchor badges use canvas-layer-moment-accent."
      testId="timeline-fixture-moment-layer"
    >
      <VariantMatrix testId="timeline-moment-layer-matrix">
        <VariantChip label="Scene · anchor + status">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentSceneChrome
              title="Arrival at the Ashen Gate"
              sceneId="sc-1"
              manuscriptAnchorLabel="Ch. 1 · sc-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Scene · no anchor">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentSceneChrome
              title="Unanchored Passage"
              sceneId="sc-loose"
              manuscriptAnchorLabel={null}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Beat · anchor + status">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentBeatChrome
              title="Hook Beat"
              manuscriptAnchorLabel="Ch. 1 · sc-1 · bt-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Beat · no anchor">
          <NodeChromeShell accent="outline">
            <WorkTimelineMomentBeatChrome
              title="Loose Beat"
              manuscriptAnchorLabel={null}
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Selected scene">
          <NodeChromeShell accent="outline" selected>
            <WorkTimelineMomentSceneChrome
              title="Arrival at the Ashen Gate"
              sceneId="sc-1"
              manuscriptAnchorLabel="Ch. 1 · sc-1"
              status="draft"
            />
          </NodeChromeShell>
        </VariantChip>

        <VariantChip label="Dragging beat">
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
/*  §4.6 World Timeline — Moment empty-state (V1.156 P1 T2)             */
/* ------------------------------------------------------------------ */

/**
 * Moment empty-state frame — honest panel when the `sceneBeatFixture` slot
 * is absent or empty (zero projected nodes, PD-3). Copy + testids mirror
 * the app's `MomentEmptyState` verbatim: no "create Moment" CTA because
 * this is NOT a World Moment authoring surface (no World-owned Moment
 * write flow) — the escape hatch returns to Narrative.
 */
function MomentEmptyStateFixtureFrame() {
  return (
    <FixtureFrame
      title="World Timeline — Moment empty-state"
      description="Honest Moment-layer empty state when no bound-Works scene/beat fixture is injected (PD-3). Scene-precision is available when bound Works have scene/beat data in their Outline; the panel says exactly that and offers a CTA back to Narrative — there is NO 'create Moment' CTA."
      testId="timeline-fixture-moment-empty"
    >
      <div
        data-testid="timeline-moment-empty-state"
        className="rounded-card border border-gray-alpha-400 bg-background-100"
      >
        <EmptyState
          title="No scene or beat data yet"
          description="Scene-precision is available when bound Works have scene/beat data in their Outline. Add scenes and beats to a bound Work, or switch to Narrative for events."
          action={
            <Button
              type="button"
              variant="primary"
              data-testid="timeline-moment-empty-cta"
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
 * World Timeline fixtures — Brief-era / Event / KeyBlock / Directed axis /
 * Moment layer / Moment empty-state covering boundary §4.1–§4.6 variant
 * matrices. Presentational-only; no daemon, no RF, no contracts, no i18n.
 */
export function TimelineCanvasFixtures() {
  return (
    <div data-testid="timeline-canvas-fixtures">
      <BriefEraFixtureFrame />
      <EventFixtureFrame />
      <KeyBlockFixtureFrame />
      <DirectedAxisFixtureFrame />
      <MomentLayerFixtureFrame />
      <MomentEmptyStateFixtureFrame />
    </div>
  );
}
