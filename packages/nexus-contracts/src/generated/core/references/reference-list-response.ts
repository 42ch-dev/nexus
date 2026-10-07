/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

import type { ReferenceSourceInfo } from './reference-source-info';

/**
 * Response for `GET /v1/daemon/references`.
 */
export interface ReferenceListResponse {
  references: ReferenceSourceInfo[];
}
