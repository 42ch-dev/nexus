/**
 * V1.163 P1 inbound `?event=` focus — shared-hook extraction (R-V1163P1QC1-002).
 *
 * The hook is the deduplicated body of the two Timeline surfaces' identical
 * inbound-focus effects: World (`timeline-canvas.tsx`) and Work
 * (`work-timeline-canvas.tsx`). These cases pin the contract both surfaces
 * rely on — select-the-matching-node, honest empty focus for an unknown id,
 * the per-URL one-shot guard, late projection, and the caller-supplied target
 * rule (the ONE difference between the two surfaces: the World Timeline's
 * `entity:` id namespace is shared with context-entity / Brief-era nodes and
 * must require `layoutHint === 'event'`; the Work Timeline's `wt-event:`
 * namespace is event-exclusive).
 */
import { renderHook } from '@testing-library/react';
import type { Node } from '@xyflow/react';
import { describe, expect, it, vi } from 'vitest';

import {
  useInboundEventFocus,
  type InboundFocusSurface,
} from '../use-inbound-event-focus';

interface TestNodeData extends Record<string, unknown> {
  layoutHint?: string;
}

const CONTEXT_NODE: Node<TestNodeData> = {
  id: 'entity:kb-char-1',
  type: 'timeline-key-block',
  position: { x: 0, y: 0 },
  data: { layoutHint: 'context' },
};
const EVENT_NODE: Node<TestNodeData> = {
  id: 'entity:evt-1',
  type: 'timeline-event',
  position: { x: 0, y: 0 },
  data: { layoutHint: 'event' },
};

/** The World Timeline's target rule (its `entity:` ids are shared). */
const isEventNode = (node: Node<TestNodeData>) => node.data.layoutHint === 'event';

interface HarnessProps {
  surface: InboundFocusSurface<TestNodeData>;
  nodeId: string | null;
  isFocusTarget?: (node: Node<TestNodeData>) => boolean;
}

function renderFocus(
  nodeId: string | null,
  options: {
    nodes?: Node<TestNodeData>[];
    selectedNodeId?: string | null;
    isFocusTarget?: (node: Node<TestNodeData>) => boolean;
  } = {},
) {
  const onNodesChange = vi.fn();
  const surface: InboundFocusSurface<TestNodeData> = {
    nodes: options.nodes ?? [CONTEXT_NODE, EVENT_NODE],
    selectedNodeId: options.selectedNodeId ?? null,
    onNodesChange,
  };
  // Annotated so `renderHook`'s inferred Props keep `isFocusTarget` optional —
  // the rerenders below exercise the "no target rule" (Work) call shape.
  const initialProps: HarnessProps = { surface, nodeId, isFocusTarget: options.isFocusTarget };
  const view = renderHook(
    ({ surface: s, nodeId: id, isFocusTarget }: HarnessProps) =>
      useInboundEventFocus(s, id, isFocusTarget),
    { initialProps },
  );
  return {
    onNodesChange,
    surface,
    rerender: (overrides: Partial<HarnessProps> = {}) =>
      view.rerender({ ...initialProps, ...overrides }),
  };
}

describe('useInboundEventFocus', () => {
  it('selects the projected node matching the inbound id', () => {
    const { onNodesChange } = renderFocus('entity:evt-1');

    expect(onNodesChange).toHaveBeenCalledTimes(1);
    expect(onNodesChange).toHaveBeenCalledWith([
      { type: 'select', id: CONTEXT_NODE.id, selected: false },
      { type: 'select', id: EVENT_NODE.id, selected: true },
    ]);
  });

  it('does nothing without an inbound id', () => {
    const { onNodesChange } = renderFocus(null);

    expect(onNodesChange).not.toHaveBeenCalled();
  });

  it('honest empty focus: an unknown id selects nothing', () => {
    const { onNodesChange } = renderFocus('entity:missing');

    expect(onNodesChange).not.toHaveBeenCalled();
  });

  it('applies once the target node is projected (late projection)', () => {
    const { onNodesChange, surface, rerender } = renderFocus('entity:evt-1', {
      nodes: [CONTEXT_NODE],
    });
    expect(onNodesChange).not.toHaveBeenCalled();

    rerender({
      surface: { ...surface, nodes: [CONTEXT_NODE, EVENT_NODE] },
      nodeId: 'entity:evt-1',
    });

    expect(onNodesChange).toHaveBeenCalledTimes(1);
    expect(onNodesChange).toHaveBeenCalledWith([
      { type: 'select', id: CONTEXT_NODE.id, selected: false },
      { type: 'select', id: EVENT_NODE.id, selected: true },
    ]);
  });

  it('one-shot: never fights a later user selection', () => {
    const { onNodesChange, surface, rerender } = renderFocus('entity:evt-1');
    expect(onNodesChange).toHaveBeenCalledTimes(1);

    // A projection rebuild + the user selecting a different node.
    rerender({
      surface: {
        ...surface,
        nodes: [...surface.nodes],
        selectedNodeId: CONTEXT_NODE.id,
      },
      nodeId: 'entity:evt-1',
    });

    expect(onNodesChange).toHaveBeenCalledTimes(1);
  });

  it('one-shot: a selection that already survived a rebuild is marked applied', () => {
    const { onNodesChange, surface, rerender } = renderFocus('entity:evt-1', {
      selectedNodeId: EVENT_NODE.id,
    });
    expect(onNodesChange).not.toHaveBeenCalled();

    rerender({
      surface: { ...surface, nodes: [...surface.nodes] },
      nodeId: 'entity:evt-1',
    });

    expect(onNodesChange).not.toHaveBeenCalled();
  });

  it('target rule: rejects a node id that belongs to another projection kind', () => {
    const { onNodesChange } = renderFocus('entity:kb-char-1', {
      isFocusTarget: isEventNode,
    });

    expect(onNodesChange).not.toHaveBeenCalled();
  });
});
