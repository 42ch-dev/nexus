/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single rule item returned by POST /v1/daemon/worlds/{world_id}/rules (201) and PATCH /v1/daemon/worlds/{world_id}/rules/{rule_id} (200) (V1.169 P1 / AR-1). The WorldRulesListResponseRulesItem shape verbatim: spoke Rule author metadata projected verbatim (canonical_name, kind, statement, severity_hint, status, target_entry_types — open spoke vocabulary, no nexus coercion at rest) plus the AR-2 constraint carrier surfaced first-class from extensions.nexus.constraint (absent/malformed → omitted; the extensions bag itself is NOT exposed). The projection converts the stored INTEGER Unix-epoch timestamps to RFC 3339.
 */
export interface WorldRuleResponse {
  /**
   * Stable rule id (rul_<uuid v4 simple>, AR-2) — minted server-side, immutable, path-addressed, never a DTO field.
   */
  rule_id: string;
  /**
   * Human-stable name (author-metadata list order key).
   */
  canonical_name: string;
  /**
   * Open string. Core vocabulary (documented, not enforced): rule, prohibition, style. Stored verbatim — author classification, not Finding kind (PD-1).
   */
  kind: string;
  /**
   * Human summary only — never parsed by the evaluator (PD-1).
   */
  statement?: string | null;
  /**
   * Longer explanation for integrators or authors. Not mutable on the write surface (product field set); NULL at API create.
   */
  description?: string | null;
  /**
   * Open string. Core vocabulary (documented, not enforced): info, warning, error. Verbatim; the evaluator defaults to warning when absent (AR-4).
   */
  severity_hint?: string | null;
  /**
   * Open string. Core vocabulary (documented, not enforced): draft, active, deprecated. Verbatim; auto-include only evaluates status=active (PD-1).
   */
  status?: string | null;
  /**
   * Targeting axis for the three entry families; empty = all entry types in check scope (PD-1). Inapplicable to observer_cardinality (events carry no entry_type — the combination is rejected on write, AR-2).
   */
  target_entry_types: string[];
  /**
   * The AR-2 constraint carrier (extensions.nexus.constraint) projected first-class: raw typed JSON object discriminated by family. Absent/malformed stored carrier → omitted. The extensions bag itself is not exposed.
   */
  constraint?: {
    [k: string]: unknown | undefined;
  };
  /**
   * RFC 3339 UTC datetime string (projected from stored epoch seconds).
   */
  created_at?: string;
  /**
   * RFC 3339 UTC datetime string (projected from stored epoch seconds).
   */
  updated_at?: string;
}
