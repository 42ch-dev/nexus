/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/check (V1.148 P2). Mirrors the spoke CheckResponse wire contract 1:1: the success branch carries the checker Finding(s) produced by spoke orchestrate_check (persisted via FindingPort.put_findings — persisting findings is the check outcome, not an 'apply fix'); the error branch carries a structured error envelope. HTTP 200 may carry either branch (findings XOR error); spoke Reject results are surfaced as 4xx/5xx daemon error-envelope responses instead of the error branch (handler module docs — nearest compute/kb pattern).
 */
export type CheckResponse = NexusDaemonCheckResponseSuccess | NexusDaemonCheckResponseError;

/**
 * Success: checker output findings.
 */
export interface NexusDaemonCheckResponseSuccess {
  findings: {
    /**
     * Stable finding id.
     */
    finding_id: string;
    /**
     * Wire schema version (integer >= 1).
     */
    schema_version: number;
    /**
     * Short finding title.
     */
    title: string;
    /**
     * Finding detail text.
     */
    description: string;
    /**
     * Open string. Core vocabulary (documented, not enforced): info, warning, error.
     */
    severity: string;
    /**
     * Open string. Core vocabulary (documented, not enforced): open, resolved, dismissed.
     */
    status: string;
    /**
     * Optional checker kind or category.
     */
    kind?: string;
    /**
     * Optional KnowledgeEntry the finding targets.
     */
    target_entry_id?: string;
    /**
     * Spoke SourceAnchor wire shape (mirrored inline).
     */
    source_anchor?: {
      source_id: string;
      schema_version: number;
      label?: string;
      mime_type?: string;
      span?: {
        start: number;
        end: number;
      };
      extensions?: {
        [k: string]: unknown | undefined;
      };
    };
    /**
     * Optional suggested remediation text.
     */
    suggested_fix?: string;
    /**
     * Optional position hint within source text.
     */
    text_position?: {
      [k: string]: unknown | undefined;
    };
    /**
     * RFC 3339 UTC datetime string.
     */
    created_at?: string;
    /**
     * RFC 3339 UTC datetime string.
     */
    updated_at?: string;
    /**
     * Product namespace bag keyed by product-chosen ids matching ^[a-z][a-z0-9_-]*$. Values are opaque JSON objects.
     */
    extensions?: {
      [k: string]: unknown | undefined;
    };
  }[];
  /**
   * Product namespace bag keyed by product-chosen ids matching ^[a-z][a-z0-9_-]*$. Values are opaque JSON objects.
   */
  extensions?: {
    [k: string]: unknown | undefined;
  };
}
/**
 * Wire error envelope (spoke CheckResponse error branch; unused by the current handler — Reject maps to 4xx/5xx daemon error responses — kept for wire parity).
 */
export interface NexusDaemonCheckResponseError {
  /**
   * Spoke ErrorEnvelope wire shape (mirrored inline).
   */
  error: {
    /**
     * Machine-readable error code.
     */
    code: string;
    /**
     * Human-readable error message.
     */
    message: string;
    /**
     * Optional structured error context.
     */
    details?: {
      [k: string]: unknown | undefined;
    };
    /**
     * Product namespace bag keyed by product-chosen ids matching ^[a-z][a-z0-9_-]*$. Values are opaque JSON objects.
     */
    extensions?: {
      [k: string]: unknown | undefined;
    };
  };
  /**
   * Product namespace bag keyed by product-chosen ids matching ^[a-z][a-z0-9_-]*$. Values are opaque JSON objects.
   */
  extensions?: {
    [k: string]: unknown | undefined;
  };
}
