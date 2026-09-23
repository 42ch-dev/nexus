/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One bounded pull from a workflow event subscription: at most 16 frames and 1 MiB, already encoded, in ring order. Frames keep the existing run-event vocabulary (`run_state`, `host_event`, `gap`, `history_unavailable`) and the existing `<UUID epoch>:<decimal sequence>` cursor; the transport writes `id`/`event`/`data` verbatim and never resequences.
 */
export interface CoreWorkflowEventBatch {
  /**
   * Frames strictly after the subscription's current cursor, in ring order.
   *
   * @maxItems 16
   */
  events: {
    /**
     * SSE cursor `<UUID epoch>:<decimal sequence>`, or the empty string for a control frame that carries no cursor (a history_unavailable close).
     */
    id: string;
    /**
     * Event name discriminated from the frame payload.
     */
    event: string;
    /**
     * Frame payload, already serialized by the core; the transport must not re-encode it.
     */
    data: string;
  }[];
  /**
   * True when the stream ended AT this batch: the run reached its durable terminal frame, the ring is gone, or the subscription was released or the owner closed. No further pull is served for a closed subscription.
   */
  closed: boolean;
}
