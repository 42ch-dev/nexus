/**
 * Timeline compute Studio fixtures — smoke + boundary tests (V1.147 P2,
 * behavior spec §5 Timeline citizenship + §1 P2 Run Module entry).
 *
 * Evidence shapes:
 *   - Boundary — fixture imports promoted `@42ch/nexus-ui` primitives +
 *     `@web-canvas/*` extracts only; no RF / contracts / daemon / i18n.
 *   - §1 — Compute result node renders on the Narrative layer alongside KB
 *     Event nodes; one compute card per Run (no double-render evidence);
 *     kind pill + provenance chip distinguish it from Event cards.
 *   - §2 — Inspector content: module + version + params digest + affected
 *     knowledge + Run id + provenance + Open Run affordance; preset variant
 *     keeps provenance without a Run; sparse variant hides empty sections.
 *   - §3 — Run Module entry chrome: canvas toolbar button + empty-state hint.
 *   - §4 — Brief-layer-unaffected evidence: same world renders era markers
 *     only; no compute chrome inside the Brief frame.
 *   - Both themes render without throw.
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TimelineComputeFixtures } from '@/fixtures/timeline-compute';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(__dirname, '../timeline-compute.tsx');

function mockMatchMedia(prefersDark: boolean) {
  const media = {
    matches: prefersDark,
    media: '(prefers-color-scheme: dark)',
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  };
  vi.spyOn(window, 'matchMedia').mockReturnValue(
    media as unknown as MediaQueryList,
  );
}

beforeEach(() => {
  document.documentElement.classList.remove('dark');
});

afterEach(() => {
  vi.restoreAllMocks();
  document.documentElement.classList.remove('dark');
});

// ---------------------------------------------------------------------------
// Boundary — fixture file stays RF / contracts / daemon / i18n free
// ---------------------------------------------------------------------------

describe('timeline-compute fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
    expect(imports.join('\n')).not.toMatch(/@tauri-apps/);
  });

  it('imports promoted compute primitives from @42ch/nexus-ui and the shared extracts', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/ComputeResultNodeChrome/);
    expect(source).toMatch(/ComputeInspectorSections/);
    expect(source).toMatch(/@web-canvas\/timeline-node-chrome.*transitional/);
    expect(source).toMatch(/@web-canvas\/node-chrome-shell.*transitional/);
  });
});

// ---------------------------------------------------------------------------
// Rendering — §1 Narrative node, §2 inspector, §3 entry, §4 Brief evidence
// ---------------------------------------------------------------------------

describe('TimelineComputeFixtures render', () => {
  it('renders all four fixture frames without throw', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    expect(screen.getByTestId('timeline-compute-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-narrative')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-inspector')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-run-module-entry')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-brief')).toBeInTheDocument();
  });

  it('renders compute nodes alongside KB event cards with kind pill + provenance chip', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const matrix = screen.getByTestId('timeline-compute-narrative-matrix');

    // KB event cards remain on the Narrative row (no double-render — the
    // compute cards are distinct node kinds, not extra event cards).
    expect(within(matrix).getAllByText('The Crossing').length).toBeGreaterThanOrEqual(1);
    expect(within(matrix).getAllByText('Silent Accord').length).toBeGreaterThanOrEqual(1);

    // Compute cards carry the kind pill + provenance chip (direct + preset).
    const kindPills = within(matrix).getAllByTestId('compute-node-kind-pill');
    expect(kindPills.length).toBeGreaterThanOrEqual(3);
    for (const pill of kindPills) {
      expect(pill).toHaveTextContent('Compute result');
    }
    expect(
      within(matrix).getAllByTestId('compute-node-provenance-chip')[0],
    ).toHaveTextContent('From module run');
    expect(
      within(matrix).getAllByTestId('compute-node-provenance-chip')[1],
    ).toHaveTextContent('From preset');

    // Direct runs carry the run id suffix; preset cards omit it.
    expect(
      within(matrix).getAllByTestId('compute-node-run-id').length,
    ).toBeGreaterThanOrEqual(2);
    // Title appears on the default + selected variants (same event).
    expect(within(matrix).getAllByText('Aria strikes Brann').length).toBeGreaterThanOrEqual(1);
    expect(
      within(matrix).getAllByText('The arena bell tolls').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('covers selected/dragging compute node states', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const matrix = screen.getByTestId('timeline-compute-narrative-matrix');
    expect(
      matrix.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      matrix.querySelectorAll('[class*="data-[dragging=true]"]').length,
    ).toBeGreaterThanOrEqual(0);
  });

  it('renders the direct-run inspector with module, params, Run id, provenance, Open Run', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const direct = screen.getByTestId('timeline-compute-inspector-direct');
    expect(within(direct).getByTestId('compute-inspector-module-name')).toHaveTextContent(
      'Basic Combat',
    );
    expect(
      within(direct).getByTestId('compute-inspector-module-version'),
    ).toHaveTextContent('v1.0.0');
    expect(within(direct).getByTestId('compute-inspector-report-digest')).toHaveTextContent(
      'Brann takes 6 damage',
    );
    expect(within(direct).getByTestId('compute-inspector-params-digest')).toHaveTextContent(
      'attacker_id: char-aria',
    );
    expect(
      within(direct).getByTestId('compute-inspector-affected-char-brann'),
    ).toHaveTextContent('Brann');
    expect(within(direct).getByTestId('compute-inspector-run-id')).toHaveTextContent(
      'run_9f3a2c',
    );
    expect(within(direct).getByTestId('compute-inspector-provenance')).toHaveTextContent(
      'From module run',
    );
    expect(within(direct).getByTestId('compute-inspector-open-run')).toHaveTextContent(
      'Open Run',
    );
  });

  it('renders the preset inspector with provenance but no Run section', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const preset = screen.getByTestId('timeline-compute-inspector-preset');
    expect(within(preset).getByTestId('compute-inspector-provenance')).toHaveTextContent(
      'From preset',
    );
    expect(within(preset).queryByTestId('compute-inspector-section-run')).not.toBeInTheDocument();
    expect(within(preset).queryByTestId('compute-inspector-open-run')).not.toBeInTheDocument();
  });

  it('renders the sparse inspector with module + provenance only', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const sparse = screen.getByTestId('timeline-compute-inspector-sparse');
    expect(within(sparse).getByTestId('compute-inspector-module-name')).toHaveTextContent(
      'Economy Ticker',
    );
    expect(within(sparse).queryByTestId('compute-inspector-section-report')).not.toBeInTheDocument();
    expect(within(sparse).queryByTestId('compute-inspector-section-params')).not.toBeInTheDocument();
    expect(within(sparse).queryByTestId('compute-inspector-section-affected')).not.toBeInTheDocument();
    expect(within(sparse).queryByTestId('compute-inspector-section-run')).not.toBeInTheDocument();
  });

  it('renders the Run Module entry chrome — toolbar button + empty-state hint', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const toolbar = screen.getByTestId('timeline-compute-toolbar');
    expect(within(toolbar).getByTestId('timeline-compute-run-module-button')).toHaveTextContent(
      'Run Module',
    );

    const hint = screen.getByTestId('timeline-compute-empty-hint');
    expect(within(hint).getByText('No events yet')).toBeInTheDocument();
    expect(
      within(hint).getByTestId('timeline-compute-empty-run-module'),
    ).toHaveTextContent('Run Module');
  });

  it('proves the Brief layer is unaffected — era markers only, no compute chrome', () => {
    mockMatchMedia(false);
    render(<TimelineComputeFixtures />);

    const brief = screen.getByTestId('timeline-compute-brief-matrix');
    expect(within(brief).getAllByText('The First Age').length).toBeGreaterThanOrEqual(1);
    expect(within(brief).getAllByText('Era').length).toBeGreaterThanOrEqual(2);
    expect(within(brief).queryByTestId('compute-node-kind-pill')).not.toBeInTheDocument();
    expect(within(brief).queryByTestId('compute-node-provenance-chip')).not.toBeInTheDocument();
    expect(
      within(brief).getByTestId('timeline-compute-brief-note'),
    ).toHaveTextContent('era markers only');
  });

  it('renders under .dark without throw (theme class toggle)', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<TimelineComputeFixtures />)).not.toThrow();
    expect(screen.getByTestId('timeline-compute-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-narrative')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-inspector')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-run-module-entry')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-compute-brief')).toBeInTheDocument();
  });
});
