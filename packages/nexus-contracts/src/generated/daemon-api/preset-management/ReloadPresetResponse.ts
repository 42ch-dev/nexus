import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReloadPresetResponse
 *
 * Response for POST /v1/daemon/presets/{id}:reload.
 *
 * @schema_version 1
 * @source reload-preset-response.schema.json
 */
/** Response for POST /v1/daemon/presets/{id}:reload. */
export interface ReloadPresetResponse {
  id: string;
  reloaded: boolean;
}
