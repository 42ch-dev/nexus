/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/inspector/moment (V1.151 P0 DF-76). Mirrors the enriched inspector packet emitted by nexus-moment-context-assembly::inspector::build_inspector_packet 1:1: spoke `modules` (placement + activation_trace), plus three additive product-local sections — `slot_map`, `budget`, `moment_directive` (status/metadata only; the directive body is NEVER on the wire — AC-I3). All sections are always present with nullable/empty values; root keeps additionalProperties: true so product-local sections may grow.
 */
export interface MomentInspectResponse {
  /**
   * Spoke assemble-module recipe (unchanged, AC-I3).
   */
  modules: {
    /**
     * Entries that passed activation (accepted == true).
     */
    placement: {
      /**
       * Stable entry id.
       */
      entry_id: string;
      /**
       * Human-readable entry name.
       */
      canonical_name: string;
      /**
       * Why the entry was matched.
       */
      reason: string;
    }[];
    /**
     * Full per-entry fire/miss trace.
     */
    activation_trace: {
      /**
       * Stable entry id.
       */
      entry_id: string;
      /**
       * Human-readable entry name.
       */
      canonical_name: string;
      /**
       * Why the entry was matched or not.
       */
      reason: string;
      /**
       * Whether the entry ended up in the matched placement.
       */
      accepted: boolean;
    }[];
  };
  /**
   * entry_id → slot id (world.before | default | world.after | kb.outlet.<name> | style.post_history | moment.directive), captured post stage-gate at assembly time.
   */
  slot_map: {
    /**
     * The routed entry's stable id.
     */
    entry_id: string;
    /**
     * The named slot the entry landed in.
     */
    slot: string;
  }[];
  /**
   * Activation token-budget accounting (chars/4 estimates).
   */
  budget: {
    /**
     * Estimated primary-match tokens. Zero when no activation ran.
     */
    primary_tokens_est: number;
    /**
     * Estimated relation-hop tokens. Zero when no activation ran.
     */
    hop_tokens_est: number;
    /**
     * Activation budget cap; null when no activation ran.
     */
    cap: number | null;
    /**
     * Remaining budget after activation; null when no activation ran.
     */
    remaining: number | null;
  };
  /**
   * Status/metadata only — the directive body is excluded by construction (AC-I3); "none" + nulls when no directive injected.
   */
  moment_directive: {
    /**
     * Directive scope kind: work | world. Null when no directive injected.
     */
    scope: string | null;
    /**
     * Work id (scope=work) or world id (scope=world). Null when no directive injected.
     */
    scope_id: string | null;
    /**
     * Placement within the directive region: head | mid | tail. Null when no directive injected.
     */
    insert_depth: string | null;
    /**
     * TTL kind: generations | chapters. Null when no directive injected.
     */
    ttl_kind: string | null;
    /**
     * Remaining TTL count after this assembly's decrement. Null when no directive injected.
     */
    ttl_remaining: number | null;
    /**
     * Clear when the focused moment anchor changes between assembles. False when no directive injected.
     */
    clear_on_scene_change: boolean;
    /**
     * Open string. Core vocabulary (documented, not enforced): none (no directive injected), active (directive injected this assembly).
     */
    status: string;
  };
  [k: string]: unknown | undefined;
}
