import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ValidatePresetRequest
 *
 * Request body for POST /v1/local/presets:validate.
 *
 * @schema_version 1
 * @source validate-preset-request.schema.json
 */
/** Request body for POST /v1/local/presets:validate. */
export interface ValidatePresetRequest {
  path: string;
}
