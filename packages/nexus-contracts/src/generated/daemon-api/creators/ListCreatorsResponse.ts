import type { CreatorInfo } from './CreatorInfo';
import type { PaginationInfo } from '../kb/PaginationInfo';
import type { SchemaVersion } from '../../common/CommonTypes';
/**
 * Nexus ListCreatorsResponse
 *
 * Response for GET /v1/daemon/creators.
 *
 * @schema_version 1
 * @source list-creators-response.schema.json
 */
/** Response for GET /v1/daemon/creators. */
export interface ListCreatorsResponse {
  items: CreatorInfo[];
  pagination: PaginationInfo;
}
