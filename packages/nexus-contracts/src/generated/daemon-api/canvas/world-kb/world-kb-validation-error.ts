/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Structured detail placed inside the canonical ErrorResponse.details field when a World KB patch is rejected for domain-rule violations (HTTP 422, V1.73). Distinct from 409 WorldKbConflictError which is concurrent-write version mismatch only.
 */
export interface WorldKbValidationError {
  validation_summary: {
    /**
     * Fatal validation messages that prevented the patch. Always present; empty on success.
     */
    errors: string[];
    /**
     * Non-fatal validation messages. Always present; empty when none.
     */
    warnings: string[];
  };
}
