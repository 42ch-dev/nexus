/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Viewpoint for an Actor-mode agent-host session. Contains World plus optional binding/branch/event. Never carries an Actor id.
 */
export interface SessionViewpoint {
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Required for Character actor_ref; must be omitted for Creator.
   */
  binding_id?: string;
  /**
   * Optional ForkBranch id participating in session isolation.
   */
  branch_id?: string;
  /**
   * Optional rewind/event anchor participating in session isolation.
   */
  event_id?: string;
}
