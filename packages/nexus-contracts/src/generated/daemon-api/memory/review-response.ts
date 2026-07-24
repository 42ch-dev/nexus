/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/memory/review. Summarizes how many pending entries were promoted to long-term memory, fragmented, or dropped by the rule-based classifier. Shipped behavior: PassthroughSummarizer (no LLM); each pending row is classified and the pending row is deleted on promote/fragment/drop success.
 */
export interface ReviewResponse {
  promoted: number;
  fragmented: number;
  dropped: number;
  /**
   * V1.80 REL-01 additive: when true the pending queue was not fully drained by this call (the bounded fetch found more rows, or the per-call review budget expired mid-batch). The client should re-issue POST /review to drain the remainder. Omitted by pre-V1.80 daemons; consumers treat absent as false.
   */
  has_more?: boolean;
  /**
   * V1.80 REL-01 additive: number of pending rows inspected (classified + action attempted) during this call. Used by the client's zero-progress drain guard. Omitted by pre-V1.80 daemons; consumers treat absent as 0.
   */
  processed?: number;
}
