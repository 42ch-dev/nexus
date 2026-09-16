/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Closed v1 instance-bound stop request for the runtime stop endpoint. A mismatch on instance id or engine epoch returns conflict and performs no stop; no PID-only stop exists.
 */
export interface CoreServiceStopRequest {
  /**
   * Instance identity from discovery; stop is refused when it no longer owns the service.
   */
  expected_instance_id: string;
  /**
   * Engine epoch expected by the caller; null matches an uninitialized service. Mismatch returns conflict.
   */
  expected_engine_epoch: number | null;
}
