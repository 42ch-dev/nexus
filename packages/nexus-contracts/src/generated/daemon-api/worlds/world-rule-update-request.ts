/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/worlds/{world_id}/rules/{rule_id} (V1.169 P1 / AR-1, AR-3). All seven mutable members are optional and per-field replace (present → set, absent → unchanged). No nullable members — values are present-or-absent, no null-clearing (AR-3). At least one member must be present (handler-enforced 400 invalid_input with field=patch). Type structure only — value validation runs in the handler so form-producible failures surface through the field-level envelope (AR-2), never the axum Json extractor.
 */
export interface WorldRuleUpdateRequest {
  /**
   * Per-field replace (product lock: a mistyped name is not a dead-end). Must be non-empty after trimming (handler-enforced, AR-2).
   */
  canonical_name?: string;
  /**
   * Per-field replace. Must be non-empty after trimming (handler-enforced, AR-2); cannot be unset (no null-clearing, AR-3).
   */
  statement?: string;
  /**
   * Per-field replace. Must be non-empty when present (handler-enforced, AR-2); cannot be unset once set (AR-3 — read projection stays nullable).
   */
  severity_hint?: string;
  /**
   * Per-field replace. Write vocabulary enforced to draft | active | deprecated (AR-2). status=deprecated is the Deactivate recovery (product lock — no DELETE route).
   */
  status?: string;
  /**
   * Per-field replace. Must be non-empty when present (handler-enforced, AR-2).
   */
  kind?: string;
  /**
   * Per-field replace; [] is meaningful (explicit clear to all entry types in check scope, AR-3). NOTE (V1.169 implementer, AR-1/AR-3 resolution): the nullable union is the only typify-0.3 shape that keeps absent distinct from [] on the Rust DTO (optional arrays collapse to empty otherwise) — without it PATCH could never clear the axis (AR-3 product lock). Semantics stay present-or-absent: absent/null → unchanged, [] → clear, members must be non-empty, observer_cardinality combination rejected on the effective pair (AR-2/AR-5). No field can be null-cleared: null means unchanged, exactly like absent.
   */
  target_entry_types?: string[] | null;
  /**
   * Whole-carrier replacement (AR-3): the new carrier is validated first via the spoke-adapter seam (AR-2), then only extensions.nexus.constraint is overwritten — the rest of the extensions bag (other nexus keys + other namespaces) survives. Absent → unchanged.
   */
  constraint?: {
    [k: string]: unknown | undefined;
  };
}
