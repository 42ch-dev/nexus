import type { WorldKbEntityProjection } from './WorldKbEntityProjection';
import type { WorldKbSourceAnchorProjection } from './WorldKbSourceAnchorProjection';
import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbGraphResponse
 *
 * Read projection for GET /v1/local/worlds/{world_id}/kb/graph (V1.73). Entities + source-anchor provenance edges. `relationships` is always empty in V1.73 (no kb_relationships table until V1.74); derived reference edges render read-only from source_anchors.
 *
 * @schema_version 1
 * @source world-kb-graph-response.schema.json
 */
/** Read projection for GET /v1/local/worlds/{world_id}/kb/graph (V1.73). Entities + source-anchor provenance edges. `relationships` is always empty in V1.73 (no kb_relationships table until V1.74); derived reference edges render read-only from source_anchors. */
export interface WorldKbGraphResponse {
  entities: WorldKbEntityProjection[];
  source_anchors: WorldKbSourceAnchorProjection[];
  relationships: Record<string, unknown>[];
}
