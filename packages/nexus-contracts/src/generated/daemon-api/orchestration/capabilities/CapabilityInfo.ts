import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus CapabilityInfo
 *
 * Description of a single registered capability (name + I/O schemas).
 *
 * @schema_version 1
 * @source capability-info.schema.json
 */
/** Description of a single registered capability (name + I/O schemas). */
export interface CapabilityInfo {
  name: string;
  input_schema: string;
  output_schema: string;
}
