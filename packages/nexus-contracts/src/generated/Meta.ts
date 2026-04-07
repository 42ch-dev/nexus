import type { SchemaVersion } from './CommonTypes';
/**
 * Nexus Meta Schema
 *
 * Meta schema defining schema versioning and structure rules for all Nexus schemas
 *
 * @schema_version 1
 * @source meta.schema.json
 */

/** Inline enum type */
export type MetaType = 'object' | 'array' | 'string' | 'number' | 'integer' | 'boolean' | 'null';

/** Meta schema defining schema versioning and structure rules for all Nexus schemas */
export interface Meta {
  $schema: string;
  $id: string;
  schema_version: number;
  title: string;
  description?: string;
  type: MetaType;
}
