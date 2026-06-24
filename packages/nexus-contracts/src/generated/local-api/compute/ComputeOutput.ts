import type { KeyBlock } from '../../domain/KeyBlock';
import type { TimelineEvent } from '../../domain/TimelineEvent';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus Compute Output Envelope
 *
 * Standard 4-part output envelope returned by a WASM compute module (V1.61 ABI, compass Q8). Modules emit state deltas to apply, timeline events to append (aligned with V1.60 timeline.event.append), new KeyBlocks to create, and a module-declared freeform report. The host applies these in order: state_delta -> new_key_blocks -> timeline_events, then surfaces battle_report.
 *
 * @schema_version 1
 * @source compute-output.schema.json
 */

/** Inline enum type */
export type ComputeOutputStateDeltaOp = 'add' | 'sub' | 'set';

/** Standard 4-part output envelope returned by a WASM compute module (V1.61 ABI, compass Q8). Modules emit state deltas to apply, timeline events to append (aligned with V1.60 timeline.event.append), new KeyBlocks to create, and a module-declared freeform report. The host applies these in order: state_delta -> new_key_blocks -> timeline_events, then surfaces battle_report. */
export interface ComputeOutput {
  schema_version: number;
  state_delta: { op: ComputeOutputStateDeltaOp; path: string; target_key_block_id?: string; value?: unknown }[];
  timeline_events: TimelineEvent[];
  new_key_blocks: KeyBlock[];
  battle_report: { kind?: string };
}
