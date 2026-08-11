/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/moment-directive (V1.151 P0 DF-76; DR-63 schema-tightening). Formalizes the live wire shape the handler already emits: `set`/`show` return the full directive row (all 15 fields, incl. body — the author surface, NOT the inspector packet; AC-I3 governs only the packet's status-only `moment_directive` section), and `show` with no effective directive / `clear` return `{}`. Both branches are honest payloads today, so both are valid responses; the schema adds the typed contract without any wire change (additive only).
 */
export type MomentDirectiveResponse =
  NexusDaemonMomentDirectiveResponseDirective | NexusDaemonMomentDirectiveResponseEmpty;

/**
 * The active directive row (set, or show with an effective directive). Mirrors `nexus_local_db::moment_directive::MomentDirectiveRow` 1:1 — the nullable fields serialize as explicit JSON null (never omitted), matching the row's `serde::Serialize` output.
 */
export interface NexusDaemonMomentDirectiveResponseDirective {
  /**
   * Unique directive id (application-generated, `dir_<uuid v4>`).
   */
  directive_id: string;
  /**
   * Owning creator.
   */
  creator_id: string;
  /**
   * Scope kind of the directive: work | world.
   */
  scope_kind: "work" | "world";
  /**
   * Work id (scope_kind=work) or world id (scope_kind=world) the directive is scoped to. For an inherited World override on a Work show, names the inherited source.
   */
  scope_id: string;
  /**
   * Author instruction text (author surface — this route carries the body by design; the inspector packet's `moment_directive` section is status-only per AC-I3).
   */
  body: string;
  /**
   * Placement within the directive region: head | mid | tail.
   */
  insert_depth: "head" | "mid" | "tail";
  /**
   * TTL kind: count down by assembling generations or chapter advances.
   */
  ttl_kind: "generations" | "chapters";
  /**
   * Remaining TTL count (decremented in place; 0 ⇒ expired, so an active row always carries >= 1).
   */
  ttl_remaining: number;
  /**
   * Clear when the focused moment anchor changes between assembles.
   */
  clear_on_scene_change: boolean;
  /**
   * Directive lifecycle status: active | expired (soft-delete).
   */
  status: "active" | "expired";
  /**
   * Last focused `MomentRequest.event_id` seen at an injecting assemble (scene-change signal). Null until the first injection.
   */
  last_focused_event_id: string | null;
  /**
   * Unix epoch millis when created.
   */
  created_at: number;
  /**
   * Unix epoch millis of the last lifecycle write.
   */
  updated_at: number;
  /**
   * Unix epoch millis when soft-deleted (TTL-0 / scene-clear / manual clear). Null while active.
   */
  expires_at: number | null;
  /**
   * New directive id when `--replace` superseded this row. Null otherwise.
   */
  replaced_by: string | null;
}
/**
 * No directive: `show` with no effective directive for the scope, or a successful `clear`. An empty object `{}` — the pre-schema wire behavior, unchanged.
 */
export interface NexusDaemonMomentDirectiveResponseEmpty {
  [k: string]: unknown | undefined;
}
