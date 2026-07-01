import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus SoulNarrativeResponse
 *
 * Response body for POST /v1/local/memory/soul/reflect. Reports the whole-Creator SOUL narrative cache state, stale metadata, current counts, and insufficient-data thresholds.
 *
 * @schema_version 1
 * @source soul-narrative-response.schema.json
 */

/** Inline enum type */
export type SoulNarrativeResponseState = 'ungenerated' | 'current' | 'stale' | 'insufficient_data';

/** Response body for POST /v1/local/memory/soul/reflect. Reports the whole-Creator SOUL narrative cache state, stale metadata, current counts, and insufficient-data thresholds. */
export interface SoulNarrativeResponse {
  creator_id: string;
  state: SoulNarrativeResponseState;
  narrative?: string;
  generated_at?: string;
  stale: boolean;
  fragment_count_at_generation?: number;
  max_fragment_created_at_at_generation?: string;
  current_fragment_count: number;
  current_distinct_keyword_count: number;
  min_fragment_count: number;
  min_distinct_keyword_count: number;
}
