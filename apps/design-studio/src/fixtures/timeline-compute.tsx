/**
 * Studio fixtures for Compute result nodes on the World Timeline (V1.147 P2,
 * behavior spec §5 Timeline citizenship + §1 P2 Run Module entry).
 *
 * Composes:
 *   - `@42ch/nexus-ui` promoted primitives:
 *     `ComputeResultNodeChrome` (Narrative node body) + `ComputeInspectorSections`
 *     (node inspector content) — V1.147 P2 promotion list.
 *   - `@web-canvas/node-chrome-shell` + `@web-canvas/timeline-node-chrome`
 *     (app presentational extracts) for the KB Event / Brief-era cards the
 *     compute nodes sit alongside.
 *
 * Variant matrix:
 *   1. Compute result node on the Narrative layer WITH KB event nodes — one
 *      compute card per accepted Run, no double-render (Event cards and
 *      Compute cards are distinct node kinds over different sources).
 *   2. Compute inspector — module + version + params digest + affected
 *      knowledge + Run id + provenance chip + Open Run affordance; direct
 *      run / preset / sparse variants.
 *   3. Run Module entry chrome — canvas toolbar entry + empty-state hint
 *      (fixture-level product chrome; the app owns wiring in Task 3).
 *   4. Brief-layer-unaffected evidence — the same world renders era markers
 *      only on Brief; compute events never leak into the era sweep.
 *
 * Boundary: no `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon, no
 * `useTranslation`. All copy is literal English caller-owned strings. Toggle
 * the shell theme to verify light + dark — every class is token-backed.
 */

import type { ReactNode } from 'react';

import { Cpu, Play } from 'lucide-react';

