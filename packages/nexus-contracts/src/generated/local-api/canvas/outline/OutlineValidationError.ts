import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus OutlineValidationError
 *
 * Structured detail placed inside the canonical ErrorResponse.details field when an Outline or Timeline patch fails domain validation (HTTP 422). Mirrors the validation_summary shape of OutlinePatchResponse.
 *
 * @schema_version 1
 * @source outline-validation-error.schema.json
 */
/** Structured detail placed inside the canonical ErrorResponse.details field when an Outline or Timeline patch fails domain validation (HTTP 422). Mirrors the validation_summary shape of OutlinePatchResponse. */
export interface OutlineValidationError {
  validation_summary: { errors: string[]; warnings: string[] };
}
