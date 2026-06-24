import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * CreatorRuntimePolicyResponseV1
 *
 * GET /creators/:id/runtime-policy 200 response body. Exposes Creator-level policy capabilities for CLI consumption. SSOT: v1-spec platform/local-first-runtime-policy-v1.md §4, §7.
 *
 * @schema_version 1
 * @source creator-runtime-policy-response.schema.json
 */
/** GET /creators/:id/runtime-policy 200 response body. Exposes Creator-level policy capabilities for CLI consumption. SSOT: v1-spec platform/local-first-runtime-policy-v1.md §4, §7. */
export interface CreatorRuntimePolicyResponse {
  schema_version: number;
  creator_id: string;
  memory_structured_write: boolean;
  memory_vector_index: boolean;
  local_first_embedding_remaining?: number;
}
