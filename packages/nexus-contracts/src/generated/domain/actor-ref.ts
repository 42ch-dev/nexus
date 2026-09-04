/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 Actor identity sum: Creator | Character. No unknown kinds; a payload cannot carry both bearer ids.
 */
export type ActorRef = CreatorActorRef | CharacterActorRef;

export interface CreatorActorRef {
  /**
   * Actor kind discriminant for a Creator bearer.
   */
  actor_kind: string;
  /**
   * Creator ID: lowercase ctr_ prefix and exactly 32 hex characters.
   */
  creator_id: string;
}
export interface CharacterActorRef {
  /**
   * Actor kind discriminant for a Character bearer. Unrelated to KnowledgeEntry block_type=character.
   */
  actor_kind: string;
  /**
   * Character ID: lowercase chr_ prefix and exactly 32 hex characters.
   */
  character_id: string;
}
