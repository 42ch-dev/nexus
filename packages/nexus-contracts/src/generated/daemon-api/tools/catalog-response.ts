/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/tools — the dispatchable spine catalog (AR-68 #7).
 */
export interface CatalogResponse {
  items: NexusCatalogTool[];
}
/**
 * One dispatchable tool row in the spine catalog (AR-68 #7): static nexus.* rows, admitted user capabilities, and PeerToolTable entries.
 */
export interface NexusCatalogTool {
  id: string;
  description: string;
  input_schema: string;
  output_schema?: string;
  /**
   * Provenance of the tool: "builtin" (static nexus.* row), "user" (locally-installed capability), "peer" (admitted dialer tool).
   */
  origin: "builtin" | "user" | "peer";
}
