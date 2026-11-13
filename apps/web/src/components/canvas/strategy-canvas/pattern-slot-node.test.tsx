/**
 * `PatternSlot` node (v239-p3-canvas-compose-ui T2/T3b).
 *
 * Collapsed render carries `{pattern_id, version, effect_class, slot_bindings}`;
 * the binding diagnostic names an unbound required slot on the node; the
 * compose-path simulation overlay marks a visited slot as simulated.
 */
import { screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import type { NodeProps } from '@xyflow/react';

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return {
    ...actual,
    Handle: ({ type: _type, position: _position }: { type: string; position: number }) =>
      null as unknown as ReactNode,
  };
});

import { renderInApp } from '@/test/test-providers';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { StrategyPatternSlotNode } from '@/components/canvas/strategy-nodes';

function slotProps(data: Partial<StrategyNodeData>): NodeProps {
  return {
    id: 'collect',
    type: 'strategy-pattern-slot',
    position: { x: 0, y: 0 },
    data: {
      stateId: 'collect',
      label: 'collect',
      stateKind: 'pattern_slot',
      presetId: 'user/demo',
      isTerminal: false,
      isInitial: false,
      isGroup: false,
      patternId: 'prompt-step',
      patternVersion: '1.0.0',
      patternEffectClass: 'external',
      ...data,
    } as StrategyNodeData,
    selected: false,
    draggable: false,
    selectable: false,
    connectable: false,
    deletable: false,
    focusable: false,
    dragging: false,
    zIndex: 0,
    isConnectable: false,
    positionAbsoluteX: 0,
    positionAbsoluteY: 0,
  } as unknown as NodeProps;
}

describe('StrategyPatternSlotNode', () => {
  it('renders the collapsed pin with its effect class', () => {
    renderInApp(<StrategyPatternSlotNode {...slotProps({})} />);
    expect(screen.getByText('prompt-step@1.0.0')).toBeInTheDocument();
    expect(screen.getByTestId('pattern-slot-effect')).toHaveTextContent('effect: external');
  });

  it('names an unbound required slot on the node', () => {
    renderInApp(<StrategyPatternSlotNode {...slotProps({ unboundRequired: ['topic'] })} />);
    expect(screen.getByTestId('pattern-slot-unbound')).toHaveTextContent('topic');
  });

  it('marks a slot the trajectory visited as simulated (T3b overlay)', () => {
    renderInApp(<StrategyPatternSlotNode {...slotProps({ simulated: true })} />);
    expect(screen.getByTestId('pattern-slot-simulated')).toBeInTheDocument();
  });

  it('toggles expand through the patched handler', () => {
    const onToggleExpand = vi.fn();
    renderInApp(<StrategyPatternSlotNode {...slotProps({ onToggleExpand })} />);
    fireEvent.click(screen.getByTestId('pattern-slot-toggle'));
    expect(onToggleExpand).toHaveBeenCalledTimes(1);
  });
});
