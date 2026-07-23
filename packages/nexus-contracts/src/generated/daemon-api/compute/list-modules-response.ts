/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/compute/modules.
 */
export interface ListModulesResponse {
  items: NexusComputeModuleSummary[];
  has_more: boolean;
}
/**
 * Summary of an installed compute module surfaced by the registry list endpoint.
 */
export interface NexusComputeModuleSummary {
  module_id: string;
  name: string;
  version: string;
  description?: string;
  required_key_block_types: string[];
  battle_report_kind?: string;
  status: "ok" | "broken";
}
