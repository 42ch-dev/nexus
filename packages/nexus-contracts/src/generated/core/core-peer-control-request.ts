/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Authenticated peer control request: the session peer identity, the operation to drive, and its extensible JSON payload. Caller identity is the authenticated peer, never a payload claim.
 */
export interface CorePeerControlRequest {
  /**
   * Authenticated session peer id.
   */
  peer_id: string;
  operation: string;
  /**
   * Operation payload; extensible JSON.
   */
  payload: {
    [k: string]: unknown | undefined;
  };
}
