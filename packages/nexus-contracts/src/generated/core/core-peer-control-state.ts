/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Current peer control state: whether the reverse-invoke operator cohort is enabled and which peers are active.
 */
export interface CorePeerControlState {
  enabled: boolean;
  /**
   * Authenticated peers with an active control session.
   */
  active_peers: string[];
}
