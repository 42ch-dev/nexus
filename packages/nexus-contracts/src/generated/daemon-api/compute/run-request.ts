/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/compute/run — invoke a compute module against an owned World (V1.147 P0 direct lane).
 */
export interface RunRequest {
  /**
   * The World to run the module against. Must be owned by the active creator (ownership gate enforced server-side).
   */
  world_id: string;
  /**
   * Installed compute module ID (from ModuleCache).
   */
  module_id: string;
  /**
   * Optional timeline branch to scope the run to. Defaults to the World root branch when omitted.
   */
  branch_id?: string;
  /**
   * Module-specific invocation parameters (manifest-driven: attacker_id, defender_id, etc.). Keys ending in _id that correspond to required_key_block_types are resolved into KnowledgeEntry references during ComputeInput assembly.
   */
  invocation_params?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Optional caller-supplied durable operation id (v1.207 §B.1, shape op_<32 lowercase hex>). When present it stabilizes retry identity: a retry after a lost response is answered from the stored receipt instead of re-running the module, and the same id with a different request is refused with operation_id_conflict. When omitted the run keeps its run-scoped derived id (unchanged behavior).
   */
  operation_id?: string;
}
