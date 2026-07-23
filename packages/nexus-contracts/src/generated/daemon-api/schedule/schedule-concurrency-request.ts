/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Concurrency mode for schedule creation. Serial runs alone; ParallelWith groups schedules; ParallelAny allows any concurrency.
 */
export type ScheduleConcurrencyRequest = "serial" | "parallel_with" | "parallel_any";
