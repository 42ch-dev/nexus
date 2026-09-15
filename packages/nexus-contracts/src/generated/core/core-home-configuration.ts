/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Home/configuration projection for the narrow home entry: resolved home roots and the currently selected creator/workspace. Carries no workspace pool, execution or Host state.
 */
export interface CoreHomeConfiguration {
  /**
   * Raw user home this service was opened with.
   */
  user_home: string;
  /**
   * Resolved .nexus42 home root (home-layout appends it exactly once).
   */
  nexus_home: string;
  /**
   * Currently selected creator id; null before selection.
   */
  active_creator_id: string | null;
  /**
   * Currently selected workspace slug; null before selection.
   */
  active_workspace_slug: string | null;
}
