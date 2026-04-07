import type { SchemaVersion } from './CommonTypes';

/**
 * Nexus VersionRef
 *
 * Value object describing the baseline version of a bundle/entity/world. Aligned with data-model-v1.md §6.2.
 *
 * @schema_version 1
 * @source version-ref.schema.json
 */
/** Value object describing the baseline version of a bundle/entity/world. Aligned with data-model-v1.md §6.2. */
export interface VersionRef {
  entity_type: string;
  entity_id: string;
  revision: number;
}
