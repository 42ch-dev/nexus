/**
 * Nexus ScheduleConcurrencyRequest
 *
 * Concurrency mode for schedule creation. Serial runs alone; ParallelWith groups schedules; ParallelAny allows any concurrency.
 *
 * @schema_version 1
 * @source schedule-concurrency-request.schema.json
 */

/** Concurrency mode for schedule creation. Serial runs alone; ParallelWith groups schedules; ParallelAny allows any concurrency. */
export type ScheduleConcurrencyRequest = 'serial' | 'parallel_with' | 'parallel_any';
