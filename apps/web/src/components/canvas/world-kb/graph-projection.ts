/**
 * World KB graph projection — World → entity/candidate nodes + source-anchor
 * provenance edges (canvas-strategy-surface.md §3.3 surface 3 + §3.4).
 *
 * Pure functions: convert the V1.73 generated projections
 * (`WorldKbGraphResponse` + `WorldKbCandidatesResponse`) into the
 * {@link WorldKbNodeData} / {@link WorldKbEdgeData} payloads plus React Flow
 * `Node`/`Edge` arrays. The graph adapter owns node layout (a deterministic
 * grid by BlockType lane); the inspector + conflict modal own write affordances.
 *
 * Relationship edges (`world-kb-relationship-edge`) are derived read-only from
 * source anchors in V1.73; `world_kb.patch_relationship` + a `kb_relationships`
 * table are deferred to V1.74.
 */
import type { Edge, Node } from '@xyflow/react';
import type { BlockType } from '@42ch/nexus-contracts';

import {
  BLOCK_TYPE_LABELS,
  isComputableBlock,
  lifecycleFromStatus,
  type WorldKbEdgeData,
  type WorldKbNodeData,
} from './types';
import type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbGraphResponse,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

/** Lane order for the deterministic grid layout (by BlockType). */
const LANE_ORDER: readonly BlockType[] = [
  'character',
  'organization',
  'faction',
  'species',
  'ability',
  'magic_system',
  'technology',
  'item',
  'scene',
  'event',
  'conflict',
  'info_point',
  'deity',
  'level',
  'economy_tier',
  'dialogue',
  'beat',
  'act',
];

const LANE_X = 280;
const ROW_Y = 150;
const ORIGIN_X = 40;
const ORIGIN_Y = 40;

/**
 * Build a deterministic lane+row layout key per BlockType so entities of the
 * same kind stack vertically and kinds spread horizontally across the canvas.
 */
function laneIndexOf(kind: BlockType): number {
  const i = LANE_ORDER.indexOf(kind);
  return i === -1 ? LANE_ORDER.length : i;
}

/** Convert a confirmed/rejected/merged KeyBlock entity to node data. */
export function entityToNodeData(
  entity: WorldKbEntityProjection,
  worldId: string,
): WorldKbNodeData {
  return {
    worldId,
    keyBlockId: entity.key_block_id,
    entityKind: entity.block_type,
    name: entity.canonical_name,
    lifecycle: lifecycleFromStatus(entity.status),
    version: entity.version,
    sourceAnchorCount: entity.source_anchor_count ?? 0,
    computable: isComputableBlock(entity.block_type),
    updatedAt: entity.updated_at,
  };
}

/** Convert a pending promotion candidate to node data. */
export function candidateToNodeData(
  candidate: WorldKbCandidateProjection,
): WorldKbNodeData {
  return {
    worldId: candidate.world_id,
    candidateId: candidate.candidate_id,
    jobId: candidate.job_id,
    entityKind: candidate.block_type,
    name: candidate.canonical_name,
    lifecycle: 'pending',
    version: candidate.version,
    sourceAnchorCount: candidate.source_anchor_count ?? 0,
    computable: isComputableBlock(candidate.block_type),
    updatedAt: candidate.created_at,
  };
}

/** Position nodes in a deterministic lane grid; stable ids survive refetch. */
export function layoutNodes(
  entities: WorldKbEntityProjection[],
  candidates: WorldKbCandidateProjection[],
  worldId: string,
): Node<WorldKbNodeData>[] {
  /** Per-lane row counter so each lane stacks downward. */
  const rowByLane = new Map<number, number>();
  const nodes: Node<WorldKbNodeData>[] = [];

  for (const entity of entities) {
    const lane = laneIndexOf(entity.block_type);
    const row = rowByLane.get(lane) ?? 0;
    rowByLane.set(lane, row + 1);
    nodes.push({
      id: `entity:${entity.key_block_id}`,
      type: 'worldkb-entity',
      position: { x: ORIGIN_X + lane * LANE_X, y: ORIGIN_Y + row * ROW_Y },
      data: entityToNodeData(entity, worldId),
    });
  }

  // Pending candidates stack in a dedicated trailing lane so reviewers can find
  // them without hunting across entity lanes.
  const pendingLane = LANE_ORDER.length + 1;
  candidates.forEach((candidate, row) => {
    nodes.push({
      id: `candidate:${candidate.candidate_id}`,
      type: 'worldkb-entity',
      position: { x: ORIGIN_X + pendingLane * LANE_X, y: ORIGIN_Y + row * ROW_Y },
      data: candidateToNodeData(candidate),
    });
  });

  return nodes;
}

/**
 * Derive source-anchor provenance edges (read-only) from the projection.
 *
 * Each anchor links a source reference to its backing entity; we render it as
 * an undirected-style edge from the source-anchor node to the entity node.
 * `world_kb.patch_relationship` and a `kb_relationships` table are V1.74.
 */
export function deriveEdges(
  anchors: WorldKbSourceAnchorProjection[],
): Edge[] {
  return anchors.map((anchor) => {
    const data: WorldKbEdgeData = {
      relationType: 'source_anchor',
      sourceAnchorIds: [anchor.source_anchor_id],
    };
    return {
      id: `anchor:${anchor.source_anchor_id}`,
      source: `anchor-node:${anchor.source_anchor_id}`,
      target: `entity:${anchor.key_block_id}`,
      type: 'straight',
      data,
      // Read-only: no patch affordance in V1.73.
      selectable: false,
      focusable: true,
    } satisfies Edge;
  });
}

/**
 * Lightweight source-anchor nodes (one per anchor) so provenance edges have a
 * visible origin. Rendered with the soft `source-anchor-node` fill.
 */
export function anchorNodes(anchors: WorldKbSourceAnchorProjection[]): Node[] {
  /** One column per target entity keeps anchors visually adjacent to their entity. */
  const seenEntities = new Set<string>();
  return anchors.map((anchor, i) => {
    const dup = seenEntities.has(anchor.key_block_id);
    seenEntities.add(anchor.key_block_id);
    return {
      id: `anchor-node:${anchor.source_anchor_id}`,
      type: 'worldkb-source-anchor',
      position: { x: ORIGIN_X - 120, y: ORIGIN_Y + i * 56 + (dup ? 16 : 0) },
      data: {
        relationType: 'source_anchor' as const,
        reference: anchor.reference,
        sourceType: anchor.source_type,
      },
    };
  });
}

/** Human-readable summary for the canvas screen-reader region + alt view. */
export function graphSummary(
  graph: WorldKbGraphResponse | undefined,
  candidateCount: number,
): string {
  if (!graph) return 'World KB graph not loaded.';
  const entityCount = graph.entities.length;
  const anchorCount = graph.source_anchors.length;
  return `World KB graph: ${entityCount} ${entityCount === 1 ? 'entity' : 'entities'}, ${anchorCount} ${anchorCount === 1 ? 'source anchor' : 'source anchors'}, ${candidateCount} pending ${candidateCount === 1 ? 'candidate' : 'candidates'}.`;
}

/** Stable count of confirmed entities + pending candidates for the freshness indicator. */
export function entryCountOf(
  graph: WorldKbGraphResponse | undefined,
  candidateCount: number,
): number {
  return (graph?.entities.length ?? 0) + candidateCount;
}

export { BLOCK_TYPE_LABELS };
