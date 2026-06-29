/**
 * Nexus WorldKbRelationshipKind
 *
 * Core taxonomy values for World KB typed relationships (V1.74). Use `custom` with a non-empty `custom_label` for out-of-enum narrative relationships.
 *
 * @schema_version 1
 * @source world-kb-relationship-kind.schema.json
 */

/** Core taxonomy values for World KB typed relationships (V1.74). Use `custom` with a non-empty `custom_label` for out-of-enum narrative relationships. */
export type WorldKbRelationshipKind = 'allied_with' | 'opposes' | 'parent_of' | 'child_of' | 'member_of' | 'located_in' | 'rules_over' | 'references' | 'serves' | 'rival_of' | 'mentor_of' | 'custom';
