import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus PresetSummary
 *
 * Summary of a single preset entry (id, source, run intents).
 *
 * @schema_version 1
 * @source preset-summary.schema.json
 */

/** Inline enum type */
export type PresetSummarySource = 'embedded' | 'system' | 'user';

/** Summary of a single preset entry (id, source, run intents). */
export interface PresetSummary {
  id: string;
  source: PresetSummarySource;
  run_intents?: string[];
}
