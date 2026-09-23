/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Open one authorized subscription to a durable workflow run's bounded event stream. The subscription streams the same retained/live frames the run's ring already holds; it is not a second event dialect.
 */
export interface CoreWorkflowSubscribeRequest {
  /**
   * Root durable workflow run id (the session id the public inspect returns). A child session id is never accepted: ancestry is resolved from stored state, never from the caller's string.
   */
  run_id: string;
  /**
   * Exclusive resume cursor `<UUID epoch>:<decimal sequence>` from a previously delivered frame, passed verbatim.
   */
  last_event_id?: string;
}