import {
  Button,
  ComputeInspectorSections,
  ComputeResultNodeChrome,
  type ComputeInspectorSectionsCopy,
} from '@42ch/nexus-ui';
import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // @web-canvas/node-chrome-shell - transitional until package promotion criteria met
import {
  TimelineBriefEraChrome,
  TimelineEventChrome,
} from '@web-canvas/timeline-node-chrome'; // @web-canvas/timeline-node-chrome - transitional until package promotion criteria met

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (product vocabulary lock — Run / Compute result /  */
/*  From module run / From preset; no protocol jargon)                  */
/* ------------------------------------------------------------------ */

const KIND_LABEL = 'Compute result';
const PROVENANCE_DIRECT = 'From module run';
const PROVENANCE_PRESET = 'From preset';

const INSPECTOR_COPY: ComputeInspectorSectionsCopy = {
  moduleTitle: 'Module',
  reportTitle: 'Report',
  affectedTitle: 'Affected knowledge',
  runTitle: 'Run',
  paramsTitle: 'Parameters',
  openRunLabel: 'Open Run',
};

/* ------------------------------------------------------------------ */
/*  Fake data — mirrors the T1 TimelineEventInfo provenance envelope     */
/*  extensions.compute = { module_id, module_version, run_id,            */
/*  source_kind: "direct_invoke" | "preset" }                            */
/* ------------------------------------------------------------------ */

const RUN_EVENT = {
  title: 'Aria strikes Brann',
  summary: 'Brann takes 6 damage and staggers back.',
  moduleName: 'Basic Combat',
  moduleVersion: '1.0.0',
  runId: 'run_9f3a2c',
  paramsDigest: 'attacker_id: char-aria · defender_id: char-brann',
  affected: [
    { id: 'char-aria', title: 'Aria' },
    { id: 'char-brann', title: 'Brann' },
  ],
};

const PRESET_EVENT = {
  title: 'The arena bell tolls',
  summary: 'Combat Engine resolves a skirmish across the arena.',
  moduleName: 'Combat Engine',
  moduleVersion: '3.0.0',
  paramsDigest: 'scenario: arena · rounds: 3',
  affected: [{ id: 'loc-arena', title: 'Arena' }],
};

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

/* ------------------------------------------------------------------ */
/*  §1 — Compute result node on the Narrative layer (with KB events)    */
/* ------------------------------------------------------------------ */

function NodeShellCard({
  children,
  selected,
  dragging,
}: {
  children: ReactNode;
  selected?: boolean;
  dragging?: boolean;
}) {
  return (
    <NodeChromeShell accent="worldkb" selected={selected} dragging={dragging}>
      {children}
    </NodeChromeShell>
  );
}

/**
 * Mini when-axis scene: two KB Event cards (manual author events) with a
 * Compute result card between them — proving compute nodes join the same
 * Narrative row as manual events, rendered once per accepted Run (no
 * double-render: one card kind per source, no duplicate compute card for the
 * same event). Distinguishable via Cpu icon + kind pill + provenance chip.
 */
function ComputeNarrativeFrame() {
  return (
    <FixtureFrame
      title="Compute result node — Narrative layer (alongside KB events)"
      description="One when-axis row: manual KB Event cards and a Compute result card from an accepted Run. Same family (NodeChromeShell + narrative layer accent) but distinguishable — Cpu icon, 'Compute result' kind pill, provenance chip (module · version · run id). No double-render: each accepted Run produces exactly one compute card; Event cards stay on the KB path."
      testId="timeline-compute-narrative"
    >
      <div
        data-testid="timeline-compute-narrative-matrix"
        className="flex flex-wrap items-start gap-6 rounded-card bg-canvas-surface p-6"
      >
        <VariantChip label="KB event (unchanged)">
          <NodeShellCard>
            <TimelineEventChrome
              title="The Crossing"
              blockTypeLabel="Event"
              occurredAtHint="Year 412 · spring"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="4 source anchors"
              version={3}
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Compute result · direct run · with summary">
          <NodeShellCard>
            <ComputeResultNodeChrome
              title={RUN_EVENT.title}
              kindLabel={KIND_LABEL}
              provenanceLabel={PROVENANCE_DIRECT}
              moduleName={RUN_EVENT.moduleName}
              moduleVersion={RUN_EVENT.moduleVersion}
              runId={RUN_EVENT.runId}
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Compute result · preset · no Run id">
          <NodeShellCard>
            <ComputeResultNodeChrome
              title={PRESET_EVENT.title}
              kindLabel={KIND_LABEL}
              provenanceLabel={PROVENANCE_PRESET}
              moduleName={PRESET_EVENT.moduleName}
              moduleVersion={PRESET_EVENT.moduleVersion}
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Compute result · summary-less fallback (module name)">
          <NodeShellCard>
            <ComputeResultNodeChrome
              title="Economy Ticker"
              kindLabel={KIND_LABEL}
              provenanceLabel={PROVENANCE_DIRECT}
              moduleName="Economy Ticker"
              moduleVersion="2.1.0"
              runId="run_55e8a1"
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Compute result · selected">
          <NodeShellCard selected>
            <ComputeResultNodeChrome
              title={RUN_EVENT.title}
              kindLabel={KIND_LABEL}
              provenanceLabel={PROVENANCE_DIRECT}
              moduleName={RUN_EVENT.moduleName}
              moduleVersion={RUN_EVENT.moduleVersion}
              runId={RUN_EVENT.runId}
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Compute result · dragging">
          <NodeShellCard dragging>
            <ComputeResultNodeChrome
              title={PRESET_EVENT.title}
              kindLabel={KIND_LABEL}
              provenanceLabel={PROVENANCE_PRESET}
              moduleName={PRESET_EVENT.moduleName}
              moduleVersion={PRESET_EVENT.moduleVersion}
            />
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="KB event (unchanged)">
          <NodeShellCard>
            <TimelineEventChrome
              title="Silent Accord"
              blockTypeLabel="Event"
              occurredAtHint="Year 700"
              temporalUnknownLabel="Temporal unknown"
              sourceAnchorLabel="0 source anchors"
              version={1}
            />
          </NodeShellCard>
        </VariantChip>
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §2 — Compute inspector (provenance + Run link)                      */
/* ------------------------------------------------------------------ */

function InspectorVariant({
  testId,
  chipLabel,
  children,
}: {
  testId: string;
  chipLabel: string;
  children: ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className="flex flex-col gap-3 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
    >
      <span className="text-label-12 font-medium text-gray-500">{chipLabel}</span>
      {children}
    </div>
  );
}

function ComputeInspectorFrame() {
  return (
    <FixtureFrame
      title="Compute inspector — provenance + Run link"
      description="Inspector content for a selected Compute result node (spec §5 Inspector row): Module name + version with provenance chip, Report digest, Parameters digest, affected KnowledgeEntries, and the Run section (id + Open Run affordance). The provenance chip renders in every state so the inspector can always tell direct-run from preset. Open Run hands navigation to the caller (T3 wires Settings → Modules run detail)."
      testId="timeline-compute-inspector"
    >
      <div
        data-testid="timeline-compute-inspector-matrix"
        className="grid gap-6 lg:grid-cols-3"
      >
        <InspectorVariant testId="timeline-compute-inspector-direct" chipLabel="Direct run">
          <ComputeInspectorSections
            moduleName={RUN_EVENT.moduleName}
            moduleVersion={RUN_EVENT.moduleVersion}
            reportDigest={RUN_EVENT.summary}
            paramsDigest={RUN_EVENT.paramsDigest}
            affectedEntries={RUN_EVENT.affected}
            runId={RUN_EVENT.runId}
            provenanceLabel={PROVENANCE_DIRECT}
            copy={INSPECTOR_COPY}
            onOpenRun={() => undefined}
          />
        </InspectorVariant>

        <InspectorVariant testId="timeline-compute-inspector-preset" chipLabel="Preset path">
          <ComputeInspectorSections
            moduleName={PRESET_EVENT.moduleName}
            moduleVersion={PRESET_EVENT.moduleVersion}
            reportDigest={PRESET_EVENT.summary}
            paramsDigest={PRESET_EVENT.paramsDigest}
            affectedEntries={PRESET_EVENT.affected}
            provenanceLabel={PROVENANCE_PRESET}
            copy={INSPECTOR_COPY}
          />
        </InspectorVariant>

        <InspectorVariant testId="timeline-compute-inspector-sparse" chipLabel="Sparse event">
          <ComputeInspectorSections
            moduleName="Economy Ticker"
            moduleVersion="2.1.0"
            provenanceLabel={PROVENANCE_DIRECT}
            copy={INSPECTOR_COPY}
          />
        </InspectorVariant>
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §3 — Run Module entry chrome (canvas toolbar + empty-state hint)    */
/* ------------------------------------------------------------------ */

/**
 * Fixture-level product chrome (app-owned in Task 3): the Run Module button
 * lives in the Timeline canvas header row, opening the shared Run Studio with
 * World + branch pre-filled (spec §1 P2). Copy + navigation are caller-owned.
 */
function CanvasToolbarChrome() {
  return (
    <div
      data-testid="timeline-compute-toolbar"
      className="flex flex-wrap items-center justify-between gap-2 rounded-card border border-gray-alpha-300 bg-background-100 px-4 py-3"
    >
      <div>
        <p className="text-heading-16 font-heading text-gray-1000">World Timeline</p>
        <p className="text-copy-13 text-gray-700">
          Narrative — make the world react here.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="secondary"
          size="small"
          data-testid="timeline-compute-run-module-button"
        >
          <Cpu className="h-3.5 w-3.5" aria-hidden />
          Run Module
        </Button>
        <Button variant="secondary" size="small" data-testid="timeline-compute-alt-button">
          Show list
        </Button>
      </div>
    </div>
  );
}

/**
 * Empty-state hint: a fresh World has no events yet — the Run Module CTA is
 * the in-flow entry to make the world react (spec §1 P2). App-owned
 * EmptyState composition in Task 3; this proves the hint + CTA copy.
 */
function EmptyStateHintChrome() {
  return (
    <div
      data-testid="timeline-compute-empty-hint"
      className="flex flex-col items-center gap-3 rounded-card border border-gray-alpha-300 bg-canvas-surface px-6 py-10 text-center"
    >
      <Cpu className="h-8 w-8 text-canvas-layer-narrative-accent" aria-hidden />
      <p className="text-heading-16 font-heading text-gray-1000">
        No events yet
      </p>
      <p className="max-w-md text-copy-13 text-gray-700">
        Run a module to make the world react here — accepted Runs land as
        Compute result nodes on this timeline.
      </p>
      <Button variant="primary" size="small" data-testid="timeline-compute-empty-run-module">
        <Play className="h-3.5 w-3.5" aria-hidden />
        Run Module
      </Button>
    </div>
  );
}

function RunModuleEntryFrame() {
  return (
    <FixtureFrame
      title="Run Module entry — canvas toolbar + empty-state hint"
      description="Spec §1 P2 hero shortcut: a Run Module button in the Timeline header opens the SAME Run Studio primitives with World + branch pre-filled (P1 reuse — no parallel studio). Fixture-level chrome mirrors the app header button styling; Task 3 wires navigation + pre-fill."
      testId="timeline-compute-run-module-entry"
    >
      <div className="grid gap-6">
        <CanvasToolbarChrome />
        <EmptyStateHintChrome />
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  §4 — Brief-layer-unaffected evidence                                */
/* ------------------------------------------------------------------ */

/**
 * Same world, Brief layer: era markers only. Even though the world has
 * compute events (accepted Runs), the Brief era sweep never renders compute
 * nodes — spec §5 "Layer: Narrative only this iteration; Brief unaffected."
 */
function BriefUnaffectedFrame() {
  return (
    <FixtureFrame
      title="Brief layer — unaffected (same world data)"
      description="Evidence variant: the world below HAS accepted compute events, but the Brief layer projects era markers only — no compute nodes, no provenance chrome. Compute is a Narrative-layer citizen this iteration; the era sweep stays untouched (layer-feel: brief = wide sparse gold sweep)."
      testId="timeline-compute-brief"
    >
      <div
        data-testid="timeline-compute-brief-matrix"
        className="flex flex-wrap items-start gap-6 rounded-card bg-canvas-surface p-6"
      >
        <VariantChip label="Brief era (unchanged)">
          <NodeShellCard>
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
          </NodeShellCard>
        </VariantChip>

        <VariantChip label="Brief era (unchanged)">
          <NodeShellCard>
            <TimelineBriefEraChrome
              title="Age of Crossing"
              blockTypeLabel="Era"
              timeSpan="Year 412 → Year 700"
              temporalUnknownLabel="Temporal unknown"
              eraId="era-crossing"
              sourceAnchorLabel="1 source anchor"
              version={1}
            />
          </NodeShellCard>
        </VariantChip>

        <div
          data-testid="timeline-compute-brief-note"
          className="max-w-xs rounded-control border border-gray-alpha-300 bg-background-100 p-3 text-copy-13 text-gray-700"
        >
          This world has accepted compute runs — yet the Brief sweep renders
          era markers only. Compute result nodes never appear on the Brief
          layer (Narrative-only this iteration).
        </div>
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Timeline compute fixtures — Compute result node (Narrative), compute
 * inspector, Run Module entry chrome, and Brief-unaffected evidence.
 * Presentational-only; no daemon, no RF, no contracts, no i18n.
 */
export function TimelineComputeFixtures() {
  return (
    <div data-testid="timeline-compute-fixtures" className="grid gap-8">
      <ComputeNarrativeFrame />
      <ComputeInspectorFrame />
      <RunModuleEntryFrame />
      <BriefUnaffectedFrame />
    </div>
  );
}
