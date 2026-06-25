import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus UpdatePresetResponse
 *
 * Response for PATCH /v1/local/presets/{id} (V1.65 P0).
 *
 * @schema_version 1
 * @source update-preset-response.schema.json
 */
/** Response for PATCH /v1/local/presets/{id} (V1.65 P0). */
export interface UpdatePresetResponse {
  id: string;
  updated: boolean;
}
