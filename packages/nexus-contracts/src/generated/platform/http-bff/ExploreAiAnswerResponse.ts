import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ExploreAiAnswerResponse
 *
 * Response for Explore AI Q&A with optional citations envelope (platform plan 19).
 *
 * @schema_version 1
 * @source explore-ai-answer-response.schema.json
 */
/** Response for Explore AI Q&A with optional citations envelope (platform plan 19). */
export interface ExploreAiAnswerResponse {
  schema_version: number;
  answer: string;
  citations?: { title: string; snippet?: string; source_ref?: string; entity_id?: string }[];
  model?: string;
}
