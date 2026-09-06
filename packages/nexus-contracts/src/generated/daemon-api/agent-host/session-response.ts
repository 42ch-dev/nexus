/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 Actor identity sum: Creator | Character. No unknown kinds; a payload cannot carry both bearer ids.
 */
export type NexusActorRef = CreatorActorRef | CharacterActorRef;

/**
 * Agent-host session summary. Optional actor_ref and viewpoint are omitted for legacy sessions.
 */
export interface SessionResponse {
  session_id: string;
  provider_id: string;
  state: string;
  active_op_id?: string;
  model?: string;
  actor_ref?: NexusActorRef;
  viewpoint?: NexusSessionViewpoint;
}
export interface CreatorActorRef {
  /**
   * Actor kind discriminant for a Creator bearer.
   */
  actor_kind: "creator";
  /**
   * Creator bearer id (`CreatorId`).
   */
  creator_id: string;
}
export interface CharacterActorRef {
  /**
   * Actor kind discriminant for a Character bearer. Unrelated to KnowledgeEntry block_type=character.
   */
  actor_kind: "character";
  /**
   * Character ID: lowercase chr_ prefix and exactly 32 hex characters.
   */
  character_id: string;
}
/**
 * Viewpoint for an Actor-mode agent-host session. Contains World plus optional binding/branch/event. Never carries an Actor id.
 */
export interface NexusSessionViewpoint {
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
