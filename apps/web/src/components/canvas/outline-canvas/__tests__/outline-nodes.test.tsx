/**
 * Chapter card node — status pill AA text-step pins (v1.183 P0 QC2 F-002).
 *
 * Mirrors the scene-beat hue-1000 assertions in `scene-beat-nodes.test.tsx`
 * onto the chapter pill: `STATUS_TEXT_TOKEN_VAR` in `outline-nodes.tsx`
 * maps pending → gray-1000, draft → blue-1000, finalized → green-1000 so
 * label-12 pill text on the 12% status tint clears WCAG AA (the raw `*-700`
 * status color fails on light tints). A regression on the mapping must fail
 * these tests.
 *
 * `@xyflow/react`'s `Handle` needs the RF internal store; we stub it so the
 * node body renders in isolation (same recipe as scene-beat-nodes.test.tsx).
 */
import { describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';

import { renderInApp } from '@/test/test-providers';

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return {
    ...actual,
    Handle: ({ type: _type, position: _position }: { type: string; position: number; className?: string }) =>
      null as unknown as ReactNode,
  };
});

import { Handle, Position, type NodeProps } from '@xyflow/react';

import type { ChapterStatus } from '@42ch/nexus-contracts';

import { OutlineChapterNode } from '../outline-nodes';
import type { OutlineChapterNodeData } from '../rf-projection';

function chapterProps(
  data: Partial<OutlineChapterNodeData>,
  selected = false,
): NodeProps {
  return {
    id: 'chapter:test',
    type: 'outline-chapter',
    position: { x: 0, y: 0 },
    data: {
      workId: 'w1',
      chapterId: 1,
      volumeId: 1,
      title: 'Chapter One',
      slug: null,
      status: 'not_started',
      plannedWordCount: 2000,
      actualWordCount: null,
      ...data,
    } as OutlineChapterNodeData,
    selected,
    draggable: false,
    selectable: false,
    connectable: false,
    deletable: false,
    focusable: false,
    dragging: false,
    sourcePosition: Position.Right,
    targetPosition: Position.Left,
    width: 0,
    height: 0,
    depth: 0,
    extent: null,
    parentId: null,
    zIndex: 0,
    hidden: false,
    internalsSymbol: Symbol('test'),
    resizeObserver: null,
  } as unknown as NodeProps;
}

/** The status pill is the rounded-pill chip carrying the status label. */
function statusPill(container: HTMLElement): HTMLElement {
  const pill = container.querySelector('.rounded-pill.bg-gray-alpha-100') as HTMLElement;
  expect(pill).not.toBeNull();
  return pill;
}

describe('OutlineChapterNode status pill AA text step', () => {
  // pending → gray-1000 (not_started + outlined both map to pending).
  it.each(['not_started', 'outlined'] as ChapterStatus[])(
    'renders %s pill text on the gray-1000 AA step',
    (status) => {
      const { container } = renderInApp(
        <OutlineChapterNode {...chapterProps({ status })} />,
      );
      expect(statusPill(container).style.color).toBe('var(--color-gray-1000)');
    },
  );

  it('renders draft pill text on the blue-1000 AA step', () => {
    const { container } = renderInApp(
      <OutlineChapterNode {...chapterProps({ status: 'draft' })} />,
    );
    expect(statusPill(container).style.color).toBe('var(--color-blue-1000)');
  });

  // finalized → green-1000 (finalized + published both map to completed).
  it.each(['finalized', 'published'] as ChapterStatus[])(
    'renders %s pill text on the green-1000 AA step',
    (status) => {
      const { container } = renderInApp(
        <OutlineChapterNode {...chapterProps({ status })} />,
      );
      expect(statusPill(container).style.color).toBe('var(--color-green-1000)');
    },
  );

  it('keeps the status dot on the raw status color (not the AA text step)', () => {
    const { container } = renderInApp(
      <OutlineChapterNode {...chapterProps({ status: 'draft' })} />,
    );
    const dot = statusPill(container).querySelector('[aria-hidden]') as HTMLElement;
    expect(dot).not.toBeNull();
    expect(dot.style.background).toBe('var(--color-canvas-outline-chapter-card-status-drafted)');
  });
});

// Silence the unused-import lint for the Handle re-import (the mock above
// stubs Handle to null; Position is used in the fixture props).
void Handle;
