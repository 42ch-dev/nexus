/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/inspector/moment — assemble and return the enriched inspector packet for one moment (V1.151 P0 DF-76).
 */
export interface MomentInspectRequest {
  /**
   * World id to assemble the moment over.
   */
  world_id: string;
  /**
   * Optional work id for a work-bound moment (scope resolution of the directive region).
   */
  work_id?: string;
  /**
   * Generation stage gate. Maps via GenerationStage::as_str/parse; unknown values default to unspecified.
   */
  generation_stage?:
    | "intake"
    | "research"
    | "produce"
    | "review"
    | "persist"
    | "work_maintenance"
    | "system_maintenance"
    | "unspecified";
}
