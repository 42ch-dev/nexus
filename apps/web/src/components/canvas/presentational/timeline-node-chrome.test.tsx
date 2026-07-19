/**
 * Timeline node body chrome — boundary + layer-accent rendering tests (V1.124).
 *
 * Boundary (architect contract studio-timeline-fixture-boundaries.md §5/§7):
 *   the extract MUST NOT import the React Flow package, the wire-contract
 *   package, or the i18n hook. App RF wrappers adapt node props → resolved
 *   props; Design Studio fixtures consume the chrome directly via
 *   `@web-canvas/*`.
 *
 * Rendering pins the V1.123 P4 layer-accent migration completed inside the
 * extract: Brief → brief-accent; Event/Narrative → narrative-accent;
 * Moment → moment-accent.
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import {
  TimelineBriefEraChrome,
  TimelineEventChrome,
  TimelineKeyBlockChrome,
  WorkTimelineMomentBeatChrome,
  WorkTimelineMomentSceneChrome,
  WorkTimelineNarrativeEventChrome,
} from './timeline-node-chrome';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const EXTRACT_SOURCE_PATH = path.resolve(__dirname, 'timeline-node-chrome.tsx');

// ---------------------------------------------------------------------------
// Boundary — no RF / contracts / i18n in the extract module
// ---------------------------------------------------------------------------

describe('timeline-node-chrome presentational boundary', () => {
  it('does not import the React Flow package', () => {
    const source = readFileSync(EXTRACT_SOURCE_PATH, 'utf8');
    // Task verify grep + boundary: no RF package specifier anywhere in the
    // extract (comments included — keep the file greppable-clean).
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
// Rendering — six chrome kinds + layer accents
// ---------------------------------------------------------------------------

describe('TimelineBriefEraChrome', () => {
  it('renders brief-accent on icon + time-span badge', () => {
    const { container } = render(
      <TimelineBriefEraChrome
        title="The First Age"
        blockTypeLabel="Era"
        timeSpan="1000 → 1100"
        temporalUnknownLabel="Era time span unknown"
        eraId="era-first"
        worldSummary="A time of myth."
        sourceAnchorLabel="2 source anchors"
        version={1}
      />,
    );
    expect(container.querySelector('.text-canvas-layer-brief-accent')).not.toBeNull();
    expect(
      container.querySelector('span.text-canvas-layer-brief-accent'),
    ).not.toBeNull();
    expect(screen.getByText('The First Age')).toBeInTheDocument();
    expect(screen.getByText('1000 → 1100')).toBeInTheDocument();
    expect(screen.getByText(/2 source anchors · v1/)).toBeInTheDocument();
  });

  it('renders temporal-unknown pill when timeSpan is null', () => {
    render(
      <TimelineBriefEraChrome
        title="Undated Era"
        blockTypeLabel="Era"
        timeSpan={null}
        temporalUnknownLabel="Era time span unknown"
        sourceAnchorLabel="0 source anchors"
        version={1}
      />,
    );
    expect(screen.getByText('Era time span unknown')).toBeInTheDocument();
  });
});

describe('TimelineEventChrome', () => {
  it('renders narrative-accent on dated badge (not worldkb-accent)', () => {
    const { container } = render(
      <TimelineEventChrome
        title="The Crossing"
        blockTypeLabel="Event"
        occurredAtHint="Year 42"
        temporalUnknownLabel="Temporal signal unknown"
        sourceAnchorLabel="1 source anchor"
        version={2}
      />,
    );
    expect(
      container.querySelector('.text-canvas-layer-narrative-accent'),
    ).not.toBeNull();
    expect(container.querySelector('.text-canvas-worldkb-accent')).toBeNull();
    expect(screen.getByText('Year 42')).toBeInTheDocument();
  });

  it('renders temporal-unknown when occurredAtHint is null', () => {
    render(
      <TimelineEventChrome
        title="Undated"
        blockTypeLabel="Event"
        occurredAtHint={null}
        temporalUnknownLabel="Temporal signal unknown"
        sourceAnchorLabel="0 source anchors"
        version={1}
      />,
    );
    expect(screen.getByText('Temporal signal unknown')).toBeInTheDocument();
  });
});

describe('TimelineKeyBlockChrome', () => {
  it('renders title + block-type + source meta without layer accent badge', () => {
    const { container } = render(
      <TimelineKeyBlockChrome
        title="Aria"
        blockTypeLabel="Character"
        sourceAnchorLabel="4 source anchors"
        version={3}
      />,
    );
    expect(screen.getByText('Aria')).toBeInTheDocument();
    expect(screen.getByText('Character')).toBeInTheDocument();
    expect(screen.getByText(/4 source anchors · v3/)).toBeInTheDocument();
    expect(container.querySelector('.text-canvas-layer-brief-accent')).toBeNull();
    expect(
      container.querySelector('.text-canvas-layer-narrative-accent'),
    ).toBeNull();
  });
});

describe('WorkTimelineNarrativeEventChrome', () => {
  it('renders narrative-accent on Flag + chapter badge', () => {
    const { container } = render(
      <WorkTimelineNarrativeEventChrome
        title="Inciting Incident"
        eventId="ev-1"
        chapterAnchor="Ch. 1"
        noChapterLabel="No chapter anchor"
        description="The hero leaves home."
      />,
    );
    expect(
      container.querySelector('.text-canvas-layer-narrative-accent'),
    ).not.toBeNull();
    expect(container.querySelector('.text-canvas-worldkb-accent')).toBeNull();
    expect(screen.getByText('Ch. 1')).toBeInTheDocument();
    expect(screen.getByText('The hero leaves home.')).toBeInTheDocument();
  });

  it('renders no-chapter pill when chapterAnchor is null', () => {
    render(
      <WorkTimelineNarrativeEventChrome
        title="Loose event"
        eventId="ev-2"
        chapterAnchor={null}
        noChapterLabel="No chapter anchor"
      />,
    );
    expect(screen.getByText('No chapter anchor')).toBeInTheDocument();
  });
});

describe('WorkTimelineMomentSceneChrome', () => {
  it('renders moment-accent on icon + manuscript-anchor badge', () => {
    const { container } = render(
      <WorkTimelineMomentSceneChrome
        title="Opening Scene"
        sceneId="sc-1"
        manuscriptAnchorLabel="Ch. 1 · sc-1"
        status="draft"
      />,
    );
    expect(
      container.querySelector('.text-canvas-layer-moment-accent'),
    ).not.toBeNull();
    expect(
      container.querySelector('span.text-canvas-layer-moment-accent'),
    ).not.toBeNull();
    expect(screen.getByText('sc-1')).toBeInTheDocument();
    expect(screen.getByText('draft')).toBeInTheDocument();
  });
});

describe('WorkTimelineMomentBeatChrome', () => {
  it('renders moment-accent on icon + manuscript-anchor badge', () => {
    const { container } = render(
      <WorkTimelineMomentBeatChrome
        title="Hook Beat"
        manuscriptAnchorLabel="Ch. 1 · sc-1 · bt-1"
      />,
    );
    expect(
      container.querySelector('.text-canvas-layer-moment-accent'),
    ).not.toBeNull();
    expect(screen.getByText('Ch. 1 · sc-1 · bt-1')).toBeInTheDocument();
  });
});
