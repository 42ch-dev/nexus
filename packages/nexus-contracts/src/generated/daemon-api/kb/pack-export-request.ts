/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/worlds/:world_id/kb/pack/export — export one World's lore as a Narrative Knowledge Pack (V1.152 P0 DF-77). The export reads through the exporting Creator's admitted selection: shared rows always, owned known-private material only under explicit author intent.
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
   * Explicit author intent to include owned known-private material in the export (v1.191 P1 T10, holder-governance.md §6). Absent/false exports only the shared rows the exporting Creator's admitted policy may read; private material of another holder and quarantined import atoms are never emitted. Character/Connect exports never set this.
   */
  include_owned_private?: boolean;
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
