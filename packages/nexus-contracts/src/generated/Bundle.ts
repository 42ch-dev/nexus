/**
 * Nexus CLI Sync Bundle (V1.0)
 *
 * V1.0 sync-specific bundle view for CLI <-> Platform synchronization. Reuses domain bundle envelope with sync-specific constraints.
 *
 * @schema_version 1
 * @source bundle.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

export interface Bundle {
  bundle_type?: unknown;
  manuscript_phase?: unknown;
  output_manuscript?: unknown;
  submitting_creator_id?: unknown;
  deltas?: unknown;
}
