/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Provenance edge projection derived from kb_source_anchors (V1.73). Rendered read-only on the canvas graph.
 */
export interface WorldKbSourceAnchorProjection {
  /**
   * Source anchor identifier.
   */
  source_anchor_id: string;
  /**
   * KeyBlock the anchor attaches to.
   */
  key_block_id: string;
  /**
   * Origin kind (e.g. chapter, review, manual).
   */
  source_type: string;
  /**
   * Locator string (path / chapter ref / review id).
   */
  reference: string;
  /**
   * ISO-8601 timestamp.
   */
  created_at?: string;
}
