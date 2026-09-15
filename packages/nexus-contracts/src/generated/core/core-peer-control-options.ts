/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Peer control start options: whether reverse-invoke peer control is enabled and the bounded operation allowlist peers may drive. Empty allowlist is fail-closed.
 */
export interface CorePeerControlOptions {
  enabled: boolean;
  /**
   * Bounded allowlist of operations a peer may invoke; empty means none.
   */
  allowed_operations?: string[];
}
