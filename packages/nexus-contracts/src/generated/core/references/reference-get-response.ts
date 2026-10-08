/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { ReferenceSourceInfo } from './reference-source-info';

/**
 * Response for `GET /v1/daemon/references/{reference_id}`; 404 for an unknown id. Carries the registry fields `reference show` renders alongside the lean `reference` projection shared with the list response.
 */
export interface ReferenceGetResponse {
  reference: ReferenceSourceInfo;
  workspace_id: string;
  /**
   * Last registry update timestamp (nullable).
   */
  updated_at?: string | null;
  /**
   * Serialized tag list as stored by the registry (nullable).
   */
  tags?: string | null;
  /**
   * Hash of the canonical body.md when available (nullable).
   */
  content_hash?: string | null;
  /**
   * Refresh policy: `on_change` | `scheduled` | `offline`.
   */
  refresh_policy: string;
}
