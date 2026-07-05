import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus StrategyConflictError
 *
 * Structured detail placed inside the canonical ErrorResponse.details field when a Strategy patch is rejected because base_revision is stale (HTTP 409).
 *
 * @schema_version 1
 * @source strategy-conflict-error.schema.json
 */
/** Structured detail placed inside the canonical ErrorResponse.details field when a Strategy patch is rejected because base_revision is stale (HTTP 409). */
export interface StrategyConflictError {
  current_revision: number;
  node_id: string;
  conflicting_path: string;
  recovery_hint: string;
}
