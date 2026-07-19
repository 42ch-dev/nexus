/**
 * Work Timeline Studio fixtures — smoke + boundary tests (V1.124 P0 T4).
 *
 * Acceptance evidence shapes (studio-fixture-acceptance-criteria.md §8):
 *   F1/F4 — fixture imports shared extract; no RF / contracts / daemon / i18n
 *   F3/F7 — three frames + product vocabulary (Narrative / Moment / scene / beat)
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
  });
});

// ---------------------------------------------------------------------------
// Rendering — three frames + product vocabulary + themes
// ---------------------------------------------------------------------------

describe('WorkTimelineCanvasFixtures render', () => {
  it('renders all three Work Timeline fixture frames without throw', () => {
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
  });

  it('exposes product vocabulary labels (Narrative / Moment / scene / beat)', () => {
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
  });
});
