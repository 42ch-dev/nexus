/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/orchestration/capabilities (cursor-based pagination, F-P3). The array field is `items`; the legacy `capabilities` key was removed in `@42ch/nexus-contracts` 0.6.0.
 */
export interface ListCapabilitiesResponse {
  items: NexusCapabilityInfo[];
  pagination: NexusPaginationInfo;
}
/**
 * Description of a single registered capability (name + I/O schemas).
 */
export interface NexusCapabilityInfo {
  name: string;
  input_schema: string;
  output_schema: string;
  /**
   * Provenance of the capability (AR-40/AR-68): "builtin" ships with the engine, "user" is a locally-installed developer capability, "peer" is an admitted dialer tool.
   */
  origin?: "builtin" | "user" | "peer";
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
