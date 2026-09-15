/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Rust-admitted provider launch recipe; never accepted from HTTP or JS callers.
 */
export interface ValidatedProviderRecipe {
  provider_id: string;
  recipe_generation: string;
  executable: string;
  args: string[];
  env: {
    [k: string]: string | undefined;
  };
  cwd: string;
  permissions_ref?: string | null;
  config_ref?: string | null;
  process_identity?: {
    pid: number;
    process_birth?: string | null;
    group_id?: string | null;
  } | null;
}
