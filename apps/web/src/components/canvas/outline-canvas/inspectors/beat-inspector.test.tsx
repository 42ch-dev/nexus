/**
 * BeatInspector — read-only Beat detail panel (V1.109 C2 T3; FB-C2-002).
 *
 * The inspector is read-only (no write wire — `wire_contracts_changed: false`).
 * These tests pin the locked Voice & Content strings: heading **Beat**,
 * status field label **Status**, parent helper *Part of {scene_title}.*,
 * read-only banner *Beat details are view-only for now.*
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { BeatInspector } from './beat-inspector';
import type { OutlineSceneStatus } from '../rf-projection';

function beat(partial: {
  beatId?: string;
  title?: string | null;
  status?: OutlineSceneStatus | null;
} = {}) {
  return {
    beatId: 'b1',
    title: 'Turn: the call',
    status: 'drafted' as OutlineSceneStatus,
    ...partial,
  };
}

describe('BeatInspector', () => {
  it('renders the locked "Beat" heading', () => {
    render(<BeatInspector beat={beat()} parentSceneTitle="The Arrival" />);
    expect(screen.getByText('Beat')).toBeInTheDocument();
  });

  it('renders the beat title from data', () => {
    render(<BeatInspector beat={beat({ title: 'Turn: the call' })} parentSceneTitle="The Arrival" />);
    expect(screen.getByText('Turn: the call')).toBeInTheDocument();
  });

  it('renders the "Status" field label with the status value', () => {
    render(<BeatInspector beat={beat({ status: 'drafted' })} parentSceneTitle="The Arrival" />);
    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Drafted')).toBeInTheDocument();
  });

  it('renders the parent scene helper with the real scene title', () => {
    render(<BeatInspector beat={beat()} parentSceneTitle="The Arrival" />);
    expect(screen.getByText('Part of The Arrival.')).toBeInTheDocument();
  });

  it('renders the locked read-only banner', () => {
    render(<BeatInspector beat={beat()} parentSceneTitle="The Arrival" />);
    expect(screen.getByText('Beat details are view-only for now.')).toBeInTheDocument();
  });

  it('renders a select prompt when no beat is selected', () => {
    render(<BeatInspector beat={null} parentSceneTitle={null} />);
    expect(screen.queryByText('Beat details are view-only for now.')).not.toBeInTheDocument();
    expect(screen.queryByText('Part of')).not.toBeInTheDocument();
  });
});
