/**
 * World Timeline Studio fixtures — smoke + boundary tests (V1.124 P0 T3;
 * V1.156 P1 T2 Moment layer + Moment empty-state).
 *
 * Acceptance evidence shapes (studio-fixture-acceptance-criteria.md §8):
 *   F1/F4 — fixture imports shared extract; no RF / contracts / daemon / i18n
 *   F3/F7 — frames + product vocabulary (Brief / Event / KeyBlock / Moment /
 *           Timeline)
 *   F2/F5 — light + dark render without throw
 *   F9 — discoverable testids on Surfaces Canvas page section
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TimelineCanvasFixtures } from '@/fixtures/timeline-canvas-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../timeline-canvas-fixtures.tsx',
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

describe('timeline-canvas-fixtures presentational boundary', () => {
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
    expect(source).toMatch(/TimelineBriefEraChrome/);
    expect(source).toMatch(/TimelineEventChrome/);
    expect(source).toMatch(/TimelineKeyBlockChrome/);
    // V1.156 — World Timeline Moment layer reuses the Work Moment chrome.
    expect(source).toMatch(/WorkTimelineMomentSceneChrome/);
    expect(source).toMatch(/WorkTimelineMomentBeatChrome/);
  });
});

// ---------------------------------------------------------------------------
// Rendering — three frames + product vocabulary + themes
// ---------------------------------------------------------------------------

describe('TimelineCanvasFixtures render', () => {
  it('renders all six World Timeline fixture frames without throw', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    expect(screen.getByTestId('timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-brief-era')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-event')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-key-block')).toBeInTheDocument();
    // V1.156 — Moment layer + Moment empty-state frames.
    expect(screen.getByTestId('timeline-fixture-moment-layer')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-moment-empty')).toBeInTheDocument();
  });

  it('exposes product vocabulary labels (Brief / Event / KeyBlock / Moment / Timeline)', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const root = screen.getByTestId('timeline-canvas-fixtures');
    expect(
      within(root).getByRole('heading', {
        name: 'World Timeline — Brief-era',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'World Timeline — Event',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'World Timeline — KeyBlock Context cluster',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'World Timeline — Moment layer',
      }),
    ).toBeInTheDocument();
    expect(
      within(root).getByRole('heading', {
        name: 'World Timeline — Moment empty-state',
      }),
    ).toBeInTheDocument();

    // Body chrome product terms
    expect(within(root).getAllByText('Era').length).toBeGreaterThanOrEqual(1);
    expect(within(root).getAllByText('Event').length).toBeGreaterThanOrEqual(1);
    expect(within(root).getAllByText('Character').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      within(root).getAllByText('Organization').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(root).getAllByText('The Crossing').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(within(root).getAllByText('Temporal unknown').length).toBeGreaterThanOrEqual(
      1,
    );
  });

  it('covers Brief-era time-span variants including temporal-unknown', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const brief = screen.getByTestId('timeline-brief-era-matrix');
    expect(within(brief).getAllByText('Year 0 → Year 412').length).toBeGreaterThanOrEqual(1);
    expect(within(brief).getByText('Year 412 →')).toBeInTheDocument();
    expect(within(brief).getByText('→ Year 900')).toBeInTheDocument();
    expect(within(brief).getAllByText('Temporal unknown').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      within(brief).getByText(
        'Founding myths and the first knowledge entry lineages of the World.',
      ),
    ).toBeInTheDocument();
  });

  it('covers Event dated vs temporal-unknown and KeyBlock block-type diversity', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const events = screen.getByTestId('timeline-event-matrix');
    expect(within(events).getAllByText('Year 412 · spring').length).toBeGreaterThanOrEqual(1);
    expect(within(events).getAllByText('Temporal unknown').length).toBeGreaterThanOrEqual(
      1,
    );

    const keyBlocks = screen.getByTestId('timeline-key-block-matrix');
    expect(within(keyBlocks).getAllByText('Kael Veynor').length).toBeGreaterThanOrEqual(1);
    expect(within(keyBlocks).getAllByText('Hearthstone Covenant').length).toBeGreaterThanOrEqual(1);
    expect(within(keyBlocks).getByText('Ashen Gate')).toBeInTheDocument();
  });

  it('covers the V1.156 Moment layer — scene + beat chrome from the sceneBeatFixture slot', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const moment = screen.getByTestId('timeline-moment-layer-matrix');
    expect(
      within(moment).getAllByText('Arrival at the Ashen Gate').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(moment).getAllByText('sc-1').length).toBeGreaterThanOrEqual(1);
    expect(within(moment).getByText('Unanchored Passage')).toBeInTheDocument();
    expect(within(moment).getByText('sc-loose')).toBeInTheDocument();
    expect(
      within(moment).getAllByText('Ch. 1 · sc-1').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(moment).getByText('Hook Beat')).toBeInTheDocument();
    expect(
      within(moment).getAllByText('Ch. 1 · sc-1 · bt-1').length,
    ).toBeGreaterThanOrEqual(1);
    expect(within(moment).getByText('Loose Beat')).toBeInTheDocument();
    expect(within(moment).getByText('Turn Beat')).toBeInTheDocument();
  });

  it('renders the V1.156 Moment empty-state with honest copy + Narrative CTA', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const empty = screen.getByTestId('timeline-fixture-moment-empty');
    expect(
      within(empty).getByText('No scene or beat data yet'),
    ).toBeInTheDocument();
    expect(
      within(empty).getByText(
        'Scene-precision is available when bound Works have scene/beat data in their Outline. Add scenes and beats to a bound Work, or switch to Narrative for events.',
      ),
    ).toBeInTheDocument();
    // Escape hatch mirrors the app's MomentEmptyState CTA (no "create Moment").
    const panel = within(empty).getByTestId('timeline-moment-empty-state');
    const buttons = within(panel).queryAllByRole('button');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveTextContent('Switch to Narrative');
  });

  it('marks selected variants with canvas-node-border-selected', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    const brief = screen.getByTestId('timeline-brief-era-matrix');
    // Selected Brief-era shares title with default; find selected shell via class.
    const selectedShells = brief.querySelectorAll(
      '[class*="border-canvas-node-border-selected"]',
    );
    expect(selectedShells.length).toBeGreaterThanOrEqual(1);

    const events = screen.getByTestId('timeline-event-matrix');
    expect(
      events.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);

    const keyBlocks = screen.getByTestId('timeline-key-block-matrix');
    expect(
      keyBlocks.querySelectorAll(
        '[class*="border-canvas-node-border-selected"]',
      ).length,
    ).toBeGreaterThanOrEqual(1);

    // V1.156 — Moment layer matrix carries a selected scene variant.
    const moment = screen.getByTestId('timeline-moment-layer-matrix');
    expect(
      moment.querySelectorAll('[class*="border-canvas-node-border-selected"]')
        .length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('renders under .dark without throw (theme class toggle)', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<TimelineCanvasFixtures />)).not.toThrow();
    expect(screen.getByTestId('timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-brief-era')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-event')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-key-block')).toBeInTheDocument();
    // V1.156 frames render in dark too.
    expect(
      screen.getByTestId('timeline-fixture-moment-layer'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('timeline-fixture-moment-empty'),
    ).toBeInTheDocument();
  });
});
