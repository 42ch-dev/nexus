/**
 * `useAutoLayout` — dagre-powered auto-layout with manual-override semantics.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import type { Edge, Node } from '@xyflow/react';

import { useAutoLayout } from '../use-auto-layout';

describe('useAutoLayout — dagre integration', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function makeNodes(): Node<{ label: string }>[] {
    return [
      { id: 'a', type: 'default', position: { x: 0, y: 0 }, data: { label: 'A' } },
      { id: 'b', type: 'default', position: { x: 0, y: 0 }, data: { label: 'B' } },
      { id: 'c', type: 'default', position: { x: 0, y: 0 }, data: { label: 'C' } },
    ];
  }

  function makeEdges(): Edge[] {
    return [
      { id: 'e1', source: 'a', target: 'b' },
      { id: 'e2', source: 'b', target: 'c' },
    ];
  }

  it('passes nodes through unchanged when layoutOptions is undefined', () => {
    const nodes = makeNodes();
    const edges = makeEdges();
    const { result } = renderHook(() => useAutoLayout(nodes, edges));

    expect(result.current.nodes).toBe(nodes);
    expect(result.current.isLayouting).toBe(false);
    expect(result.current.relayout).toBeTypeOf('function');
  });

  it('computes non-zero, distinct positions on initial layout', () => {
    const nodes = makeNodes();
    const edges = makeEdges();
    const { result } = renderHook(() => useAutoLayout(nodes, edges, { direction: 'TB' }));

    const positions = result.current.nodes.map((n) => n.position);
    expect(positions.some((p) => p.x !== 0 || p.y !== 0)).toBe(true);
    // Top-to-bottom layout should place nodes on different y ranks.
    const yValues = new Set(positions.map((p) => p.y));
    expect(yValues.size).toBeGreaterThanOrEqual(2);
  });

  it('preserves a manually dragged position and suppresses re-layout', () => {
    const nodes = makeNodes();
    const edges = makeEdges();
    const { result, rerender } = renderHook(
      ({ nodes, edges }) => useAutoLayout(nodes, edges, { direction: 'TB' }),
      { initialProps: { nodes, edges } },
    );

    const laidOut = result.current.nodes.map((n) => ({ id: n.id, position: n.position }));
    const laidOutB = laidOut.find((n) => n.id === 'b')!.position;

    // Simulate a manual drag of node 'b'.
    const draggedNodes = result.current.nodes.map((n) =>
      n.id === 'b' ? { ...n, position: { x: 999, y: 999 } } : n,
    );
    rerender({ nodes: draggedNodes, edges });

    const b = result.current.nodes.find((n) => n.id === 'b')!;
    expect(b.position).toEqual({ x: 999, y: 999 });

    // Other nodes should keep their last laid-out positions.
    const a = result.current.nodes.find((n) => n.id === 'a')!;
    const c = result.current.nodes.find((n) => n.id === 'c')!;
    expect(a.position).toEqual(laidOut.find((n) => n.id === 'a')!.position);
    expect(c.position).toEqual(laidOut.find((n) => n.id === 'c')!.position);
    expect(b.position).not.toEqual(laidOutB);
  });

  it('relayout() clears manual overrides and repositions all nodes', () => {
    const nodes = makeNodes();
    const edges = makeEdges();
    const { result, rerender } = renderHook(
      ({ nodes, edges }) => useAutoLayout(nodes, edges, { direction: 'TB' }),
      { initialProps: { nodes, edges } },
    );

    const draggedNodes = result.current.nodes.map((n) =>
      n.id === 'b' ? { ...n, position: { x: 999, y: 999 } } : n,
    );
    rerender({ nodes: draggedNodes, edges });
    expect(result.current.nodes.find((n) => n.id === 'b')!.position).toEqual({ x: 999, y: 999 });

    act(() => {
      result.current.relayout();
    });

    const b = result.current.nodes.find((n) => n.id === 'b')!;
    expect(b.position.x).not.toBe(999);
    expect(b.position.y).not.toBe(999);
  });

  it('handles compound parent/child nesting with relative child positions', () => {
    const parent: Node<{ label: string }> = {
      id: 'parent',
      type: 'default',
      position: { x: 0, y: 0 },
      data: { label: 'Parent' },
    };
    const child1: Node<{ label: string }> = {
      id: 'child1',
      type: 'default',
      position: { x: 0, y: 0 },
      data: { label: 'Child 1' },
      parentId: 'parent',
    };
    const child2: Node<{ label: string }> = {
      id: 'child2',
      type: 'default',
      position: { x: 0, y: 0 },
      data: { label: 'Child 2' },
      parentId: 'parent',
    };
    const nodes = [parent, child1, child2];
    const edges: Edge[] = [];

    const { result } = renderHook(() => useAutoLayout(nodes, edges, { direction: 'TB' }));

    const laidOutParent = result.current.nodes.find((n) => n.id === 'parent')!;
    const laidOutChild1 = result.current.nodes.find((n) => n.id === 'child1')!;
    const laidOutChild2 = result.current.nodes.find((n) => n.id === 'child2')!;

    expect(laidOutParent.position.x).toBeDefined();
    expect(laidOutParent.position.y).toBeDefined();
    // Children should be positioned relative to the parent origin.
    expect(laidOutChild1.position.x).toBeGreaterThanOrEqual(0);
    expect(laidOutChild1.position.y).toBeGreaterThanOrEqual(0);
    expect(laidOutChild2.position.x).toBeGreaterThanOrEqual(0);
    expect(laidOutChild2.position.y).toBeGreaterThanOrEqual(0);
    // Parent should have explicit dimensions so the relative child coordinates
    // are contained.
    expect(laidOutParent.style).toMatchObject({
      width: expect.any(Number),
      height: expect.any(Number),
    });
  });

  it('lays out a 50-node compound graph within the 200ms performance threshold', () => {
    const parent: Node<{ label: string }> = {
      id: 'parent',
      type: 'default',
      position: { x: 0, y: 0 },
      data: { label: 'Parent' },
    };
    const children: Node<{ label: string }>[] = [];
    const edges: Edge[] = [];
    for (let i = 0; i < 49; i++) {
      children.push({
        id: `child-${i}`,
        type: 'default',
        position: { x: 0, y: 0 },
        data: { label: `Child ${i}` },
        parentId: 'parent',
      });
      if (i > 0) {
        edges.push({ id: `e-${i}`, source: `child-${i - 1}`, target: `child-${i}` });
      }
    }
    const nodes = [parent, ...children];

    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { result } = renderHook(() => useAutoLayout(nodes, edges, { direction: 'TB' }));

    expect(result.current.nodes).toHaveLength(50);
    expect(warnSpy).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});

describe('useAutoLayout — supplied-positions mode (W003)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('preserves supplied positions on first open when hasSuppliedPositions is true', () => {
    const nodes: Node<{ label: string }>[] = [
      { id: 'a', type: 'default', position: { x: 100, y: 200 }, data: { label: 'A' } },
      { id: 'b', type: 'default', position: { x: 300, y: 400 }, data: { label: 'B' } },
      { id: 'c', type: 'default', position: { x: 500, y: 600 }, data: { label: 'C' } },
    ];
    const edges: Edge[] = [
      { id: 'e1', source: 'a', target: 'b' },
      { id: 'e2', source: 'b', target: 'c' },
    ];

    const { result } = renderHook(() =>
      useAutoLayout(nodes, edges, { direction: 'TB', hasSuppliedPositions: true }),
    );

    // Positions must be preserved exactly — dagre must not override them.
    expect(result.current.nodes.find((n) => n.id === 'a')!.position).toEqual({ x: 100, y: 200 });
    expect(result.current.nodes.find((n) => n.id === 'b')!.position).toEqual({ x: 300, y: 400 });
    expect(result.current.nodes.find((n) => n.id === 'c')!.position).toEqual({ x: 500, y: 600 });
  });

  it('does not treat supplied positions as manual drags on subsequent renders', () => {
    const initialNodes: Node<{ label: string }>[] = [
      { id: 'a', type: 'default', position: { x: 10, y: 20 }, data: { label: 'A' } },
      { id: 'b', type: 'default', position: { x: 30, y: 40 }, data: { label: 'B' } },
    ];
    const edges: Edge[] = [{ id: 'e1', source: 'a', target: 'b' }];

    const { result, rerender } = renderHook(
      ({ nodes, edges }) => useAutoLayout(nodes, edges, { direction: 'TB', hasSuppliedPositions: true }),
      { initialProps: { nodes: initialNodes, edges } },
    );

    // Re-render with the same positions (e.g. a graph refresh). The hook must
    // not classify the supplied positions as manual drags and must continue to
    // preserve them.
    rerender({ nodes: initialNodes, edges });

    expect(result.current.nodes.find((n) => n.id === 'a')!.position).toEqual({ x: 10, y: 20 });
    expect(result.current.nodes.find((n) => n.id === 'b')!.position).toEqual({ x: 30, y: 40 });
  });

  it('relayout() overrides supplied positions even when hasSuppliedPositions is true', () => {
    const nodes: Node<{ label: string }>[] = [
      { id: 'a', type: 'default', position: { x: 100, y: 200 }, data: { label: 'A' } },
      { id: 'b', type: 'default', position: { x: 300, y: 400 }, data: { label: 'B' } },
    ];
    const edges: Edge[] = [{ id: 'e1', source: 'a', target: 'b' }];

    const { result } = renderHook(() =>
      useAutoLayout(nodes, edges, { direction: 'TB', hasSuppliedPositions: true }),
    );

    // First open: positions preserved.
    expect(result.current.nodes.find((n) => n.id === 'a')!.position).toEqual({ x: 100, y: 200 });

    act(() => {
      result.current.relayout();
    });

    // After explicit relayout: dagre takes over and repositions.
    const aAfter = result.current.nodes.find((n) => n.id === 'a')!.position;
    const bAfter = result.current.nodes.find((n) => n.id === 'b')!.position;
    expect(aAfter).not.toEqual({ x: 100, y: 200 });
    expect(bAfter).not.toEqual({ x: 300, y: 400 });
  });
});

describe('useAutoLayout — defensive dagre label fallback (M001)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it('does not crash and falls back to finite positions when a dagre label is missing', async () => {
    // Mock @dagrejs/dagre so that Graph.node('ghost') returns undefined,
    // simulating a dagre compound-graph edge case where layout does not assign
    // coordinates to every setNode'd node.
    vi.doMock('@dagrejs/dagre', async (importOriginal) => {
      const actual = await importOriginal<typeof import('@dagrejs/dagre')>();
      const OriginalGraph = actual.Graph;
      return {
        ...actual,
        Graph: class extends OriginalGraph {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          node(id: string): any {
            if (id === 'ghost') return undefined;
            return super.node(id);
          }
        },
      };
    });

    const { useAutoLayout: useAutoLayoutMocked } = await import('../use-auto-layout');
    const { renderHook: renderHookMocked } = await import('@testing-library/react');

    const nodes: Node<{ label: string }>[] = [
      { id: 'a', type: 'default', position: { x: 0, y: 0 }, data: { label: 'A' } },
      { id: 'ghost', type: 'default', position: { x: 0, y: 0 }, data: { label: 'Ghost' } },
    ];
    const edges: Edge[] = [{ id: 'e1', source: 'a', target: 'ghost' }];

    // Must not throw — the defensive fallback handles the missing label.
    const { result } = renderHookMocked(() =>
      useAutoLayoutMocked(nodes, edges, { direction: 'TB' }),
    );

    // The ghost node's position must be a finite number, not NaN or a crash.
    const ghost = result.current.nodes.find((n) => n.id === 'ghost')!;
    expect(Number.isFinite(ghost.position.x)).toBe(true);
    expect(Number.isFinite(ghost.position.y)).toBe(true);

    // The normal node still gets a real dagre position.
    const a = result.current.nodes.find((n) => n.id === 'a')!;
    expect(Number.isFinite(a.position.x)).toBe(true);
    expect(Number.isFinite(a.position.y)).toBe(true);
  });
});
