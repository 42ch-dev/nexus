/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Canonical Connect invoke request served by the core: operation name plus extensible JSON args. Non-served or unknown operations are refused with an unsupported error and zero side effects.
 */
export interface CoreConnectInvokeRequest {
  /**
   * Connect operation name.
   */
  op: string;
  /**
   * Operation arguments; extensible JSON.
   */
  args: {
    [k: string]: unknown | undefined;
  };
}
