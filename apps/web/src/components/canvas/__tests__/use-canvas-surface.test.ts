/**
 * `useCanvasSurface` — composition hook for the shared canvas surface adapter.
 *
 * Pins: projection correctness, conflict-state handling, alt-view toggle, and
 * viewport delegation. React Flow / CanvasShell are not mounted; the hook is
 * exercised directly via `renderHook`.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import type { NodeTypes, Viewport } from '@xyflow/react';

import type { ConflictModalProps } from '../conflict-modal';
import type { CanvasSurfaceAdapter } from '../canvas-surface-adapter';
import {
  useCanvasSurface,
  type CanvasSurfaceQueryResult,
} from '../use-canvas-surface';
import { clearViewportCache } from '../use-canvas-viewport';

interface SimpleGraph {
  items: string[];
}

const nodeTypes: NodeTypes = {
  default: () => null,
};

function makeAdapter(): CanvasSurfaceAdapter<
  SimpleGraph,
  { label: string },
  { label: string }
> {
  return {
    surfaceKind: 'strategy',
    projectGraph(graph) {
      return {
        nodes: graph.items.map((id, index) => ({
          id,
          type: 'default',
          position: { x: index * 10, y: index * 10 },
          data: { label: id },
        })),
        edges: [],
      };
    },
    nodeTypes,
    summarizeGraph(graph) {
      return `Graph with ${graph.items.length} items`;
    },
    adaptConflict(_error) {
      return {
        open: true,
        currentRevision: 1,
        draft: {
          label: '',
          description: '',
          nextTarget: '',
          promptBody: '',
        },
        changedFields: ['label'],
        onUseCurrent: () => {},
        onReapply: () => {},
        onDismiss: () => {},
      } satisfies ConflictModalProps;
    },
    renderAltView() {
      return 'alt-view';
    },
    renderInspector(node) {
      return `inspector:${node.id}`;
    },
  };
}

function makeQuery(
  partial: Partial<CanvasSurfaceQueryResult<SimpleGraph>> = {},
): CanvasSurfaceQueryResult<SimpleGraph> {
  return {
    data: { items: ['a', 'b'] },
    isLoading: false,
    isError: false,
    error: null,
    refetch: () => {},
    ...partial,
  };
}

describe('useCanvasSurface', () => {
  beforeEach(() => {
    clearViewportCache();
  });

  it('returns an empty graph while data is undefined', () => {
    const adapter = makeAdapter();
    const query = makeQuery({ data: undefined });
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );
    expect(result.current.nodes).toEqual([]);
    expect(result.current.edges).toEqual([]);
    expect(result.current.summaryText).toBe('');
  });

  it('projects the graph via the adapter and computes the summary', () => {
    const adapter = makeAdapter();
    const query = makeQuery({ data: { items: ['a', 'b', 'c'] } });
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );
    expect(result.current.nodes.map((n) => n.id)).toEqual(['a', 'b', 'c']);
    expect(result.current.edges).toEqual([]);
    expect(result.current.summaryText).toBe('Graph with 3 items');
  });

  it('preserves manual positions and selection across projection rebuilds', () => {
    const adapter = makeAdapter();
    const query = makeQuery({ data: { items: ['a'] } });
    const hook = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );

    act(() => {
      hook.result.current.onNodesChange([
        { id: 'a', type: 'position', position: { x: 999, y: 999 } },
        { id: 'a', type: 'select', selected: true },
      ]);
    });

    const moved = hook.result.current.nodes.find((n) => n.id === 'a')!;
    expect(moved.position).toEqual({ x: 999, y: 999 });
    expect(moved.selected).toBe(true);

    hook.rerender({ adapter, query: makeQuery({ data: { items: ['a', 'b'] } }) });

    const preserved = hook.result.current.nodes.find((n) => n.id === 'a')!;
    expect(preserved.position).toEqual({ x: 999, y: 999 });
    expect(preserved.selected).toBe(true);

    const appended = hook.result.current.nodes.find((n) => n.id === 'b')!;
    expect(appended.position).toEqual({ x: 10, y: 10 });
  });

  it('exposes selectedNode and selectedNodeId from node selection', () => {
    const adapter = makeAdapter();
    const query = makeQuery({ data: { items: ['a', 'b'] } });
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );

    expect(result.current.selectedNode).toBeNull();
    expect(result.current.selectedNodeId).toBeNull();

    act(() => {
      result.current.onNodesChange([
        { id: 'a', type: 'select', selected: true },
        { id: 'b', type: 'select', selected: false },
      ]);
    });

    expect(result.current.selectedNode?.id).toBe('a');
    expect(result.current.selectedNodeId).toBe('a');
    expect(result.current.inspector).toBe('inspector:a');
  });

  it('renders the adapter alt view and toggles it', () => {
    const adapter = makeAdapter();
    const query = makeQuery();
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );

    expect(result.current.showAlt).toBe(false);
    expect(result.current.altView).toBe('alt-view');

    act(() => {
      result.current.setShowAlt(true);
    });
    expect(result.current.showAlt).toBe(true);
  });

  it('auto-populates conflict from query errors', () => {
    const adapter = makeAdapter();
    const query = makeQuery({
      data: undefined,
      isError: true,
      error: new Error('graph failed'),
    });
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );

    expect(result.current.isError).toBe(true);
    expect(result.current.conflict).not.toBeNull();
  });

  it('exposes handleConflict and setConflict for manual conflict management', () => {
    const adapter = makeAdapter();
    const query = makeQuery();
    const { result } = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );

    expect(result.current.conflict).toBeNull();

    act(() => {
      result.current.handleConflict(new Error('patch conflict'));
    });
    expect(result.current.conflict).not.toBeNull();

    act(() => {
      result.current.setConflict(null);
    });
    expect(result.current.conflict).toBeNull();
  });

  it('delegates viewport caching to useCanvasViewport', () => {
    const adapter = makeAdapter();
    const query = makeQuery();
    const viewport: Viewport = { x: 42, y: 84, zoom: 1.25 };

    const first = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );
    expect(first.result.current.viewport.cachedViewport).toBeNull();

    act(() => {
      first.result.current.viewport.onViewportChange(viewport);
    });
    first.unmount();

    const second = renderHook(
      ({ adapter, query }) => useCanvasSurface(adapter, query),
      { initialProps: { adapter, query } },
    );
    expect(second.result.current.viewport.cachedViewport).toEqual(viewport);
  });
});
