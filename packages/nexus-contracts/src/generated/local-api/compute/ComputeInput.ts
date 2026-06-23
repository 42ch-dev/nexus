import type { KeyBlock } from '../../domain/KeyBlock';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus Compute Input Envelope
 *
 * Standard input envelope passed into a WASM compute module (V1.61 ABI, compass Q3/Q8). Bundles a read-only KeyBlock snapshot, the narrative position, and module-declared invocation parameters. Modules are stateless pure functions (compass Q6): every call receives a fresh envelope and returns a ComputeOutput.
 *
 * @schema_version 1
 * @source compute-input.schema.json
 */
/** Standard input envelope passed into a WASM compute module (V1.61 ABI, compass Q3/Q8). Bundles a read-only KeyBlock snapshot, the narrative position, and module-declared invocation parameters. Modules are stateless pure functions (compass Q6): every call receives a fresh envelope and returns a ComputeOutput. */
export interface ComputeInput {
  schema_version: number;
  world_ref: { world_id?: string; branch_id?: string; timeline_head_event_id?: string };
  key_blocks: KeyBlock[];
  narrative_state?: { timeline_position?: string; current_chapter?: string; current_scene?: string };
  invocation?: Record<string, unknown>;
}
