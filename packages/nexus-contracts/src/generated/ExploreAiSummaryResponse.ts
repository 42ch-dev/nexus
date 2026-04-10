import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus ExploreAiSummaryResponse
 *
 * Response for Explore AI summarization (platform plan 19).
 *
 * @schema_version 1
 * @source explore-ai-summary-response.schema.json
 */
/** Response for Explore AI summarization (platform plan 19). */
export interface ExploreAiSummaryResponse {
  schema_version: number;
  summary: string;
  model?: string;
}
