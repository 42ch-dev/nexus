/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Description of a single registered capability (name + I/O schemas).
 */
export interface CapabilityInfo {
  name: string;
  input_schema: string;
  output_schema: string;
  /**
   * Provenance of the capability (AR-40): "builtin" ships with the engine, "user" is a locally-installed developer capability.
   */
  origin?: "builtin" | "user";
}
