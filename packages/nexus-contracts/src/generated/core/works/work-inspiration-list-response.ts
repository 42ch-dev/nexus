/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Offset-paginated inspiration page for `GET /v1/daemon/works/pool/inspiration`.
 */
export interface WorkInspirationListResponse {
  items: WorkInspirationItem[];
  total: number;
  limit: number;
  offset: number;
}
/**
 * One inspiration-pool item as served on the wire (stored `creator_id` intentionally not serialized).
 */
export interface WorkInspirationItem {
  item_id: string;
  rel_path: string;
  title: string;
  status: string;
  promoted_work_id?: string;
  created_at: string;
  promoted_at?: string;
}
