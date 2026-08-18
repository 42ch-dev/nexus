/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/worlds/{world_id}/rules (V1.169 P1 / AR-1). Type structure only — deliberately no minLength/enum/format value constraints: form-producible failures (empty name/statement after trim, non-core status, carrier member errors) must surface through the field-level error envelope (AR-2), never the axum Json extractor. The constraint carrier's internal shape is validated by the spoke-adapter seam (nexus-spoke-adapter::constraint::parse_carrier_json_member — closed four-family grammar); the schema only requires it to be a JSON object.
 */
export interface WorldRuleCreateRequest {
  /**
   * Human-stable name (author-metadata list order key). Required; must be non-empty after trimming (handler-enforced, AR-2).
   */
  canonical_name: string;
  /**
   * Human summary only — never parsed by the evaluator (PD-1). Required; must be non-empty after trimming (handler-enforced, AR-2).
   */
  statement: string;
  /**
   * The AR-2 constraint carrier: a typed JSON object discriminated by family (module_presence | module_absence | required_field | observer_cardinality). Internal shape is NOT schema-enforced — the handler validates it member-aware via the spoke-adapter seam so carrier errors surface through the envelope as constraint.* (AR-2).
   */
  constraint: {
    [k: string]: unknown | undefined;
  };
  /**
   * Open string; default 'rule' when omitted (CLI parity, AR-3). Must be non-empty when present (handler-enforced, AR-2). Core vocabulary (documented, not enforced): rule, prohibition, style.
   */
  kind?: string;
  /**
   * Open string; stored NULL when omitted (evaluation defaults to warning — V1.166 PD-1). Must be non-empty when present (handler-enforced, AR-2). Core vocabulary (documented, not enforced): info, warning, error.
   */
  severity_hint?: string;
  /**
   * Write vocabulary enforced to the core set draft | active | deprecated (AR-2); default 'active' when omitted (product lock: first rule auto-includes, AR-3).
   */
  status?: string;
  /**
   * Targeting axis for the three entry families; default [] when omitted (= all entry types in check scope, AR-3). Empty members and the observer_cardinality combination are rejected handler-side (AR-2).
   */
  target_entry_types?: string[];
}
