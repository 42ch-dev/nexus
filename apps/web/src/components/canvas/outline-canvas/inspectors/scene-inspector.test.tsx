/**
 * SceneInspector — read-only Scene detail panel (V1.109 C2 T3; FB-C2-002).
 *
 * The inspector is read-only (no write wire — `wire_contracts_changed: false`).
 * These tests pin the locked Voice & Content strings: heading **Scene**,
 * status field label **Status**, parent helper *Part of {chapter_title}.*,
 * read-only banner *Scene details are view-only for now.*
 */
import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';

import { SceneInspector } from './scene-inspector';
import type { OutlineSceneStatus } from '../rf-projection';

function scene(partial: {
  sceneId?: string;
  title?: string | null;
  status?: OutlineSceneStatus | null;
} = {}) {
  return {
    sceneId: 's1',
    title: 'The Arrival',
    status: 'drafted' as OutlineSceneStatus,
    ...partial,
  };
}

describe('SceneInspector', () => {
  it('renders the locked "Scene" heading', () => {
    render(<SceneInspector scene={scene()} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Scene')).toBeInTheDocument();
  });

  it('renders the scene title from data', () => {
    render(<SceneInspector scene={scene({ title: 'The Arrival' })} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('The Arrival')).toBeInTheDocument();
  });

  it('renders the "Status" field label with the status value', () => {
    render(<SceneInspector scene={scene({ status: 'drafted' })} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.getByText('Drafted')).toBeInTheDocument();
  });

  it('renders "Completed" status label for completed status', () => {
    render(<SceneInspector scene={scene({ status: 'completed' })} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Completed')).toBeInTheDocument();
  });

  it('omits a status value when status is null but keeps the Status label', () => {
    render(<SceneInspector scene={scene({ status: null })} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Status')).toBeInTheDocument();
    expect(screen.queryByText('Drafted')).not.toBeInTheDocument();
    expect(screen.queryByText('Completed')).not.toBeInTheDocument();
  });

  it('renders the parent chapter helper with the real chapter title', () => {
    render(<SceneInspector scene={scene()} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Part of Chapter One.')).toBeInTheDocument();
  });

  it('renders the locked read-only banner', () => {
    render(<SceneInspector scene={scene()} parentChapterTitle="Chapter One" />);
    expect(screen.getByText('Scene details are view-only for now.')).toBeInTheDocument();
  });

  it('renders a select prompt when no scene is selected', () => {
    render(<SceneInspector scene={null} parentChapterTitle={null} />);
    // The empty state must not paint scene-specific chrome.
    expect(screen.queryByText('Scene details are view-only for now.')).not.toBeInTheDocument();
    expect(screen.queryByText('Part of')).not.toBeInTheDocument();
  });
});
