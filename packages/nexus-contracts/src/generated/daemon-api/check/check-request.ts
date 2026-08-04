/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/check — run spoke orchestrate_check over an owned World (V1.148 P2).
 */
export interface CheckRequest {
  /**
   * World ownership key; must match active creator ownership (is_world_owned).
   */
  world_id: string;
  /**
   * Spoke Scope selector (scope_id should equal world_id). Mirrors spoke ops CheckRequest.scope fields needed by codegen.
   */
  scope: {
    scope_id: string;
    entry_ids?: string[];
    entry_types?: string[];
    timeline_event_ids?: string[];
    timeline_scale?: string;
    fork_id?: string;
    source_id?: string;
    extensions?: {
      [k: string]: unknown | undefined;
    };
  };
  /**
   * Opaque rule ids resolved via RuleQueryPort (P1).
   */
  rule_refs?: string[];
  /**
   * Optional embedded spoke Rule objects (portable interchange). Prefer $ref to spoke rule schema if monorepo codegen supports external $ref; else opaque object validated when mapped to spoke Rule.
   */
  rules?: {
    [k: string]: unknown | undefined;
  }[];
  checker_kinds?: string[];
  extensions?: {
    [k: string]: unknown | undefined;
  };
}
