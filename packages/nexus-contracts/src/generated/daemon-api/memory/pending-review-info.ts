/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * A single pending-review row in list/get responses. Mirrors the `memory_pending_review` table projection 1:1. `task_kind` and `created_at` are always present here (defaults are applied server-side on insert), unlike the create-request where they are optional. `world_id` is nullable.
 */
export interface PendingReviewInfo {
  pending_id: string;
  session_id: string;
  creator_id: string;
  world_id?: string;
  task_kind: string;
  raw_digest: string;
  created_at: string;
}
