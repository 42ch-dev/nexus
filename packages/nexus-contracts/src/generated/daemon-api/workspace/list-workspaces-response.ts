/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/workspaces.
 */
export interface ListWorkspacesResponse {
  items: NexusWorkspaceSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * Summary row for a workspace in list responses.
 */
export interface NexusWorkspaceSummary {
  creator_id: string;
  workspace_slug: string;
  creative_root: string;
  display_name?: string;
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
