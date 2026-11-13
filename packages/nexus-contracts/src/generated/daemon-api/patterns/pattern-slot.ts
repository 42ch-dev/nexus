/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One named slot a pattern package declares for compose-time binding. The declaration is data: binding a slot, and checking that every required slot is bound, are runtime compose steps.
 */
export interface PatternSlot {
  /**
   * Slot name. Unique within one package manifest; non-empty.
   */
  name: string;
  /**
   * What a binding supplies for this slot: a string value, an asset id, or a capability id.
   */
  kind: "string" | "asset" | "capability";
  /**
   * Whether compose must bind this slot before the package can be used.
   */
  required: boolean;
}
