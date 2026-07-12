import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbKeyBlockStateResponse
 *
 * Read projection for GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state (V1.114 P2). Surfaces the mutable runtime state of a computable KeyBlock plus its computability flag and per-row OCC version.
 *
 * @schema_version 1
 * @source world-kb-key-block-state-response.schema.json
 */
/** Read projection for GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state (V1.114 P2). Surfaces the mutable runtime state of a computable KeyBlock plus its computability flag and per-row OCC version. */
export interface WorldKbKeyBlockStateResponse {
  state: Record<string, unknown> | null;
  is_computable: boolean;
  version: number;
}
