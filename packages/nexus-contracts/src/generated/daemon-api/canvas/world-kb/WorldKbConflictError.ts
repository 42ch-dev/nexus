import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbConflictError
 *
 * Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected because expected_version is stale (HTTP 409). Per-row OCC on kb_key_blocks.revision / kb_extract_jobs.version (V1.73).
 *
 * @schema_version 1
 * @source world-kb-conflict-error.schema.json
 */
/** Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected because expected_version is stale (HTTP 409). Per-row OCC on kb_key_blocks.revision / kb_extract_jobs.version (V1.73). */
export interface WorldKbConflictError {
  current_version: number;
  entity_id: string;
  conflicting_path: string;
  recovery_hint: string;
}
