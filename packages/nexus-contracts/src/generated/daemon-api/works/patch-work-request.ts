/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/works/{work_id}. `world_id` and `story_ref` are tri-state binding patches: omitted keeps the stored binding, `null` clears it to SQL NULL, a value sets it. Both fields are deliberately unrestricted at the JSON-Schema level: a typed nullable string (`anyOf`/`oneOf` [null, string]) collapses omission and null in the generated carrier, because typify resolves `anyOf` [null, T] to `Option<T>` (typify-impl `maybe_option`, enums.rs:27-52) and a non-required nullable property already carries the option (structs.rs:135-160). The Rust/TS boundary parser owns the three-state grammar and rejects every other value before any stored effect.
 */
export interface PatchWorkRequest {
  title?: string;
  long_term_goal?: string;
  creative_brief?: string;
  intake_status?: string;
  status?: string;
  /**
   * Tri-state World binding patch: omitted = keep the stored binding, null = clear it to SQL NULL, string = set it. Deliberately unrestricted so the generated carrier cannot flatten omission and null; the Rust/TS boundary parser owns the three-state grammar and rejects every other value before any stored effect.
   */
  world_id?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Tri-state story reference patch: omitted = keep the stored reference, null = clear it to SQL NULL, string = set it. Deliberately unrestricted so the generated carrier cannot flatten omission and null; the Rust/TS boundary parser owns the three-state grammar and rejects every other value before any stored effect.
   */
  story_ref?: {
    [k: string]: unknown | undefined;
  };
  primary_preset_id?: string;
  current_stage?: string;
  stage_status?: string;
  force?: boolean;
  auto_review_master_on_timeout?: boolean;
  auto_chain_interrupted?: boolean;
  work_profile?: string;
}
