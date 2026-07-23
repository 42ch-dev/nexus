/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/memory/soul/reflect. Absent/null `world_id` reads or regenerates the Creator-level narrative; a present `world_id` scopes read/regeneration to that world's per-World narrative (ownership verified server-side).
 */
export interface SoulNarrativeRequest {
  /**
   * Active creator identifier; the endpoint rejects mismatches through the existing Daemon API auth/creator checks.
   */
  creator_id: string;
  /**
   * Optional world scope. Absent or null = the Creator-level narrative (V1.81 behavior, world-agnostic). Present = synthesize/return the per-World narrative for that world's fragment subset; the endpoint verifies the active creator owns the world via `narrative_worlds.owner_creator_id` and rejects non-owned worlds.
   */
  world_id?: string;
  /**
   * When true, synthesize and overwrite the cached narrative even if a current or stale cache row exists. When omitted or false, the endpoint is read/status-only and does not invoke the LLM.
   */
  force_regenerate?: boolean;
}
