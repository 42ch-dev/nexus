/**
 * SceneNode + BeatNode — rendering + DESIGN token consumption (V1.109 C2 T1;
 * FB-C2-001).
 *
 * These components are new in T1 but are NOT wired into the projection yet
 * (Task 2 extends `rf-projection.ts` to emit them). These tests pin their
 * standalone rendering behavior: title + status from node data, Voice & Content
 * fallback labels (**Untitled Scene** / **Untitled Beat**), and consumption of
 * the `canvas-outline-scene-*` / `canvas-outline-beat-*` DESIGN tokens.
 *
 * `@xyflow/react`'s `Handle` needs the RF internal store; we stub it so the
 * node body renders in isolation. `NodeProps` is passed through as-is.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';

// Stub Handle so the node body renders without the RF store. `NodeProps` is
// re-exported from the real module so the component's type imports resolve.
vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return {
    ...actual,
    Handle: ({ type: _type, position: _position }: { type: string; position: number; className?: string }) =>
      null as unknown as ReactNode,
  };
});

import { Handle, Position, type NodeProps } from '@xyflow/react';

import {
  OutlineSceneNode,
  OutlineBeatNode,
  type OutlineSceneNodeData,
  type OutlineBeatNodeData,
} from '../scene-beat-nodes';
import { outlineNodeTypes } from '../outline-nodes';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function sceneProps(
  data: Partial<OutlineSceneNodeData>,
  selected = false,
): NodeProps {
  return {
    id: 'scene:test',
    type: 'outline-scene',
    position: { x: 0, y: 0 },
    data: { title: null, status: null, ...data } as OutlineSceneNodeData,
    selected,
    draggable: false,
    selectable: false,
    connectable: false,
    deletable: false,
    focusable: false,
    dragging: false,
    sourcePosition: Position.Left,
    targetPosition: Position.Right,
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

function beatProps(
  data: Partial<OutlineBeatNodeData>,
  selected = false,
): NodeProps {
  return {
    id: 'beat:test',
    type: 'outline-beat',
    position: { x: 0, y: 0 },
    data: { title: null, ...data } as OutlineBeatNodeData,
    selected,
  } as unknown as NodeProps;
}

// ---------------------------------------------------------------------------
// SceneNode
// ---------------------------------------------------------------------------

describe('OutlineSceneNode', () => {
  it('renders the scene title from node data', () => {
    render(<OutlineSceneNode {...sceneProps({ title: 'The Arrival' })} />);
    expect(screen.getByText('The Arrival')).toBeInTheDocument();
  });

  it('renders "Untitled Scene" fallback when title is empty', () => {
    render(<OutlineSceneNode {...sceneProps({ title: null })} />);
    expect(screen.getByText('Untitled Scene')).toBeInTheDocument();
  });

  it('renders a status chip when status is set', () => {
    const { container } = render(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'drafted' })} />,
    );
    expect(container.textContent).toContain('Drafted');
  });

  it('renders "Completed" status label for completed status', () => {
    const { container } = render(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'completed' })} />,
    );
    expect(container.textContent).toContain('Completed');
  });

  it('omits the status chip when status is null', () => {
    const { container } = render(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: null })} />,
    );
    expect(container.textContent).not.toContain('Drafted');
    expect(container.textContent).not.toContain('Completed');
  });

  it('consumes the canvas-outline-scene-fill token on the node body', () => {
    const { container } = render(
      <OutlineSceneNode {...sceneProps({ title: 'S1' })} />,
    );
    const nodeEl = container.firstElementChild as HTMLElement;
    expect(nodeEl.style.background).toContain('var(--color-canvas-outline-scene-fill)');
  });
});

// ---------------------------------------------------------------------------
// BeatNode
// ---------------------------------------------------------------------------

describe('OutlineBeatNode', () => {
  it('renders the beat title from node data', () => {
    render(<OutlineBeatNode {...beatProps({ title: 'Turn: the call' })} />);
    expect(screen.getByText('Turn: the call')).toBeInTheDocument();
  });

  it('renders "Untitled Beat" fallback when title is empty', () => {
    render(<OutlineBeatNode {...beatProps({ title: null })} />);
    expect(screen.getByText('Untitled Beat')).toBeInTheDocument();
  });

  it('consumes the canvas-outline-beat-fill token on the node body', () => {
    const { container } = render(
      <OutlineBeatNode {...beatProps({ title: 'B1' })} />,
    );
    const nodeEl = container.firstElementChild as HTMLElement;
    expect(nodeEl.style.background).toContain('var(--color-canvas-outline-beat-fill)');
  });
});

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

describe('outlineNodeTypes registration', () => {
  it('registers outline-scene and outline-beat in the node types map', () => {
    expect(outlineNodeTypes['outline-scene']).toBeDefined();
    expect(outlineNodeTypes['outline-beat']).toBeDefined();
  });
});

// Silence the unused-import lint for Handle/Position re-import (the mock above
// stubs Handle to null; Position is used in the fixture props).
void Handle;
