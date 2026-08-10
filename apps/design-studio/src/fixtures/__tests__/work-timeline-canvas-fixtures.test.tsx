/**
 * Work Timeline Studio fixtures — smoke + boundary tests (V1.124 P0 T4;
 * V1.156 P2 T2 Brief layer + Brief empty-state).
 *
 * Acceptance evidence shapes (studio-fixture-acceptance-criteria.md §8):
 *   F1/F4 — fixture imports shared extract; no RF / contracts / daemon / i18n
 *   F3/F7 — frames + product vocabulary (Narrative / Moment / scene / beat /
 *           Brief / Era)
 *   F2/F5 — light + dark render without throw
 *   F9 — discoverable testids on Surfaces Canvas page section
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WorkTimelineCanvasFixtures } from '@/fixtures/work-timeline-canvas-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../work-timeline-canvas-fixtures.tsx',
);

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

describe('work-timeline-canvas-fixtures presentational boundary', () => {
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

  it('imports the shared timeline-node-chrome extract with transitional annotation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(
      /@web-canvas\/timeline-node-chrome.*transitional/,
    );
    expect(source).toMatch(/@web-canvas\/node-chrome-shell.*transitional/);
    expect(source).toMatch(/WorkTimelineNarrativeEventChrome/);
    expect(source).toMatch(/WorkTimelineMomentSceneChrome/);
    expect(source).toMatch(/WorkTimelineMomentBeatChrome/);
    // V1.156 — Work-Brief reuses the World Brief-era chrome (same node type).
    expect(source).toMatch(/TimelineBriefEraChrome/);
  });
});

// ---------------------------------------------------------------------------
// Rendering — three frames + product vocabulary + themes
// ---------------------------------------------------------------------------

describe('WorkTimelineCanvasFixtures render', () => {
  it('renders all six Work Timeline fixture frames without throw', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    expect(
      screen.getByTestId('work-timeline-canvas-fixtures'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-narrative-event'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-moment-scene'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-moment-beat'),
    ).toBeInTheDocument();
    // V1.156 — Brief layer + Brief empty-state frames.
    expect(
      screen.getByTestId('work-timeline-fixture-brief-layer'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-brief-empty'),
    ).toBeInTheDocument();
  });

  it('exposes product vocabulary labels (Narrative / Moment / scene / beat / Brief / Era)', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const root = screen.getByTestId('work-timeline-canvas-fixtures');
    expect(
      within(root).getByRole('heading', {
        name: 'Work Timeline — Narrative event',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'Work Timeline — Moment scene',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'Work Timeline — Moment beat',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'Work Timeline — Brief layer',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'Work Timeline — Brief empty-state',
      }),
    ).toBeInTheDocument();

    // Body chrome product terms (multi-variant → getAllByText)
    expect(
      within(root).getAllByText('The Crossing').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      within(root).getAllByText('No chapter anchor').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      within(root).getAllByText('Opening at the Gate').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(root).getAllByText('Hook Beat').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      within(root).getAllByText('Ch. 1 · sc-1').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      within(root).getAllByText('Ch. 1 · sc-1 · bt-1').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('covers Narrative chapter-anchor vs no-anchor and description variants', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const narrative = screen.getByTestId('work-timeline-narrative-event-matrix');
    expect(within(narrative).getAllByText('Ch. 3').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      within(narrative).getAllByText('No chapter anchor').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(narrative).getByText('Loose Rumor')).toBeInTheDocument();
    expect(
      within(narrative).getAllByText(
        'Kael leaves the Hearthstone road and crosses into the Ashen Gate.',
      ).length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(narrative).getByText('Silent Accord')).toBeInTheDocument();
  });

  it('covers Moment scene + beat manuscript-anchor variants (both frames required)', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const scenes = screen.getByTestId('work-timeline-moment-scene-matrix');
    expect(
      within(scenes).getAllByText('Ch. 1 · sc-1').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(scenes).getByText('Unanchored Scene')).toBeInTheDocument();
    expect(within(scenes).getAllByText('draft').length).toBeGreaterThanOrEqual(1);
    expect(within(scenes).getByText('sc-loose')).toBeInTheDocument();

    const beats = screen.getByTestId('work-timeline-moment-beat-matrix');
    expect(
      within(beats).getAllByText('Ch. 1 · sc-1 · bt-1').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(beats).getByText('Loose Beat')).toBeInTheDocument();
    expect(
      within(beats).getAllByText('Ch. 3 · sc-2 · bt-2').length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('covers the V1.156 Brief layer — bound-World era entities (fixture era data)', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const brief = screen.getByTestId('work-timeline-brief-layer-matrix');
    expect(
      within(brief).getAllByText('The First Age').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(brief).getAllByText('Era').length).toBeGreaterThanOrEqual(1);
    expect(
      within(brief).getAllByText('Year 0 → Year 412').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(brief).getByText('Year 412 →')).toBeInTheDocument();
    expect(within(brief).getByText('→ Year 900')).toBeInTheDocument();
    expect(within(brief).getByText('Uncharted Brief')).toBeInTheDocument();
    expect(
      within(brief).getAllByText('Temporal unknown').length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      within(brief).getByText(
        'Founding myths and the first knowledge entry lineages of the bound World.',
      ),
    ).toBeInTheDocument();
  });

  it('renders the V1.156 Brief empty-state with honest copy + Narrative CTA', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const empty = screen.getByTestId('work-timeline-fixture-brief-empty');
    expect(
      within(empty).getByText('No world-shape context yet'),
    ).toBeInTheDocument();
    expect(
      within(empty).getByText(
        'World-shape context appears here when this Work is bound to a World with era markers. Brief is a read-only projection of the bound World’s Brief.',
      ),
    ).toBeInTheDocument();
    // Escape hatch mirrors the app's BriefEmptyState CTA (no "create Brief").
    const panel = within(empty).getByTestId('work-timeline-brief-empty-state');
    const buttons = within(panel).queryAllByRole('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent('Switch to Narrative');
  });

  it('marks selected variants with canvas-node-border-selected', () => {
    mockMatchMedia(false);
    render(<WorkTimelineCanvasFixtures />);

    const narrative = screen.getByTestId('work-timeline-narrative-event-matrix');
    expect(
      narrative.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);

    const scenes = screen.getByTestId('work-timeline-moment-scene-matrix');
    expect(
      scenes.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);

    const beats = screen.getByTestId('work-timeline-moment-beat-matrix');
    expect(
      beats.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);

    // V1.156 — Brief layer matrix carries a selected era variant.
    const brief = screen.getByTestId('work-timeline-brief-layer-matrix');
    expect(
      brief.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('renders under .dark without throw (theme class toggle)', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<WorkTimelineCanvasFixtures />)).not.toThrow();
    expect(
      screen.getByTestId('work-timeline-canvas-fixtures'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-narrative-event'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-moment-scene'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-moment-beat'),
    ).toBeInTheDocument();
    // V1.156 frames render in dark too.
    expect(
      screen.getByTestId('work-timeline-fixture-brief-layer'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('work-timeline-fixture-brief-empty'),
    ).toBeInTheDocument();
  });
});
