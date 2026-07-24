/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/worlds (201 Created).
 */
export interface CreateWorldResponse {
  /**
   * The newly created World identifier.
   */
  world_id: string;
  /**
   * World lifecycle status (default: active).
   */
  status: "active" | "archived";
}
