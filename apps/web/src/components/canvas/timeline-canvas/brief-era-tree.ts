/**
 * Brief era taxonomy tree builder — V1.159 P1 Task 1.
 *
 * Converts the flat `WorldKbGraphResponse` era data into a nested
 * `EraTreeNode[]` forest for the World Timeline Brief layer time-band
 * rendering (DF-V1123-ERA-TAXONOMY).
 *
 * Architect-locked contract — see
 * `.mstar/specs/canvas-strategy-surface.md` §3.3.3 (V1.159 amendment) +
 * `.mstar/iterations/v1.159/specs/product-locks.md` (VC-1 option c, Q1–Q3):
 *   - Input entities: `block_type === "era"` KnowledgeEntry projections.
 *   - Nesting carrier: `relation_type === "custom"` AND
 *     `custom_label === "parent_era"` (the wire schema's sanctioned escape
 *     hatch for domain relationship kinds; `wire_contracts_changed: false`).
 *   - Edge direction: `source_entity_id` = parent (coarser era),
 *     `target_entity_id` = child (finer era); `symmetric: false`.
 *   - Both endpoints of a usable parent edge MUST be era entities (architect
 *     Q3: "filters era-era parent edges … AND both endpoints are era
 *     entities"); edges touching non-era or non-existent entities are
 *     skipped (orphan handling — never crash).
 *   - Roots = eras with no parent_era edge targeting them; DFS with a
 *     visited-set cycle guard: a node encountered again (back-edge) is
 *     placed at first encounter only and not re-rendered; a warning is
 *     logged. No hard depth cap — era taxonomy is naturally shallow
 *     (kingdom → age → epoch → period → sub-age ≈ 5 levels) and the cycle
 *     guard is the correctness invariant (architect Q1).
 *   - Flat input (no parent_era edges) → every era is a depth-0 root
 *     (backward compatible with V1.156 flat Brief rendering).
 *
 * Ordering is deterministic: eras and sibling lists are sorted by
 * `key_block_id` so the forest is stable across refetches (mirrors the
 * adapter's stable-id convention).
 */
import type {
  WorldKbEntityProjection,
  WorldKbRelationshipProjection,
} from '@42ch/nexus-contracts';

/** The custom-relation label that carries era nesting (architect VC-1 option c). */
const PARENT_ERA_LABEL = 'parent_era';

/**
 * One node of the Brief era forest. `era_type` rides in the freeform
 * `body.attributes.era_type` string and may be absent (legacy flat-era data
 * renders compatibly — no validation enum).
 */
export interface EraTreeNode {
  /** The era KnowledgeEntry (`block_type === 'era'`). */
  era: WorldKbEntityProjection;
  /** Freeform era type from `body.attributes.era_type`; absent when missing/empty. */
  era_type: string | undefined;
  /** Sub-eras (finer taxons) nested under this era, in deterministic order. */
  children: EraTreeNode[];
  /** 0 = root; 1+ = nesting level under a root. */
  depth: number;
}

function isParentEraRelationship(
  rel: WorldKbRelationshipProjection,
): boolean {
  return (
    rel.relation_type === 'custom' && rel.custom_label === PARENT_ERA_LABEL
  );
}

/**
 * Extract the freeform era type from `body.attributes.era_type`. Type
 * narrowing mirrors `occurredAtOf` / `extractEraAttributes` in the Timeline
 * adapter — `body.attributes` is `unknown` until narrowed; non-string values
 * and empty strings are treated as absent.
 */
function eraTypeOf(entity: WorldKbEntityProjection): string | undefined {
  const attrs = entity.body?.attributes;
  if (attrs === null || typeof attrs !== 'object') return undefined;
  const raw = (attrs as Record<string, unknown>).era_type;
  return typeof raw === 'string' && raw.length > 0 ? raw : undefined;
}

/**
 * Build the nested era forest from flat entities + relationships.
 *
 * @param entities      `WorldKbGraphResponse.entities` (era entities are
 *                      filtered internally; non-era entities are ignored).
 * @param relationships `WorldKbGraphResponse.relationships` (only
 *                      `custom`/`parent_era` edges are considered).
 * @returns A forest of `EraTreeNode`s: roots are eras with no parent_era
 *          edge targeting them; every remaining era (cycle islands, e.g.
 *          A→B→A) is still placed exactly once as a depth-0 root so no era
 *          vanishes from the Brief layer.
 */
export function buildEraTree(
  entities: WorldKbEntityProjection[],
  relationships: WorldKbRelationshipProjection[],
): EraTreeNode[] {
  // Deterministic era set — stable iteration order across refetches.
  const eras = entities
    .filter((e) => e.block_type === 'era')
    .slice()
    .sort((a, b) => a.key_block_id.localeCompare(b.key_block_id));
  const eraById = new Map(eras.map((e) => [e.key_block_id, e]));

  // Valid parent edges only: both endpoints must be era entities (architect
  // Q3). Orphan edges (target missing from the set) are skipped silently.
  const childrenByParent = new Map<string, string[]>();
  const hasParent = new Set<string>();
  for (const rel of relationships) {
    if (!isParentEraRelationship(rel)) continue;
    const parent = eraById.get(rel.source_entity_id);
    const child = eraById.get(rel.target_entity_id);
    if (parent === undefined || child === undefined) continue;
    const siblings = childrenByParent.get(parent.key_block_id);
    if (siblings === undefined) {
      childrenByParent.set(parent.key_block_id, [child.key_block_id]);
    } else {
      siblings.push(child.key_block_id);
    }
    hasParent.add(child.key_block_id);
  }
  for (const siblings of childrenByParent.values()) {
    siblings.sort((a, b) => a.localeCompare(b));
  }

  const visited = new Set<string>();
  const tree: EraTreeNode[] = [];

  const visit = (id: string, depth: number): EraTreeNode => {
    // `id` is always a known era: roots/sweep iterate `eras`, and children
    // come from `childrenByParent`, which only records validated era ids.
    const era = eraById.get(id) as WorldKbEntityProjection;
    visited.add(id);
    const node: EraTreeNode = {
      era,
      era_type: eraTypeOf(era),
      children: [],
      depth,
    };
    for (const childId of childrenByParent.get(id) ?? []) {
      if (visited.has(childId)) {
        // Back-edge (cycle A→B→A) or duplicate parent edge: the child is
        // already placed at its first encounter — do not re-render it here.
        // eslint-disable-next-line no-console
        console.warn(
          `[brief-era-tree] skipped parent_era edge "${id}" -> "${childId}": ` +
            `"${childId}" already placed (cycle or duplicate edge)`,
        );
        continue;
      }
      node.children.push(visit(childId, depth + 1));
    }
    return node;
  };

  // Roots: eras with no parent_era edge targeting them.
  for (const era of eras) {
    if (!hasParent.has(era.key_block_id)) {
      tree.push(visit(era.key_block_id, 0));
    }
  }

  // Cycle islands: eras unreachable from any root are still placed once, as
  // depth-0 roots — a cycle flattens instead of vanishing or recursing.
  for (const era of eras) {
    if (!visited.has(era.key_block_id)) {
      tree.push(visit(era.key_block_id, 0));
    }
  }

  return tree;
}
