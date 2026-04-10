import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus ExploreAiAnswerRequest
 *
 * Request body for Explore AI grounded Q&A over world / corpus context (platform plan 19). Boundary with context assembly: this is platform-side retrieval + generation; wire shape only.
 *
 * @schema_version 1
 * @source explore-ai-answer-request.schema.json
 */
/** Request body for Explore AI grounded Q&A over world / corpus context (platform plan 19). Boundary with context assembly: this is platform-side retrieval + generation; wire shape only. */
export interface ExploreAiAnswerRequest {
  schema_version: number;
  query: string;
  world_id?: string;
  max_citations?: number;
}
