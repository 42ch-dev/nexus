import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbSourceAnchorProjection
 *
 * Provenance edge projection derived from kb_source_anchors (V1.73). Rendered read-only on the canvas graph.
 *
 * @schema_version 1
 * @source world-kb-source-anchor-projection.schema.json
 */
/** Provenance edge projection derived from kb_source_anchors (V1.73). Rendered read-only on the canvas graph. */
export interface WorldKbSourceAnchorProjection {
  source_anchor_id: string;
  key_block_id: string;
  source_type: string;
  reference: string;
  created_at?: string;
}
