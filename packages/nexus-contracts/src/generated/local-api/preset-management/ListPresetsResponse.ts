import type { PresetSummary } from './PresetSummary';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListPresetsResponse
 *
 * Response for GET /v1/local/presets — presets grouped by source (embedded, system, user).
 *
 * @schema_version 1
 * @source list-presets-response.schema.json
 */
/** Response for GET /v1/local/presets — presets grouped by source (embedded, system, user). */
export interface ListPresetsResponse {
  embedded: PresetSummary[];
  system: PresetSummary[];
  user: PresetSummary[];
}
