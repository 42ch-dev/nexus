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
 * Request body for POST /v1/daemon/actor-knowledge/view. actor_ref is admitted from stored owners; payload claims never establish scope. Character views require binding_id.
 */
export interface ViewRequest {
  actor_ref: NexusActorRef;
  /**
   * World ID (prefix: 'wld_')
   */
  world_id: string;
  /**
   * Required for Character actor_ref; must be the active binding of that Character in world_id.
   */
  binding_id?: string;
  limit?: number;
  /**
   * Opaque two-field keyset cursor from a previous page.
   */
  cursor?: string;
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
