/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Read surface for world-attached check findings (V1.165 P1 T3 / DR-68, AR-3): GET /v1/daemon/worlds/{world_id}/findings. Items mirror the spoke Finding wire shape as inlined by check-response.schema.json (the check route returns the same shape verbatim); the projection converts the stored INTEGER Unix-epoch timestamps to RFC 3339 (spoke Timestamp = RFC 3339 date-time) and rehydrates the stored JSON columns (source_anchor / text_position / extensions). Severity/status are spoke vocabulary verbatim (info|warning|error / open|resolved|dismissed — AR-1: no nexus mapping on the world path). `truncated` is the honest flag for the 500-newest safety cap: true only when more rows exist than the cap. Owned world with zero findings → 200 + {"findings": [], "truncated": false} (PD-3).
 */
export interface WorldFindingsListResponse {
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
   * True when the 500-newest safety cap was exceeded and findings is the newest 500 (pagination lands with the Control Room panel — roadmap).
   */
  truncated: boolean;
}
