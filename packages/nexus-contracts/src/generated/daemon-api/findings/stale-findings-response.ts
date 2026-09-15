/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for `GET /v1/daemon/findings/stale` (V1.39 P4 T3): open findings for the active creator that aged past the stale threshold (default 96h; the threshold resolution stays at the adapter because the watcher schedule is daemon runtime state). v2 corrects the schema to the live wire envelope (`stale_count` / `threshold_seconds` / `now_epoch` / `findings`); the previous `open_count`/`items` shape was never emitted by the handler.
 */
export interface StaleFindingsResponse {
  /**
   * Number of open findings older than `threshold_seconds`.
   */
  stale_count: number;
  /**
   * Threshold (seconds) used for the query.
   */
  threshold_seconds: number;
  /**
   * Server-side epoch second used as `now` for the cutoff calculation.
   */
  now_epoch: number;
  /**
   * Per-finding summaries, oldest first; the CLI banner surfaces the most-aged item.
   */
  findings: StaleFindingEntry[];
}
/**
 * This interface was referenced by `StaleFindingsResponse`'s JSON-Schema
 * via the `definition` "StaleFindingEntry".
 */
export interface StaleFindingEntry {
  finding_id: string;
  work_id: string;
  severity: string;
  /**
   * Epoch second the finding was created.
   */
  created_at: number;
  age_seconds: number;
}
