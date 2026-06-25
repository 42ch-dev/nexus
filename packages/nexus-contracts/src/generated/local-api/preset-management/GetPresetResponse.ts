import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus GetPresetResponse
 *
 * Response for GET /v1/local/presets/{id} (V1.65 P0). Returns the preset manifest as raw YAML so clients can edit and PATCH it back.
 *
 * @schema_version 1
 * @source get-preset-response.schema.json
 */

/** Inline enum type */
export type GetPresetResponseSource = 'embedded' | 'system' | 'user';

/** Response for GET /v1/local/presets/{id} (V1.65 P0). Returns the preset manifest as raw YAML so clients can edit and PATCH it back. */
export interface GetPresetResponse {
  id: string;
  source: GetPresetResponseSource;
  path?: string;
  yaml: string;
}
