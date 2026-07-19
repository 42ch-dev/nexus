/**
 * Studio fixtures for World Timeline node chrome (V1.124 P0 T3).
 *
 * Composes the same presentational extract App RF wrappers use:
 *   - `@web-canvas/node-chrome-shell` (card shell + worldkb spine)
 *   - `@web-canvas/timeline-node-chrome` (Brief-era / Event / KeyBlock body)
 *
 * Boundary (studio-timeline-fixture-boundaries.md §4.1–§4.3 + F1–F9):
 *   No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients,
 *   no `useTranslation`. Static English product vocabulary only.
 *   Layer breadcrumb is out of scope (P2).
 *
 * Surface spine: `accent="worldkb"`. Layer accents live inside the extract
 * (Brief → brief-accent; Event → narrative-accent; KeyBlock → none).
 */
import { type ReactNode } from 'react';

import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // @web-canvas/node-chrome-shell - transitional until package promotion criteria met
import {
  TimelineBriefEraChrome,
  TimelineEventChrome,
  TimelineKeyBlockChrome,
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
              worldSummary="Founding myths and the first KeyBlock lineages of the World."
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
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * World Timeline fixtures — three frames (Brief-era / Event / KeyBlock)
 * covering boundary §4.1–§4.3 variant matrices. Presentational-only; no
 * daemon, no RF, no contracts, no i18n.
 */
export function TimelineCanvasFixtures() {
  return (
    <div data-testid="timeline-canvas-fixtures">
      <BriefEraFixtureFrame />
      <EventFixtureFrame />
      <KeyBlockFixtureFrame />
    </div>
  );
}
