import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus ExploreAiSummaryRequest
 *
 * Request body for Explore AI summarization over a world or manuscript (platform plan 19).
 *
 * @schema_version 1
 * @source explore-ai-summary-request.schema.json
 */

/** Inline enum type */
export type ExploreAiSummaryRequestScope = 'world' | 'manuscript';

/** Request body for Explore AI summarization over a world or manuscript (platform plan 19). */
export interface ExploreAiSummaryRequest {
  schema_version: number;
  scope: ExploreAiSummaryRequestScope;
  entity_id: string;
  max_length?: number;
}
