/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed canonical KnowledgeEntry owner union: World | Character | ActorWorldBinding. Wire shape matches the domain KnowledgeOwnerRef (kind + id).
 */
export type KnowledgeOwnerRef = WorldKnowledgeOwner | CharacterKnowledgeOwner | BindingKnowledgeOwner;

export interface WorldKnowledgeOwner {
  /**
   * World-owned KnowledgeEntry.
   */
  kind: "world";
  /**
   * World ID (prefix: 'wld_')
   */
  id: string;
}
export interface CharacterKnowledgeOwner {
  /**
   * Character-owned KnowledgeEntry shared across active bindings.
   */
  kind: "character";
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  id: string;
}
export interface BindingKnowledgeOwner {
  /**
   * Binding-local KnowledgeEntry isolated to one ActorWorldBinding.
   */
  kind: "actor_world_binding";
  /**
   * ActorWorldBinding ID (lowercase prefix awb_ and exactly 32 hex characters)
   */
  id: string;
}
