/**
 * `useAutoLayout` — T1 no-op stub.
 *
 * T4 will replace the stub with a real dagre implementation. The stub contract
 * must still pass nodes through unchanged and expose the same result shape.
 */
import { describe, expect, it } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { Edge, Node } from '@xyflow/react';

import { useAutoLayout } from '../use-auto-layout';

describe('useAutoLayout — T1 no-op stub', () => {
  it('returns nodes unchanged', () => {
    const nodes: Node<{ label: string }>[] = [
      {
        id: 'a',
        type: 'default',
        position: { x: 5, y: 10 },
        data: { label: 'A' },
      },
    ];
    const edges: Edge[] = [{ id: 'e1', source: 'a', target: 'b' }];

    const { result } = renderHook(() =>
      useAutoLayout(nodes, edges, { direction: 'TB', rankSep: 80 }),
    );

    expect(result.current.nodes).toBe(nodes);
    expect(result.current.nodes[0].position).toEqual({ x: 5, y: 10 });
  });

  it('reports no layout in progress and exposes a no-op relayout', () => {
    const nodes: Node<{ label: string }>[] = [];
    const edges: Edge[] = [];

    const { result } = renderHook(() => useAutoLayout(nodes, edges));

    expect(result.current.isLayouting).toBe(false);
    expect(result.current.relayout).toBeTypeOf('function');
    expect(() => result.current.relayout()).not.toThrow();
  });
});
