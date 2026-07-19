/**
 * Layer breadcrumb Studio fixtures — smoke + boundary tests (V1.124 P2 T3a).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LayerBreadcrumbFixtures } from '@/fixtures/layer-breadcrumb-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../layer-breadcrumb-fixtures.tsx',
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

describe('layer-breadcrumb-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
  });

  it('imports the shared extract with transitional annotation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/@web-canvas\/layer-breadcrumb.*transitional/);
  });
});

describe('LayerBreadcrumbFixtures render', () => {
  it('renders World and Work Timeline matrices', () => {
    mockMatchMedia(false);
    render(<LayerBreadcrumbFixtures />);

    expect(screen.getByTestId('layer-breadcrumb-fixtures')).toBeInTheDocument();
    expect(
      screen.getByTestId('layer-breadcrumb-fixture-world'),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId('layer-breadcrumb-fixture-work'),
    ).toBeInTheDocument();
  });

  it('exposes product vocabulary Brief / Narrative / Moment', () => {
    mockMatchMedia(false);
    render(<LayerBreadcrumbFixtures />);

    const root = screen.getByTestId('layer-breadcrumb-fixtures');
    expect(within(root).getAllByText('Brief').length).toBeGreaterThanOrEqual(1);
    expect(within(root).getAllByText('Narrative').length).toBeGreaterThanOrEqual(
      1,
    );
    expect(within(root).getAllByText('Moment').length).toBeGreaterThanOrEqual(1);
  });

  it('marks active segment with aria-current=page', () => {
    mockMatchMedia(false);
    render(<LayerBreadcrumbFixtures />);

    const worldPath = screen.getByTestId(
      'fixture-world-path-layer-breadcrumb-segment-narrative',
    );
    expect(worldPath).toHaveAttribute('aria-current', 'page');

    const workPath = screen.getByTestId(
      'fixture-work-path-layer-breadcrumb-segment-moment',
    );
    expect(workPath).toHaveAttribute('aria-current', 'page');
  });

  it('zooms out when parent segment is clicked (interactive World sample)', () => {
    mockMatchMedia(false);
    render(<LayerBreadcrumbFixtures />);

    const parent = screen.getByTestId(
      'fixture-world-live-layer-breadcrumb-segment-brief',
    );
    expect(parent.tagName).toBe('BUTTON');
    fireEvent.click(parent);
    // After zoom-out, Brief is the only segment (active, non-button).
    const briefOnly = screen.getByTestId(
      'fixture-world-live-layer-breadcrumb-segment-brief',
    );
    expect(briefOnly).toHaveAttribute('aria-current', 'page');
    expect(briefOnly.tagName).toBe('SPAN');
  });

  it('renders under .dark without throw', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<LayerBreadcrumbFixtures />)).not.toThrow();
    expect(screen.getByTestId('layer-breadcrumb-fixtures')).toBeInTheDocument();
  });
});
