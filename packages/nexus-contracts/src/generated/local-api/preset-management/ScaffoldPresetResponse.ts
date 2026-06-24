import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ScaffoldPresetResponse
 *
 * Response for POST /v1/local/presets — scaffold result with created paths.
 *
 * @schema_version 1
 * @source scaffold-preset-response.schema.json
 */
/** Response for POST /v1/local/presets — scaffold result with created paths. */
export interface ScaffoldPresetResponse {
  id: string;
  path: string;
}
