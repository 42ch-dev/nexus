/**
 * Nexus Sync Conflict Response
 *
 * Platform conflict response for bundle push operations. HTTP 200 with success:false indicates a conflict requiring resolution. See hard-vs-soft-validation-v1.md §7.
 *
 * @schema_version 1
 * @source conflict-response.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

/** Inline enum type */
export type ConflictType = 'version_mismatch' | 'sequence_conflict' | 'hard_validation_failure' | 'soft_validation_warning';

/** Inline enum type */
export type ResolutionHint = 'auto_accept' | 'auto_reject' | 'manual_review';

export interface ConflictResponse {
  success: boolean;
  conflict_type: ConflictType;
  conflicts: { code: string; message: string; delta_index?: number; expected?: unknown; actual?: unknown; resolution_hint?: ResolutionHint }[];
  server_world_revision: number;
  server_delta_sequence?: number;
  retry_after?: number | null;
}
