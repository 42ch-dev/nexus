/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { WorkPoolEntry } from './work-pool-entry';

/**
 * Offset-paginated authoring-pool page for `GET /v1/daemon/works/pool` (legacy `list_pool` envelope).
 */
export interface WorkPoolListResponse {
  entries: WorkPoolEntry[];
  total: number;
  limit: number;
  offset: number;
}
