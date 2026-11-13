/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response of GET /v1/daemon/strategies/{strategy_id}/applied, read by both the Canvas and the intake. `current_apply` is the append-only row with no successor — the identity the schedule runs — and is null when the strategy has never been applied. `newest_approval` is the highest `candidate_revision` approval of the same strategy and is null when nothing was approved. A client compares the two `approval_id` values to answer 'is the newest approval the running one'; it never derives newness from `applied_at` or `approved_at`.
 */
export interface StrategyAppliedStatus {
  /**
   * The strategy's running identity, or null when no apply exists yet.
   */
  current_apply: AppliedStrategy | null;
  /**
   * The approval with the highest `candidate_revision` for this strategy, or null when none exists. `approved_at` and `approval_id` order an otherwise tied query and never define newness.
   */
  newest_approval: NewestApproval | null;
}
/**
 * One immutable apply row, also returned by POST /v1/daemon/strategies/{strategy_id}/apply. There is no update or delete path: a rollback is a new row that supersedes this one.
 *
 * This interface was referenced by `StrategyAppliedStatus`'s JSON-Schema
 * via the `definition` "AppliedStrategy".
 */
export interface AppliedStrategy {
  /**
   * Apply identity, assigned by the daemon when the row was written.
   */
  apply_id: string;
  /**
   * The approval this row installed. Pinned to this strategy's own approval.
   */
  approval_id: string;
  /**
   * Canonical content hash recomputed from the approval's frozen document at apply time.
   */
  content_hash: string;
  /**
   * RFC 3339 UTC timestamp of the apply.
   */
  applied_at: string;
  /**
   * The row that was current when this one was written, or null for a strategy's first apply. Together with the absence of a successor it is what makes this row current.
   */
  supersedes_apply_id: string | null;
}
/**
 * The strategy's newest approval, reduced to the keys a client needs to compare it with the current apply.
 *
 * This interface was referenced by `StrategyAppliedStatus`'s JSON-Schema
 * via the `definition` "NewestApproval".
 */
export interface NewestApproval {
  /**
   * Approval identity to compare with the current apply's `approval_id`.
   */
  approval_id: string;
  /**
   * The content hash applying this approval would install.
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
