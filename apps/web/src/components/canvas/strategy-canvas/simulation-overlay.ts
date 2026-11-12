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

  const waitBySubject = new Map<string, 'manual' | 'signal'>();
  for (const wait of trajectory.predicted_waits) {
    waitBySubject.set(wait.subject, wait.reason);
  }
  const stubbed = new Set(trajectory.stubbed_judges);

  return nodes.map((node) => {
    const { stateId } = node.data;
    const predictedWait = waitBySubject.get(stateId);
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
