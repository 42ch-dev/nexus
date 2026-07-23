/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Single entry in core context version history.
 */
export interface CoreContextHistoryEntry {
  /**
   * Core context version number.
   */
  version: number;
  /**
   * Payload type (text or struct).
   */
  payload_kind: string;
  /**
   * Core context content at this version.
   */
  content?: {
    [k: string]: unknown | undefined;
  };
  /**
   * How this version was derived.
   */
  derivation_kind: string;
  /**
   * ISO-8601 creation timestamp.
   */
  created_at: string;
}
