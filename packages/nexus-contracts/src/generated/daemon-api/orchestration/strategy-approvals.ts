/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response of GET /v1/daemon/strategies/{strategy_id}/approvals — every approval of the strategy in the same total order the applied status read uses to pick `newest_approval`: `candidate_revision` DESC, then `approved_at` DESC, then `approval_id` DESC. `items[0]` is therefore the status read's `newest_approval`, and no client re-sorts or derives newness from a timestamp. The list exists so a client can aim Apply at a superseded approval — the rollback path — instead of holding only the newest.
 */
export interface StrategyApprovals {
  /**
   * Every approval of this strategy, newest first by the total order above. Empty when nothing was approved.
   */
  items: StrategyApproval[];
}
/**
 * One approval row, reduced to the keys a client needs to list it and to aim an apply at it.
 *
 * This interface was referenced by `StrategyApprovals`'s JSON-Schema
 * via the `definition` "StrategyApproval".
 */
export interface StrategyApproval {
  /**
   * Approval identity; the value a client compares with the current apply's and the newest approval's `approval_id`.
   */
  approval_id: string;
  /**
   * The content hash applying this approval would install; the apply request names this value.
   */
  content_hash: string;
  /**
   * Candidate revision the approval froze. This key, not either timestamp, defines approval newness.
   */
  candidate_revision: number;
  /**
   * RFC 3339 UTC timestamp of the approval; it orders tied revisions only.
   */
  approved_at: string;
}
