import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ComputeModuleSummary
 *
 * Summary of an installed compute module surfaced by the registry list endpoint.
 *
 * @schema_version 1
 * @source module-summary.schema.json
 */

/** Inline enum type */
export type ModuleSummaryStatus = 'ok' | 'broken';

/** Summary of an installed compute module surfaced by the registry list endpoint. */
export interface ModuleSummary {
  module_id: string;
  name: string;
  version: string;
  description?: string;
  required_key_block_types: string[];
  battle_report_kind?: string;
  status: ModuleSummaryStatus;
}
