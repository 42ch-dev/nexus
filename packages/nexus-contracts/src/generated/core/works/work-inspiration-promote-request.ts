/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Body for `POST /v1/daemon/works/pool/inspiration/promote` (atomic create-Work-from-inspiration).
 */
export interface WorkInspirationPromoteRequest {
  item_id: string;
  idea?: string;
  set_default?: boolean;
}
