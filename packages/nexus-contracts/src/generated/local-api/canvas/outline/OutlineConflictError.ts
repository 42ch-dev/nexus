import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlineConflictError
 *
 * Structured detail placed inside the canonical ErrorResponse.details field when an Outline or Timeline patch is rejected because base_revision is stale (HTTP 409).
 *
 * @schema_version 1
 * @source outline-conflict-error.schema.json
 */
/** Structured detail placed inside the canonical ErrorResponse.details field when an Outline or Timeline patch is rejected because base_revision is stale (HTTP 409). */
export interface OutlineConflictError {
  current_revision: number;
  node_id: string;
  conflicting_path: string;
  recovery_hint: string;
}
