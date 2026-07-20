/**
 * NLE Timeline Studio fixtures — smoke + boundary tests (V1.128 P1 T1).
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { render, screen, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { NleTimelineCanvasFixtures } from '@/fixtures/nle-timeline-canvas-fixtures';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const FIXTURE_SOURCE_PATH = path.resolve(
  __dirname,
  '../nle-timeline-canvas-fixtures.tsx',
);
const EXTRACT_SOURCE_PATH = path.resolve(
  __dirname,
  '../../../../web/src/components/canvas/presentational/nle-timeline-chrome.tsx',
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
// Boundary — fixture + extract stay RF / contracts / daemon / i18n free
// ---------------------------------------------------------------------------

describe('nle-timeline-canvas-fixtures presentational boundary', () => {
  it('does not import @xyflow/react, contracts, daemon clients, or useTranslation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.join('\n')).not.toMatch(/xyflow/);
    expect(imports.join('\n')).not.toMatch(/nexus-contracts/);
    expect(imports.join('\n')).not.toMatch(/useTranslation/);
    expect(imports.join('\n')).not.toMatch(/lib\/nexus/);
    expect(imports.join('\n')).not.toMatch(/NexusClient/);
  });

  it('imports the shared nle-timeline-chrome extract with transitional annotation', () => {
    const source = readFileSync(FIXTURE_SOURCE_PATH, 'utf8');
    expect(source).toMatch(/@web-canvas\/nle-timeline-chrome.*transitional/);
    expect(source).toMatch(/NleTimelineChrome/);
  });

  it('extract module does not import @xyflow/react', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    expect(source).not.toMatch(/@xyflow\/react/);
  });
});

// ---------------------------------------------------------------------------
// Rendering — multi-track band + themes
// ---------------------------------------------------------------------------

describe('NleTimelineCanvasFixtures render', () => {
  it('renders NLE multi-track band fixture without throw', () => {
    mockMatchMedia(false);
    render(<NleTimelineCanvasFixtures />);

    expect(screen.getByTestId('nle-timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-fixture-band')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-chrome')).toBeInTheDocument();
  });

  it('shows ≥2 labeled tracks and horizontal scrub region', () => {
    mockMatchMedia(false);
    render(<NleTimelineCanvasFixtures />);

    const root = screen.getByTestId('nle-timeline-canvas-fixtures');
    expect(within(root).getByTestId('nle-timeline-label-brief')).toHaveTextContent(
      'Brief',
    );
    expect(
      within(root).getByTestId('nle-timeline-label-narrative'),
    ).toHaveTextContent('Narrative');
    expect(within(root).getByTestId('nle-timeline-label-moment')).toHaveTextContent(
      'Moment',
    );
    expect(within(root).getByTestId('nle-timeline-scroll')).toBeInTheDocument();
    expect(within(root).getByText('The Crossing')).toBeInTheDocument();
  });

  it('renders under .dark without throw (theme class toggle)', () => {
    mockMatchMedia(true);
    document.documentElement.classList.add('dark');
    expect(() => render(<NleTimelineCanvasFixtures />)).not.toThrow();
    expect(screen.getByTestId('nle-timeline-canvas-fixtures')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-chrome')).toBeInTheDocument();
  });
});
