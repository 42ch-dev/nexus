/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for POST /v1/daemon/runtime/stop. The request is accepted only when expected_instance_id and expected_engine_epoch match the serving instance; a mismatch returns conflict and performs no stop. `stopped` reports a confirmed close; `stopping` reports that the owning service acknowledged the request and is draining.
 */
export interface RuntimeApi {
  /**
   * `stopped`: cleanup is confirmed and the discovery record was removed by the owning service. `stopping`: the stop was accepted and the service is draining; poll runtime health for liveness.
   */
  status: "stopped" | "stopping";
}
