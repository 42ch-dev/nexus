/**
 * NleTimelineChrome — boundary + multi-track rendering tests (V1.128 P1 T1).
 *
 * Boundary: the extract MUST NOT import `@xyflow/react` (architect lock).
 * Rendering: ≥2 labeled tracks, centered band host, horizontal scroll region.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import {
  NLE_TIMELINE_DEMO_TRACKS,
  NleTimelineChrome,
  type NleTimelineTrack,
} from './nle-timeline-chrome';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const EXTRACT_SOURCE_PATH = path.resolve(__dirname, 'nle-timeline-chrome.tsx');

const TWO_TRACK_FIXTURE: NleTimelineTrack[] = [
  {
    id: 'brief',
    label: 'Brief',
    accent: 'brief',
    clips: [{ id: 'c1', label: 'Era clip', startPx: 20, widthPx: 120 }],
  },
  {
    id: 'narrative',
    label: 'Narrative',
    accent: 'narrative',
    clips: [{ id: 'c2', label: 'Event clip', startPx: 80, widthPx: 100 }],
  },
];

// ---------------------------------------------------------------------------
// Boundary — no RF / contracts / i18n in the extract module
// ---------------------------------------------------------------------------

describe('nle-timeline-chrome presentational boundary', () => {
  it('does not import the React Flow package', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    expect(source).not.toMatch(/@xyflow\/react/);
  });

  it('does not import the wire-contract package', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    expect(source).not.toMatch(/@42ch\/nexus-contracts/);
  });

  it('does not import the i18n hook or react-i18next', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    expect(source).not.toMatch(/useTranslation/);
    expect(source).not.toMatch(/react-i18next/);
  });

  it('import lines stay free of RF, contracts, and i18n modules', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    const importLines = source.split('\n').filter((l) => /^\s*import\b/.test(l));
    for (const line of importLines) {
      expect(line).not.toMatch(/xyflow/);
      expect(line).not.toMatch(/nexus-contracts/);
      expect(line).not.toMatch(/i18next/);
    }
  });
});

// ---------------------------------------------------------------------------
// Rendering — multi-track band + horizontal scrub region
// ---------------------------------------------------------------------------

describe('NleTimelineChrome rendering', () => {
  it('renders ≥2 labeled tracks in a vertically centered band host', () => {
    render(<NleTimelineChrome tracks={TWO_TRACK_FIXTURE} contentWidthPx={800} />);

    expect(screen.getByTestId('nle-timeline-chrome')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-band')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-label-brief')).toHaveTextContent('Brief');
    expect(screen.getByTestId('nle-timeline-label-narrative')).toHaveTextContent(
      'Narrative',
    );
    expect(screen.getByTestId('nle-timeline-lane-brief')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-lane-narrative')).toBeInTheDocument();
  });

  it('exposes a horizontal scroll region for scrub/pan along time', () => {
    render(
      <NleTimelineChrome
        tracks={TWO_TRACK_FIXTURE}
        contentWidthPx={1200}
        scrollAriaLabel="Scrub timeline"
      />,
    );

    const scroll = screen.getByTestId('nle-timeline-scroll');
    expect(scroll).toHaveAttribute('role', 'region');
    expect(scroll).toHaveAttribute('aria-label', 'Scrub timeline');
    expect(scroll.className).toMatch(/overflow-x-auto/);
  });

  it('renders clip blocks and optional playhead', () => {
    render(
      <NleTimelineChrome
        tracks={NLE_TIMELINE_DEMO_TRACKS}
        contentWidthPx={1400}
        playheadPx={400}
      />,
    );

    expect(screen.getByTestId('nle-timeline-clip-era-1')).toHaveTextContent(
      'The First Age',
    );
    expect(screen.getByTestId('nle-timeline-clip-ev-1')).toHaveTextContent(
      'The Crossing',
    );
    expect(screen.getByTestId('nle-timeline-playhead')).toBeInTheDocument();
    expect(screen.getByTestId('nle-timeline-ruler')).toBeInTheDocument();
  });

  it('applies layer accent classes on track labels', () => {
    const { container } = render(
      <NleTimelineChrome tracks={TWO_TRACK_FIXTURE} contentWidthPx={600} />,
    );
    expect(
      container.querySelector('.text-canvas-layer-brief-accent'),
    ).not.toBeNull();
    expect(
      container.querySelector('.text-canvas-layer-narrative-accent'),
    ).not.toBeNull();
  });

  it('renders detach affordance when detachableClipIds and onClipDetach are set', () => {
    const onDetach = vi.fn();
    render(
      <NleTimelineChrome
        tracks={TWO_TRACK_FIXTURE}
        contentWidthPx={600}
        detachableClipIds={new Set(['c2'])}
        onClipDetach={onDetach}
      />,
    );

    const detachButton = screen.getByTestId('nle-timeline-clip-detach-c2');
    expect(detachButton).toHaveAttribute('aria-label', 'Detach Event clip from track');
    expect(screen.queryByTestId('nle-timeline-clip-detach-c1')).not.toBeInTheDocument();

    fireEvent.click(detachButton);
    expect(onDetach).toHaveBeenCalledWith('narrative', {
      id: 'c2',
      label: 'Event clip',
      startPx: 80,
      widthPx: 100,
    });
  });
});
