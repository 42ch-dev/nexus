/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

export interface UpdateCharacterRequest {
  expected_revision: number;
  display_name?: string;
  image_uri?: string | null;
  persona?: {
    [k: string]: unknown | undefined;
  } | null;
}
