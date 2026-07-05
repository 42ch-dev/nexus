import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ErrorResponse
 *
 * Canonical Daemon API error detail. The daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire; this schema models the stable, contract-locked `error` detail shared across all Daemon API failure paths (F-E1).
 *
 * @schema_version 1
 * @source error-response.schema.json
 */
/** Canonical Daemon API error detail. The daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire; this schema models the stable, contract-locked `error` detail shared across all Daemon API failure paths (F-E1). */
export interface ErrorResponse {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}
