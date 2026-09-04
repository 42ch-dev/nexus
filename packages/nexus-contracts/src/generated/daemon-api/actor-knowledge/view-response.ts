/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed canonical KnowledgeEntry owner union: World | Character | ActorWorldBinding. Wire shape matches the domain KnowledgeOwnerRef (kind + id).
 */
export type NexusKnowledgeOwnerRef = WorldKnowledgeOwner | CharacterKnowledgeOwner | BindingKnowledgeOwner;

/**
 * Response for POST /v1/daemon/actor-knowledge/view. All-or-error: never a partial page.
 */
export interface ViewResponse {
  items: NexusActorKnowledgeViewItem[];
  pagination: NexusPaginationInfo;
}
/**
 * One KnowledgeEntry in an Actor KnowledgeView or Character knowledge list, with deterministic stored-owner metadata.
 */
export interface NexusActorKnowledgeViewItem {
  entry_id: string;
  owner: NexusKnowledgeOwnerRef;
  creator_only: boolean;
  /**
   * KnowledgeEntry content type (data-model-v1.md §5.5). V1.54 P1: added game-bible variants (species, faction, magic_system, technology, deity, level, economy_tier). V1.55 P3: added script variants (dialogue, beat, act). V1.123 P1: added era (cross-profile world-shape marker for Brief layer).
   */
  block_type:
    | "character"
    | "ability"
    | "scene"
    | "organization"
    | "item"
    | "conflict"
    | "info_point"
    | "event"
    | "species"
    | "faction"
    | "magic_system"
    | "technology"
    | "deity"
    | "level"
    | "economy_tier"
    | "dialogue"
    | "beat"
    | "act"
    | "era";
  canonical_name: string;
  /**
   * KnowledgeEntry status (data-model-v1.md §5.5)
   */
  status: "provisional" | "confirmed" | "deprecated" | "merged" | "deleted";
  /**
   * ISO 8601 / RFC 3339 UTC datetime string
   */
  created_at: string;
}
export interface WorldKnowledgeOwner {
  /**
   * World-owned KnowledgeEntry.
   */
  kind: string;
  /**
   * World ID (prefix: 'wld_')
   */
  id: string;
}
export interface CharacterKnowledgeOwner {
  /**
   * Character-owned KnowledgeEntry shared across active bindings.
   */
  kind: string;
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  id: string;
}
export interface BindingKnowledgeOwner {
  /**
   * Binding-local KnowledgeEntry isolated to one ActorWorldBinding.
   */
  kind: string;
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  id: string;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
