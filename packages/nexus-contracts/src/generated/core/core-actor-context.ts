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
 * Serializable owned projection of an admitted Actor: validated Creator ownership, active World/binding/branch/event anchors and the stored Character lifecycle epoch at admission. Grants no authority.
 */
export interface CoreActorContext {
  actor_ref: NexusActorRef;
  /**
   * Trusted owner admitted at request time; never taken from a later request body.
   */
  owner_creator_id: string;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Active ActorWorldBinding id; null for Creator actors.
   */
  binding_id: string | null;
  /**
   * Fork branch anchor (fbk_*); null when the session has no branch isolation.
   */
  branch_id: string | null;
  /**
   * Timeline event rewind anchor; null when absent.
   */
  event_id: string | null;
  /**
   * Stored Character lifecycle_epoch at admission; null for Creator actors. Mismatch after admission is actor_session_stale.
   */
  character_epoch: number | null;
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
