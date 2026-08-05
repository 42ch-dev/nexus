/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/worlds/:world_id/kb/pack/export (V1.152 P0 DF-77). The Narrative Knowledge Pack envelope (spoke handbook domain-profile-narrative-knowledge-pack). Entries and relations are opaque spoke objects (V1.139 fallback).
 */
export interface PackExportResponse {
  /**
   * modules.pack catalog metadata plus any unknown modules.* dialect keys (round-trip verbatim).
   */
  modules: {
    [k: string]: unknown | undefined;
  };
  /**
   * Ordered KnowledgeEntry list (canonical_name ASC).
   */
  entries: {
    [k: string]: unknown | undefined;
  }[];
  /**
   * Ordered Relation list (relationship_id ASC).
   */
  relations: {
    [k: string]: unknown | undefined;
  }[];
  /**
   * Optional SourceAnchor list (present only when include_anchors is set on the request).
   */
  source_anchors?: {
    [k: string]: unknown | undefined;
  }[];
}
