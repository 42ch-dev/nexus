/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for PATCH /v1/daemon/works/{work_id}/findings/{finding_id}. Every field is optional; `rule_suggestion` is tri-state (R-V1190-FINDINGS-TRISTATE-DUP): omitted does not touch the stored column, `null` clears it to SQL NULL, a string sets it.
 */
export interface UpdateFindingRequest {
  severity?: string;
  status?: string;
  title?: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  /**
   * Tri-state prose-rule-suggestion patch (V1.48 P3 T3 / R-V147P0-03): omitted = do not touch the column, null = clear it to SQL NULL, string = set it. Deliberately unrestricted at the JSON-Schema level: a typed nullable string (`anyOf`/`oneOf` [null, string]) collapses omission and null in the generated carrier, because typify resolves `anyOf` [null, T] to `Option<T>` (typify-impl `maybe_option`, enums.rs:27-52) and a non-required nullable property already carries the option (structs.rs:135-160). The Rust/TS boundary parser owns the three-state grammar and rejects every other value before any stored effect.
   */
  rule_suggestion?: {
    [k: string]: unknown | undefined;
  };
}
