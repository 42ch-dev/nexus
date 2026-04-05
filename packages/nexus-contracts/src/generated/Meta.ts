/**
 * Nexus Meta Schema
 *
 * Meta schema defining schema versioning and structure rules for all Nexus schemas
 *
 * @schema_version 1
 * @source meta.schema.json
 */
import type { SchemaVersion } from './CommonTypes';

/** Inline enum type */
export type Type = 'object' | 'array' | 'string' | 'number' | 'integer' | 'boolean' | 'null';

export interface Meta {
  $schema: string;
  $id: string;
  schema_version: number;
  title: string;
  description?: string;
  type: Type;
}
