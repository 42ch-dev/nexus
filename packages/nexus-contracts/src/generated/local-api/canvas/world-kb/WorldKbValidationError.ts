import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus WorldKbValidationError
 *
 * Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected for domain-rule violations (HTTP 422, V1.73). Distinct from 409 WorldKbConflictError which is concurrent-write version mismatch only.
 *
 * @schema_version 1
 * @source world-kb-validation-error.schema.json
 */
/** Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected for domain-rule violations (HTTP 422, V1.73). Distinct from 409 WorldKbConflictError which is concurrent-write version mismatch only. */
export interface WorldKbValidationError {
  validation_summary: { errors: string[]; warnings: string[] };
}
