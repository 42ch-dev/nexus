import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ScaffoldPresetRequest
 *
 * Request body for POST /v1/local/presets — scaffold a new user preset.
 *
 * @schema_version 1
 * @source scaffold-preset-request.schema.json
 */
/** Request body for POST /v1/local/presets — scaffold a new user preset. */
export interface ScaffoldPresetRequest {
  name: string;
}
