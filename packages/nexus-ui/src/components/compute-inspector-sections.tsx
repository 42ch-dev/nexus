import type { ReactNode } from 'react';

import { ExternalLink } from 'lucide-react';

import { cn } from '../lib/cn';

import { Button } from './button';

/**
 * Compute inspector content sections (V1.147 P2) — pure presentational
 * inspector content for a Narrative-layer Compute result node (behavior spec
 * §5 Inspector row).
 *
 * Content per spec: module name + version, report digest (module event
 * summary), affected KnowledgeEntries, provenance ("From module run" vs
 * "From preset" when distinguishable) + Run id, and an Open Run affordance
 * that hands navigation back to the caller (T3 wires it to
 * Settings → Modules run detail). Parameters digest is a caller-resolved
 * string (the exact extraction from the event envelope is App-owned).
 *
 * Provenance placement: the chip renders in the Module card in EVERY state —
 * direct runs and preset-path nodes alike — so the inspector can always tell
 * which path produced the node (spec §5). The Run section (Run id + Open Run)
 * renders only when a direct Run id exists.
 *
 * Section pattern mirrors `ProposalSections` (label-14 section headers +
 * token-backed cards). Empty sections are hidden, so sparse events (no
 * digest / no affected entries / no run) render honestly without stub copy.
 *
 * Presentational boundary: no wire-contract package, no i18n hook, no
 * router. Copy + callbacks are caller-owned. Structural mirrors only —
 * compatibility with generated DTOs is via structural typing.
 */
export interface ComputeAffectedEntry {
  /** KnowledgeEntry id (mono). */
  id: string;
  /** Resolved display title (caller-owned i18n fallback). */
  title: string;
}

/** Caller-owned copy for the compute inspector (i18n lives in the app). */
export interface ComputeInspectorSectionsCopy {
  moduleTitle: string;
  reportTitle: string;
  affectedTitle: string;
  runTitle: string;
  paramsTitle: string;
  /** Open Run affordance label — e.g. "Open Run". */
  openRunLabel: string;
}

export interface ComputeInspectorSectionsProps {
  /** Module display name. */
  moduleName: string;
  /** Module version string (rendered as `v{moduleVersion}`). */
  moduleVersion: string;
  /** Module event summary — the report digest (damage line etc.). */
  reportDigest?: string;
  /** Affected KnowledgeEntries surfaced by the event. */
  affectedEntries?: ComputeAffectedEntry[];
  /** Short run correlation id. Absent → no Run section (preset path). */
  runId?: string;
  /** Resolved parameters digest (e.g. "attacker: Aria · defender: Brann"). */
  paramsDigest?: string;
  /** Provenance chip label — "From module run" / "From preset". */
  provenanceLabel: string;
  copy: ComputeInspectorSectionsCopy;
  /**
   * Open Run hand-off — the caller navigates to the Run inspector
   * (Settings → Modules run detail in the app). Rendered only when `runId`
   * is present. Optional: read-only inspections omit it.
   */
  onOpenRun?: () => void;
  className?: string;
}

function Section({
  title,
  testId,
  children,
}: {
  title: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <section data-testid={testId} className="flex flex-col gap-2">
      <h4 className="text-label-14 font-medium text-gray-1000">{title}</h4>
      {children}
    </section>
  );
}

/**
 * ComputeInspectorSections — inspector body for a Compute result node.
 * Renders Module / Report / Parameters / Affected knowledge / Run sections;
 * empty sections are hidden. The Run section carries the provenance chip +
 * Open Run affordance when a direct Run id exists.
 */
export function ComputeInspectorSections({
  moduleName,
  moduleVersion,
  reportDigest,
  affectedEntries = [],
  runId,
  paramsDigest,
  provenanceLabel,
  copy,
  onOpenRun,
  className,
}: ComputeInspectorSectionsProps) {
  return (
    <div
      data-testid="compute-inspector-sections"
      className={cn('flex flex-col gap-5', className)}
    >
      <Section title={copy.moduleTitle} testId="compute-inspector-section-module">
        <div className="flex flex-wrap items-center gap-2 rounded-control border border-gray-alpha-300 bg-background-100 p-3">
          <span
            data-testid="compute-inspector-module-name"
            className="text-label-14 font-medium text-gray-1000"
          >
            {moduleName}
          </span>
          <span
            data-testid="compute-inspector-module-version"
            className="text-copy-13-mono text-gray-700"
          >
            v{moduleVersion}
          </span>
          <span
            data-testid="compute-inspector-provenance"
            className="rounded-pill border border-canvas-layer-narrative-accent/30 bg-canvas-layer-narrative-accent/15 px-1.5 py-0.5 text-label-12 text-canvas-layer-narrative-accent"
          >
            {provenanceLabel}
          </span>
        </div>
      </Section>

      {reportDigest ? (
        <Section title={copy.reportTitle} testId="compute-inspector-section-report">
          <p
            data-testid="compute-inspector-report-digest"
            className="rounded-control border border-gray-alpha-300 bg-background-100 p-3 text-copy-13 text-gray-1000"
          >
            {reportDigest}
          </p>
        </Section>
      ) : null}

      {paramsDigest ? (
        <Section title={copy.paramsTitle} testId="compute-inspector-section-params">
          <p
            data-testid="compute-inspector-params-digest"
            className="rounded-control border border-gray-alpha-300 bg-background-100 p-3 font-mono text-copy-13 text-gray-1000"
          >
            {paramsDigest}
          </p>
        </Section>
      ) : null}

      {affectedEntries.length > 0 ? (
        <Section title={copy.affectedTitle} testId="compute-inspector-section-affected">
          <ul className="flex flex-col gap-1.5">
            {affectedEntries.map((entry) => (
              <li
                key={entry.id}
                data-testid={`compute-inspector-affected-${entry.id}`}
                className="flex flex-wrap items-baseline gap-2 rounded-control border border-gray-alpha-300 bg-background-100 px-3 py-2"
              >
                <span className="text-copy-13 text-gray-1000">{entry.title}</span>
                <span className="font-mono text-copy-13 text-gray-700">{entry.id}</span>
              </li>
            ))}
          </ul>
        </Section>
      ) : null}

      {runId ? (
        <Section title={copy.runTitle} testId="compute-inspector-section-run">
          <div className="rounded-control border border-gray-alpha-300 bg-background-100 p-3">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <span
                data-testid="compute-inspector-run-id"
                className="font-mono text-copy-13 text-gray-1000"
              >
                {runId}
              </span>
              {onOpenRun ? (
                <Button
                  variant="secondary"
                  size="small"
                  onClick={onOpenRun}
                  data-testid="compute-inspector-open-run"
                >
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden />
                  {copy.openRunLabel}
                </Button>
              ) : null}
            </div>
          </div>
        </Section>
      ) : null}
    </div>
  );
}
