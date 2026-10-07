/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Core-lane findings PATCH payload (R-V1190-FINDINGS-TRISTATE-DUP). Single definition: this schema is an `allOf` alias to the daemon-api `update-finding-request` contract, which is the one tri-state authority (`rule_suggestion` modeled with the `x-nexus-tri-state` marker, never a nullable `anyOf`). The root `properties` block re-declares ONLY that marker: `rust-gen` reads the marker from the source schema's top-level `properties`, and the prep stage inlines an `allOf` member under the referenced `title`, so the alias would otherwise lose the marker and the generated core-lane carrier would flatten omission onto null again.
 */
export type FindingsApi = NexusUpdateFindingRequest & {
  rule_suggestion?: unknown;
  [k: string]: unknown | undefined;
};

/**
 * Request body for PATCH /v1/daemon/works/{work_id}/findings/{finding_id}. Every field is optional; `rule_suggestion` is tri-state (R-V1190-FINDINGS-TRISTATE-DUP): omitted does not touch the stored column, `null` clears it to SQL NULL, a string sets it.
 */
export interface NexusUpdateFindingRequest {
  severity?: string;
  status?: string;
  title?: string;
  description?: string;
  target_executor?: string;
  kind?: string;
  rule_suggestion?: unknown;
}
