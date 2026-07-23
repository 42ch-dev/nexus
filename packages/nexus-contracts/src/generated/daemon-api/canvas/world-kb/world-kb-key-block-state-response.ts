/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Read projection for GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state (V1.114 P2). Surfaces the mutable runtime state of a computable KeyBlock plus its computability flag and per-row OCC version.
 */
export interface WorldKbKeyBlockStateResponse {
  /**
   * The block's body.state when computable; null when non-computable or when computable but no state is present.
   */
  state: {
    [k: string]: unknown | undefined;
  } | null;
  /**
   * True when the KeyBlock's body.computable is true.
   */
  is_computable: boolean;
  /**
   * Per-row OCC revision (kb_key_blocks.revision, NULL normalized to 0).
   */
  version: number;
}
