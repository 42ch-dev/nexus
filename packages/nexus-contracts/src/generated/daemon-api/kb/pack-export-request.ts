/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/worlds/:world_id/kb/pack/export — export one World's lore as a Narrative Knowledge Pack (V1.152 P0 DF-77).
 */
export interface PackExportRequest {
  /**
   * Include deprecated entries in the export (default: active entries only).
   */
  include_deprecated?: boolean;
  /**
   * Include source_anchors in the export response envelope.
   */
  include_anchors?: boolean;
  /**
   * Override modules.pack.title (default: World title).
   */
  title?: string;
  /**
   * Override modules.pack.version (default: 0.1.0).
   */
  pack_version?: string;
  /**
   * Optional modules.pack.description.
   */
  description?: string;
}
