/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Full manifest.json shape for a compute module, as defined by compute-module-abi.md §7.
 */
export interface ModuleDetail {
  module_id: string;
  name: string;
  version: string;
  nexus_abi_version: number;
  required_key_block_types: string[];
  compute_export: string;
  init_export: string;
  description?: string;
  author?: string;
  host_functions?: ("kb_read" | "narrative_query")[];
  schemas?: {
    key_block_attributes?: {
      [k: string]:
        | {
            [k: string]: unknown | undefined;
          }
        | undefined;
    };
    key_block_state?: {
      [k: string]:
        | {
            [k: string]: unknown | undefined;
          }
        | undefined;
    };
    invocation?: {
      [k: string]: unknown | undefined;
    };
    battle_report?: {
      [k: string]: unknown | undefined;
    };
  };
  battle_report_kind?: string;
  max_fuel?: number;
  max_memory_mib?: number;
  max_wall_time_ms?: number;
}
