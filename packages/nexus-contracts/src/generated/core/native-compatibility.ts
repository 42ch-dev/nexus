/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Fixed compatibility manifest returned by nativeCompatibility() before any DB open.
 */
export interface NativeCompatibility {
  native_api_version: 1;
  writer_protocol: 1;
  target_triple: string;
  package_version: string;
  contract_tree_sha256: string;
  db_schema_min: number;
  db_schema_max: number;
  napi_minimum: 8;
}
