/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { WorkInspirationItem } from './work-inspiration-item';

/**
 * Offset-paginated inspiration page for `GET /v1/daemon/works/pool/inspiration`.
 */
export interface WorkInspirationListResponse {
  items: WorkInspirationItem[];
  total: number;
  limit: number;
  offset: number;
}
