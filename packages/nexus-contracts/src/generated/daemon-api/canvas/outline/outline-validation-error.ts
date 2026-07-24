/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Structured detail placed inside the canonical ErrorResponse.details field when an Outline or Timeline patch fails domain validation (HTTP 422). Mirrors the validation_summary shape of OutlinePatchResponse.
 */
export interface OutlineValidationError {
  validation_summary: {
    /**
     * Fatal validation messages that prevented the patch.
     */
    errors: string[];
    /**
     * Non-fatal validation messages.
     */
    warnings: string[];
  };
}
