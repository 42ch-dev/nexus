/**
 * Global Timeline Studio fixtures — smoke + boundary tests (V1.124 P2 T2).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { GlobalTimelineFixtures } from '@/fixtures/global-timeline-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../global-timeline-fixtures.tsx',
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

describe('global-timeline-fixtures presentational boundary', () => {
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

  it('imports the shared extract with transitional annotation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(
      /@web-global-timeline\/global-timeline-list-chrome.*transitional/,
    );
  });
});

describe('GlobalTimelineFixtures render', () => {
  it('renders populated, empty, loading, and error frames', () => {
    mockMatchMedia(false);
    render(<GlobalTimelineFixtures />);

    expect(screen.getByTestId('global-timeline-fixtures')).toBeInTheDocument();
    expect(
      screen.getByTestId('global-timeline-fixture-populated'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('global-timeline-fixture-empty'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('global-timeline-fixture-loading'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('global-timeline-fixture-error'),
    ).toBeInTheDocument();
  });

  it('shows ≥3 World rows and product vocabulary', () => {
    mockMatchMedia(false);
    render(<GlobalTimelineFixtures />);

    const populated = screen.getByTestId('global-timeline-fixture-populated');
    expect(
      within(populated).getByRole('heading', {
        name: 'Global Timeline — populated',
      }),
    ).toBeInTheDocument();
    expect(within(populated).getAllByTestId('global-timeline-row').length).toBeGreaterThanOrEqual(
      3,
    );
    expect(within(populated).getByText('Ashen Gate Chronicles')).toBeInTheDocument();
    expect(within(populated).getByText('Hearthstone Cycle')).toBeInTheDocument();
    expect(within(populated).getByText(/Brief · 3 eras/)).toBeInTheDocument();
    expect(
      within(populated).getAllByText(/Narrative · 0 eras/).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it('exposes empty-state copy', () => {
    mockMatchMedia(false);
    render(<GlobalTimelineFixtures />);

    const empty = screen.getByTestId('global-timeline-fixture-empty');
    expect(within(empty).getByText('No Worlds yet')).toBeInTheDocument();
    expect(
      within(empty).getByText(/Create a World to start tracking Timeline/),
    ).toBeInTheDocument();
  });

  it('renders under .dark without throw', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<GlobalTimelineFixtures />)).not.toThrow();
    expect(screen.getByTestId('global-timeline-fixtures')).toBeInTheDocument();
  });
});
