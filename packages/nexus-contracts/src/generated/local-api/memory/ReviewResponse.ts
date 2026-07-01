import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ReviewResponse
 *
 * Response body for POST /v1/local/memory/review. Summarizes how many pending entries were promoted to long-term memory, fragmented, or dropped by the rule-based classifier. Shipped behavior: PassthroughSummarizer (no LLM); each pending row is classified and the pending row is deleted on promote/fragment/drop success.
 *
 * @schema_version 1
 * @source review-response.schema.json
 */
/** Response body for POST /v1/local/memory/review. Summarizes how many pending entries were promoted to long-term memory, fragmented, or dropped by the rule-based classifier. Shipped behavior: PassthroughSummarizer (no LLM); each pending row is classified and the pending row is deleted on promote/fragment/drop success. */
export interface ReviewResponse {
  promoted: number;
  fragmented: number;
  dropped: number;
}
