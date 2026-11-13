/**
 * Slot inspector (v239-p3-canvas-compose-ui T3): bind_slot writes,
 * replace_subtree and unnest route to the compose interactions.
 */
import { screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

import { renderInApp } from '@/test/test-providers';
import { SlotInspector } from './slot-inspector';

const NODE: StrategyNodeData = {
  stateId: 'collect',
  label: 'collect',
  stateKind: 'pattern_slot',
  presetId: 'user/demo',
  isTerminal: false,
  isInitial: false,
  isGroup: false,
  patternId: 'prompt-step',
  patternVersion: '1.0.0',
  nestId: 'collect-region',
  slotRows: [{ name: 'topic', required: true, kind: 'string', value: undefined }],
  unboundRequired: ['topic'],
};

const PATTERNS = [
  {
    id: 'judge-step',
    version: '2.0.0',
    available: true,
    reason: null,
  },
];

function renderInspector(props: Partial<Parameters<typeof SlotInspector>[0]> = {}) {
  return renderInApp(
    <SlotInspector
      node={NODE}
      patterns={PATTERNS}
      isComposing={false}
      onBindSlot={() => Promise.resolve()}
      onReplaceSubtree={() => Promise.resolve()}
      onUnnest={() => Promise.resolve()}
      {...props}
    />,
  );
}

describe('SlotInspector', () => {
  it('binds a slot through the compose interaction', async () => {
    const onBindSlot = vi.fn(() => Promise.resolve());
    renderInspector({ onBindSlot });
    fireEvent.change(screen.getByTestId('slot-input-topic'), { target: { value: 'water policy' } });
    fireEvent.click(screen.getByTestId('slot-bind-topic'));
    await waitFor(() =>
      expect(onBindSlot).toHaveBeenCalledWith('collect-region', 'topic', 'water policy'),
    );
  });

  it('re-pins the region through replace_subtree', async () => {
    const onReplaceSubtree = vi.fn(() => Promise.resolve());
    renderInspector({ onReplaceSubtree });
    fireEvent.change(screen.getByTestId('slot-replace'), {
      target: { value: JSON.stringify({ id: 'judge-step', version: '2.0.0' }) },
    });
    fireEvent.click(screen.getByTestId('slot-replace-commit'));
    await waitFor(() =>
      expect(onReplaceSubtree).toHaveBeenCalledWith('collect-region', {
        id: 'judge-step',
        version: '2.0.0',
      }),
    );
  });

  it('unnests the region', async () => {
    const onUnnest = vi.fn(() => Promise.resolve());
    renderInspector({ onUnnest });
    fireEvent.click(screen.getByTestId('slot-unnest'));
    await waitFor(() => expect(onUnnest).toHaveBeenCalledWith('collect-region'));
  });

  it('consumes a refused op instead of rethrowing it at the event boundary', async () => {
    // The hook has already presented the refusal; a rethrown rejection here
    // would surface as an unhandled rejection and fail this file.
    const refused = vi.fn(() => Promise.reject(new Error('409 conflict')));
    renderInspector({ onBindSlot: refused, onReplaceSubtree: refused, onUnnest: refused });

    fireEvent.change(screen.getByTestId('slot-input-topic'), { target: { value: 'water policy' } });
    fireEvent.click(screen.getByTestId('slot-bind-topic'));
    fireEvent.change(screen.getByTestId('slot-replace'), {
      target: { value: JSON.stringify({ id: 'judge-step', version: '2.0.0' }) },
    });
    fireEvent.click(screen.getByTestId('slot-replace-commit'));
    fireEvent.click(screen.getByTestId('slot-unnest'));

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(refused).toHaveBeenCalledTimes(3);
  });

  it('renders nothing without a nest id', () => {
    const { container } = renderInspector({ node: { ...NODE, nestId: undefined } });
    expect(container).toBeEmptyDOMElement();
  });
});
