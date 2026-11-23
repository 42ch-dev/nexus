/**
 * FB-C2-001).
 *
 *
 * `@xyflow/react`'s `Handle` needs the RF internal store; we stub it so the
 * node body renders in isolation. `NodeProps` is passed through as-is.
 */
import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
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

import {
  OutlineSceneNode,
  OutlineBeatNode,
  type OutlineSceneNodeData,
  type OutlineBeatNodeData,
} from '../scene-beat-nodes';
import { outlineNodeTypes } from '../outline-nodes';

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

describe('OutlineSceneNode', () => {
  it('renders the scene title from node data', () => {
    renderInApp(<OutlineSceneNode {...sceneProps({ title: 'The Arrival' })} />);
    expect(screen.getByText('The Arrival')).toBeInTheDocument();
  });

  it('renders "Untitled Scene" fallback when title is empty', () => {
    renderInApp(<OutlineSceneNode {...sceneProps({ title: null })} />);
    expect(screen.getByText('Untitled Scene')).toBeInTheDocument();
  });

  it('renders a status chip when status is set', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'drafted' })} />,
    );
    expect(container.textContent).toContain('Drafted');
  });

  it('renders "Completed" status label for completed status', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'completed' })} />,
    );
    expect(container.textContent).toContain('Completed');
  });

  it('renders the status chip text on the hue-1000 AA step', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'drafted' })} />,
    );
    const chip = container.querySelector('.rounded-pill.bg-gray-alpha-100') as HTMLElement;
    expect(chip).not.toBeNull();
    expect(chip.style.color).toBe('var(--color-blue-1000)');
  });
  it('renders the completed chip text on the green-1000 AA step', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: 'completed' })} />,
    );
    const chip = container.querySelector('.rounded-pill.bg-gray-alpha-100') as HTMLElement;
    expect(chip).not.toBeNull();
    expect(chip.style.color).toBe('var(--color-green-1000)');
  });

  it('omits the status chip when status is null', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1', status: null })} />,
    );
    expect(container.textContent).not.toContain('Drafted');
    expect(container.textContent).not.toContain('Completed');
  });

  it('consumes the canvas-outline-scene-fill token on the node body', () => {
    const { container } = renderInApp(
      <OutlineSceneNode {...sceneProps({ title: 'S1' })} />,
    );
    const nodeEl = container.firstElementChild as HTMLElement;
    expect(nodeEl.style.background).toContain('var(--color-canvas-outline-scene-fill)');
  });
});

describe('OutlineBeatNode', () => {
  it('renders the beat title from node data', () => {
    renderInApp(<OutlineBeatNode {...beatProps({ title: 'Turn: the call' })} />);
    expect(screen.getByText('Turn: the call')).toBeInTheDocument();
  });

  it('renders "Untitled Beat" fallback when title is empty', () => {
    renderInApp(<OutlineBeatNode {...beatProps({ title: null })} />);
    expect(screen.getByText('Untitled Beat')).toBeInTheDocument();
  });

  it('consumes the canvas-outline-beat-fill token on the node body', () => {
    const { container } = renderInApp(
      <OutlineBeatNode {...beatProps({ title: 'B1' })} />,
    );
    const nodeEl = container.firstElementChild as HTMLElement;
    expect(nodeEl.style.background).toContain('var(--color-canvas-outline-beat-fill)');
  });
});

describe('outlineNodeTypes registration', () => {
  it('registers outline-scene and outline-beat in the node types map', () => {
    expect(outlineNodeTypes['outline-scene']).toBeDefined();
    expect(outlineNodeTypes['outline-beat']).toBeDefined();
  });
});

void Handle;
