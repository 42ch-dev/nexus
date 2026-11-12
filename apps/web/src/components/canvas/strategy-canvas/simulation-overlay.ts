/**
 * Static-simulation overlay for the Strategy canvas.
 *
 * The trajectory names node ids, which are the preset state ids on this
 * surface. Predicted waits and collapsed judge branches are attached to the
 * matching nodes so the creator sees them on the canvas — not only inside the
 * Approve modal. Signal waits have no target node, so they stay modal-only.
 */
import type { Node } from '@xyflow/react';

import type { SimulationTrajectory } from '@42ch/nexus-contracts';

import type { StrategyNodeData } from '@/lib/canvas/strategy-graph';

export function applyTrajectoryOverlay(
  nodes: Node<StrategyNodeData>[],
  trajectory: SimulationTrajectory | null,
): Node<StrategyNodeData>[] {
  if (!trajectory) return nodes;

  // Only a `manual` wait names a node id; a `signal` subject is a signal name,
  // so a signal colliding with a state id must not attach to that node.
  const waitByNode = new Map<string, 'manual'>();
  for (const wait of trajectory.predicted_waits) {
    if (wait.reason === 'manual') waitByNode.set(wait.subject, wait.reason);
  }
  const stubbed = new Set(trajectory.stubbed_judges);

  return nodes.map((node) => {
    const { stateId } = node.data;
    const predictedWait = waitByNode.get(stateId);
    const branchNotExplored = stubbed.has(stateId);
    if (!predictedWait && !branchNotExplored) return node;
    return {
      ...node,
      data: {
        ...node.data,
        ...(predictedWait ? { predictedWait } : {}),
        ...(branchNotExplored ? { branchNotExplored: true } : {}),
      },
    };
  });
}
