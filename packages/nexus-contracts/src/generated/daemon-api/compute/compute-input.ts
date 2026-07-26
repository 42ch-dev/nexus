/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Standard input envelope passed into a WASM compute module (V1.61 ABI, compass Q3/Q8). Bundles a read-only KnowledgeEntry snapshot, the narrative position, and module-declared invocation parameters. Modules are stateless pure functions (compass Q6): every call receives a fresh envelope and returns a ComputeOutput.
 */
export interface ComputeInput {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * World and timeline locator for this invocation
   */
  world_ref: {
    /**
     * World the compute invocation runs against
     */
    world_id?: string;
    /**
     * Fork branch ID (root branch or a specific fork)
     */
    branch_id?: string;
    /**
     * Current timeline head the compute advances from
     */
    timeline_head_event_id?: string;
    [k: string]: unknown | undefined;
  };
  /**
   * Snapshot of KnowledgeEntry records relevant to this invocation. Each entry carries its body including computable state and attributes (immutable compute params). The host selects which entries to pass based on the module manifest and the capability context.
   */
  key_blocks: {
    [k: string]: unknown | undefined;
  }[];
  /**
   * Narrative position context (timeline, chapter, scene). Shape is module-declared; fields not listed here may be supplied by the host per the module manifest.
   */
  narrative_state?: {
    /**
     * Opaque timeline position label (module-interpreted)
     */
    timeline_position?: string;
    /**
     * Current chapter identifier, if applicable
     */
    current_chapter?: string;
    /**
     * Current scene identifier, if applicable
     */
    current_scene?: string;
    [k: string]: unknown | undefined;
  };
  /**
   * Module-defined input parameters for this invocation (freeform object). The exact fields are declared by the module's manifest.json; the host passes them through verbatim. This is the V1 envelope escape hatch for module-specific inputs (e.g. chosen targets, difficulty, dice seed).
   */
  invocation?: {
    [k: string]: unknown | undefined;
  };
}
