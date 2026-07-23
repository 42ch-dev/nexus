/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/agent-host/scan. Triggers a combined registry-list + PATH-probe operation that returns ACP agent entries annotated with local-install availability. Additive V1.94 endpoint — no breaking change to existing agent-host routes.
 */
export interface ScanRequest {
  /**
   * Filter results by install status. 'installed' returns only PATH-available agents; 'all' returns the full registry list annotated with install status.
   */
  filter?: "installed" | "all";
  /**
   * If true, forces a fresh fetch of the ACP registry from CDN before scanning. If false (default), uses the cached registry data (stale-while-revalidate).
   */
  registry_refresh?: boolean;
}
