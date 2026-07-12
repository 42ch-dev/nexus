import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ComputeModuleDetail
 *
 * Full manifest.json shape for a compute module, as defined by compute-module-abi.md §7.
 *
 * @schema_version 1
 * @source module-detail.schema.json
 */

/** Inline enum type */
export type ModuleDetailHostFunctions = 'kb_read' | 'narrative_query';

/** Full manifest.json shape for a compute module, as defined by compute-module-abi.md §7. */
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
  host_functions?: ModuleDetailHostFunctions[];
  schemas?: { key_block_attributes?: Record<string, unknown>; key_block_state?: Record<string, unknown>; invocation?: Record<string, unknown>; battle_report?: Record<string, unknown> };
  battle_report_kind?: string;
  max_fuel?: number;
  max_memory_mib?: number;
  max_wall_time_ms?: number;
}
