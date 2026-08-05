/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/moment-directive — set/show/clear the active Moment Directive (V1.151 P0 DF-76). Validation mirrors the CLI handle_set (apps/nexus42/src/commands/creator/moment_directive.rs).
 */
export interface MomentDirectiveRequest {
  /**
   * Directive operation: set (create/replace), show (read active), clear (remove active).
   */
  action: "set" | "show" | "clear";
  scope: {
    /**
     * Scope kind of the directive.
     */
    kind: "work" | "world";
    /**
     * Work id (kind=work) or world id (kind=world) the directive is scoped to.
     */
    id: string;
  };
  /**
   * Author instruction text (required for action=set).
   */
  body?: string;
  /**
   * Placement within the directive region. Defaults to tail.
   */
  insert_depth?: "head" | "mid" | "tail";
  /**
   * TTL kind: count down by assembling generations or chapter advances.
   */
  ttl_kind?: "generations" | "chapters";
  /**
   * TTL count (spec §5 H5 lock: the input name matches the read-back `ttl_remaining` column).
   */
  ttl_remaining?: number;
  /**
   * Clear when the focused moment anchor changes between assembles.
   */
  clear_on_scene_change?: boolean;
  /**
   * Replace an existing directive at the same scope.
   */
  replace?: boolean;
}
