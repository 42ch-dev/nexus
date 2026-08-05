/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/worlds/:world_id/kb/pack/import — import a Narrative Knowledge Pack into a World (V1.152 P0 DF-77). The pack is an opaque handbook pack object; conflict policy is required semantic state.
 */
export interface PackImportRequest {
  /**
   * Opaque Narrative Knowledge Pack (spoke handbook shape: modules.pack + entries + relations + optional source_anchors). Parsed server-side via nexus_spoke_adapter::pack::parse_pack.
   */
  pack: {
    [k: string]: unknown | undefined;
  };
  /**
   * Collision policy: skip (keep existing, default), rename (disambiguate and create), overwrite (replace one entry body, behind confirmation).
   */
  conflict: "skip" | "rename" | "overwrite";
  /**
   * Whether to import source_anchors from the pack.
   */
  include_anchors?: boolean;
}
