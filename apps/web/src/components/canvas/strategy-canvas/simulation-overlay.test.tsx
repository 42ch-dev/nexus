/**
 * Static-simulation overlay — predicted waits and collapsed judge branches are
 * visible on the canvas nodes, not only inside the Approve modal.
 */
import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
import type { Node } from '@xyflow/react';

vi.mock('@xyflow/react', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@xyflow/react')>();
  return {
    ...actual,
    Handle: ({ type: _type, position: _position }: { type: string; position: number }) =>
      null as unknown as ReactNode,
  };
});

import type { NodeProps } from '@xyflow/react';

import { renderInApp } from '@/test/test-providers';
import { simulationTrajectory } from '@/test/simulation-fixture';
import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';
import { StrategyStateNode } from '@/components/canvas/strategy-nodes';

import { applyTrajectoryOverlay } from './simulation-overlay';

function canvasNode(id: string): Node<StrategyNodeData> {
  return {
    id,
    type: 'strategy-state',
    position: { x: 0, y: 0 },
    data: { stateId: id, label: id, stateKind: 'state', presetId: 'p', isTerminal: false, isInitial: false, isGroup: false },
  };
}

function nodeProps(data: Partial<StrategyNodeData>): NodeProps {
  return {
    id: 'routing',
    type: 'strategy-state',
    position: { x: 0, y: 0 },
    data: {
      stateId: 'routing',
      label: 'Routing',
      stateKind: 'state',
      presetId: 'p',
      isTerminal: false,
      isInitial: false,
      isGroup: false,
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

describe('applyTrajectoryOverlay', () => {
  it('marks predicted waits and stubbed judge branches by node id', () => {
    const nodes = applyTrajectoryOverlay(
      [canvasNode('draft'), canvasNode('routing'), canvasNode('pause'), canvasNode('done')],
      simulationTrajectory(),
    );
    const byId = new Map(nodes.map((n) => [n.id, n.data]));

    expect(byId.get('routing')?.branchNotExplored).toBe(true);
    expect(byId.get('pause')?.predictedWait).toBe('manual');
    expect(byId.get('draft')?.predictedWait).toBeUndefined();
    expect(byId.get('draft')?.branchNotExplored).toBeUndefined();
  });

  it('leaves nodes untouched without a trajectory', () => {
    const nodes = [canvasNode('routing')];

    expect(applyTrajectoryOverlay(nodes, null)).toEqual(nodes);
  });

  it('keeps a signal wait off the canvas because it has no target node', () => {
    const nodes = applyTrajectoryOverlay(
      [canvasNode('routing')],
      simulationTrajectory({
        predicted_waits: [{ reason: 'signal', subject: 'resume' }],
        stubbed_judges: [],
      }),
    );

    expect(nodes[0].data.predictedWait).toBeUndefined();
  });

  it('does not attach a signal wait to a node whose id equals the signal name', () => {
    const nodes = applyTrajectoryOverlay(
      [canvasNode('draft'), canvasNode('resume')],
      simulationTrajectory({
        predicted_waits: [
          { reason: 'signal', subject: 'resume' },
          { reason: 'manual', subject: 'draft' },
        ],
        stubbed_judges: [],
      }),
    );
    const byId = new Map(nodes.map((n) => [n.id, n.data]));

    expect(byId.get('resume')?.predictedWait).toBeUndefined();
    expect(byId.get('draft')?.predictedWait).toBe('manual');
  });
});

describe('StrategyStateNode — simulation markers', () => {
  it('renders the predicted-wait and branch-not-explored markers', () => {
    renderInApp(
      <StrategyStateNode {...nodeProps({ predictedWait: 'manual', branchNotExplored: true })} />,
    );

    expect(screen.getByTestId('strategy-node-predicted-wait')).toHaveTextContent(
      'Predicted wait (static)',
    );
    expect(screen.getByTestId('strategy-node-branch-not-explored')).toHaveTextContent(
      'Branch not explored',
    );
  });

  it('renders neither marker for an uncovered node', () => {
    renderInApp(<StrategyStateNode {...nodeProps({})} />);

    expect(screen.queryByTestId('strategy-node-predicted-wait')).not.toBeInTheDocument();
    expect(screen.queryByTestId('strategy-node-branch-not-explored')).not.toBeInTheDocument();
  });
});
