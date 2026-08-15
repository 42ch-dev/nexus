/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Read surface for a world's structured rules (V1.166 DR-64 / AR-3): GET /v1/daemon/worlds/{world_id}/rules. Items project the spoke Rule author metadata verbatim (canonical_name, kind, statement, severity_hint, status, target_entry_types — open spoke vocabulary, no nexus coercion at rest) plus the AR-2 constraint carrier surfaced first-class from extensions.nexus.constraint (absent/malformed → omitted; the extensions bag itself is NOT exposed). The projection converts the stored INTEGER Unix-epoch timestamps to RFC 3339. Store order is canonical_name ASC, rule_id ASC (author-metadata list, not newest-first); `truncated` is the honest flag for the 500-rule safety cap: true only when more rows exist than the cap. Owned world with zero rules → 200 + {"rules": [], "truncated": false}.
 */
export interface WorldRulesListResponse {
  rules: {
    /**
     * Stable rule id (rul_<uuid v4 simple>, AR-2).
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
     * Longer explanation for integrators or authors.
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
     * Targeting axis for the three entry families; empty = all entry types in check scope (PD-1). Inapplicable to observer_cardinality (events carry no entry_type — the CLI rejects the combination, AR-2).
     */
    target_entry_types: string[];
    /**
     * The AR-2 constraint carrier (extensions.nexus.constraint) projected first-class: raw typed JSON object discriminated by family. Absent/malformed stored carrier → omitted. The extensions bag itself is not exposed — the carrier is the only product payload.
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
  }[];
  /**
   * True when the 500-rule safety cap was exceeded and rules is the first 500 of canonical_name ASC, rule_id ASC order (pagination lands with the Control Room panel — roadmap).
   */
  truncated: boolean;
}
