/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Bounded run-events page with explicit terminal and resync inspection; consumers treat resync_required as a gap and explicitly resynchronize.
 */
export interface CoreRunEventsResponse {
  run_id: string;
  events: {
    sequence: number;
    /**
     * Run event kind discriminant.
     */
    kind: string;
    /**
     * Event payload; extensible JSON, absent when the kind carries none.
     */
    payload?: {
      [k: string]: unknown | undefined;
    };
  }[];
  /**
   * Decimal string sequence watermark after this page.
   */
  next_sequence: string;
  /**
   * True when the run has reached a terminal state and no further events can be produced.
   */
  terminal: boolean;
  /**
   * True when the requested tail has been retention-trimmed; the caller must resynchronize.
   */
  resync_required: boolean;
}
