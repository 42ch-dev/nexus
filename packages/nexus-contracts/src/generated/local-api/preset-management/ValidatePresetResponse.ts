import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ValidatePresetResponse
 *
 * Response for POST /v1/local/presets:validate — validation result with structured errors and warnings.
 *
 * @schema_version 1
 * @source validate-preset-response.schema.json
 */
/** Response for POST /v1/local/presets:validate — validation result with structured errors and warnings. */
export interface ValidatePresetResponse {
  valid: boolean;
  id?: string;
  version?: number;
  state_count?: number;
  errors: string[];
  warnings?: string[];
}
