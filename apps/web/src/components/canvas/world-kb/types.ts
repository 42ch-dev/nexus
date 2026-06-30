/**
 * World KB canvas projection types (canvas-strategy-surface.md §3.4).
 *
 * These mirror the design-language contracts `WorldKbNodeData` /
 * `WorldKbEdgeData` from the Draft spec, now backed by the V1.73 generated
 * projections. They are UI-only: the wire DTOs in `@42ch/nexus-contracts`
 * remain the single source of truth for transport shapes.
 */
import type {
  BlockType,
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbSourceAnchorProjection,
} from '@42ch/nexus-contracts';

/**
 * Lifecycle rendered as a promotion-state badge on every World KB node.
 *
 * Pending candidates come from `kb_extract_jobs`; confirmed/rejected/merged
 * come from the KeyBlock `status` column (entity-scope-model §5.5).
 */
export type EntityLifecycle = 'pending' | 'confirmed' | 'rejected' | 'merged';

/** Node data payload consumed by the World KB custom React Flow node. */
export interface WorldKbNodeData {
  /** React Flow requires an index signature on node data. */
  [key: string]: unknown;
  worldId: string;
  /** Present for confirmed/rejected/merged KeyBlock entities. */
  keyBlockId?: string;
  /** Present for pending promotion candidates. */
  candidateId?: string;
  /** Present when the node originates from a promotion candidate row. */
  jobId?: string;
  entityKind: BlockType;
  name: string;
  lifecycle: EntityLifecycle;
  /** Per-row OCC version for conflict anticipation (NULL-normalized to 0). */
  version: number;
  /** Count of source-anchor provenance edges (entities only). */
  sourceAnchorCount: number;
  /** Whether this node represents a computable block (derived badge). */
  computable: boolean;
  /** Wall-clock timestamp of last version bump (ISO string). */
  updatedAt?: string;
}

/** Edge data payload for source-anchor provenance + read-only relationships. */
export interface WorldKbEdgeData {
  /** React Flow requires an index signature on edge data. */
  [key: string]: unknown;
  relationType: 'source_anchor' | 'relationship';
  /** Source-anchor ids backing this edge (empty for V1.74+ relationships). */
  sourceAnchorIds: string[];
  /** Confidence in [0,1]; omitted for source anchors (provenance is exact). */
  confidence?: number;
  /** Promotion state snapshot if the edge records a promotion event. */
  promotionState?: EntityLifecycle;
  /** V1.76: true when the edge is an extraction suggestion (needs_review=1). */
  needsReview?: boolean;
  /** V1.76: relationship provenance — 'manual' (author) or 'extraction'. */
  source?: 'manual' | 'extraction';
}

/** All BlockType variants that may appear as World KB entities. */
export const WORLD_KB_BLOCK_TYPES: readonly BlockType[] = [
  'character',
  'ability',
  'scene',
  'organization',
  'item',
  'conflict',
  'info_point',
  'event',
  'species',
  'faction',
  'magic_system',
  'technology',
  'deity',
  'level',
  'economy_tier',
  'dialogue',
  'beat',
  'act',
] as const;

/** Human-readable label for each BlockType (Title Case per DESIGN.md voice). */
export const BLOCK_TYPE_LABELS: Record<BlockType, string> = {
  character: 'Character',
  ability: 'Ability',
  scene: 'Scene',
  organization: 'Organization',
  item: 'Item',
  conflict: 'Conflict',
  info_point: 'Info Point',
  event: 'Event',
  species: 'Species',
  faction: 'Faction',
  magic_system: 'Magic System',
  technology: 'Technology',
  deity: 'Deity',
  level: 'Level',
  economy_tier: 'Economy Tier',
  dialogue: 'Dialogue',
  beat: 'Beat',
  act: 'Act',
};

/**
 * Map a raw `status` string from a KeyBlock / candidate projection to the
 * lifecycle badge. Unknown / direct-insert statuses normalize to `confirmed`
 * (entity-scope-model §5.5: `manual` direct inserts render as confirmed).
 */
export function lifecycleFromStatus(status: string | undefined): EntityLifecycle {
  const s = (status ?? '').toLowerCase();
  if (s === 'pending') return 'pending';
  if (s === 'rejected') return 'rejected';
  if (s === 'merged') return 'merged';
  // confirmed, manual, empty, or any unrecognized → confirmed (terminal-ish display).
  return 'confirmed';
}

/** True when a BlockType represents a derived/computable block kind. */
export function isComputableBlock(kind: BlockType): boolean {
  return kind === 'beat' || kind === 'act' || kind === 'economy_tier';
}

/**
 * Stable row/node id for a World KB node, prefixed by kind so entity and
 * candidate ids can never collide (`entity:<keyBlockId>` vs
 * `candidate:<candidateId>`).
 *
 * This is the SINGLE source of the prefixed format — both the alt-view row
 * keys and the canvas `selectedId` prop MUST be produced by this helper so the
 * selection-highlight contract holds. Hand-rolling the format in either place
 * caused V1.73 greploop issue 1 (raw id vs prefixed id → no highlight).
 */
export function worldKbNodeId(node: WorldKbNodeData): string {
  return node.candidateId ? `candidate:${node.candidateId}` : `entity:${node.keyBlockId}`;
}

export type {
  WorldKbCandidateProjection,
  WorldKbEntityProjection,
  WorldKbSourceAnchorProjection,
};
