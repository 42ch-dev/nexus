/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Resolve a stuck local outbox entry: retry it now or discard it. Only entries in a stuck state (conflicted/failed) are resolvable; resolution is local and never re-sends by itself.
 */
export interface CoreOutboxResolveRequest {
  outbox_entry_id: string;
  /**
   * retry re-queues the entry for delivery; discard drops it locally.
   */
  action: "retry" | "discard";
}
