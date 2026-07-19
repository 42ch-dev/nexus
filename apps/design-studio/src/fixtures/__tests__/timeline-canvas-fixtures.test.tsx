/**
 * World Timeline Studio fixtures — smoke + boundary tests (V1.124 P0 T3).
 *
 * Acceptance evidence shapes (studio-fixture-acceptance-criteria.md §8):
 *   F1/F4 — fixture imports shared extract; no RF / contracts / daemon / i18n
 *   F3/F7 — three frames + product vocabulary (Brief / Event / KeyBlock / Timeline)
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
  });
});

// ---------------------------------------------------------------------------
// Rendering — three frames + product vocabulary + themes
// ---------------------------------------------------------------------------

describe('TimelineCanvasFixtures render', () => {
  it('renders all three World Timeline fixture frames without throw', () => {
    mockMatchMedia(false);
    render(<TimelineCanvasFixtures />);

    expect(screen.getByTestId('timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-brief-era')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-event')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-key-block')).toBeInTheDocument();
  });

  it('exposes product vocabulary labels (Brief / Event / KeyBlock / Timeline)', () => {
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
        'Founding myths and the first KeyBlock lineages of the World.',
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
  });

  it('renders under .dark without throw (theme class toggle)', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<TimelineCanvasFixtures />)).not.toThrow();
    expect(screen.getByTestId('timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-brief-era')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-event')).toBeInTheDocument();
    expect(screen.getByTestId('timeline-fixture-key-block')).toBeInTheDocument();
  });
});
