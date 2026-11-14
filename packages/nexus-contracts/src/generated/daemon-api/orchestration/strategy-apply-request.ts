/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Body of POST /v1/daemon/strategies/{strategy_id}/apply. Apply installs one already-approved snapshot as the strategy's running identity: the request names the approval by its content hash only, because the approval row is immutable and already carries the document, hash, pins and trajectory. The path `strategy_id` is authoritative; a hash approved for another strategy is refused rather than paired with this one.
 */
export interface StrategyApplyRequest {
  /**
   * Canonical content hash of the approval to apply, as the approve response reported it. The daemon recomputes the hash from the approval's frozen document before writing, so a hash whose bytes moved is refused instead of installed.
   */
  approval_content_hash: string;
}
