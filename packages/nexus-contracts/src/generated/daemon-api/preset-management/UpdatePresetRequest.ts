import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus UpdatePresetRequest
 *
 * Request body for PATCH /v1/daemon/presets/{id} (V1.65 P0). Replaces the user preset's preset.yaml content after validation.
 *
 * @schema_version 1
 * @source update-preset-request.schema.json
 */
/** Request body for PATCH /v1/daemon/presets/{id} (V1.65 P0). Replaces the user preset's preset.yaml content after validation. */
export interface UpdatePresetRequest {
  yaml: string;
}
