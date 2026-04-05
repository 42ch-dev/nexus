import { globSync } from 'glob';
import { readJSON, resolveFromRoot, extractSchemaVersion, schemaToTypeName, logger } from './utils';
import path from 'path';

export interface LoadedSchema {
  filePath: string;
  fileName: string;
  /** PascalCase type name derived from file name */
  typeName: string;
  /** Integer schema version */
  schemaVersion: number;
  schemaContent: Record<string, unknown>;
  /** Whether this schema is definitions-only (no own properties to generate) */
  isDefinitionsOnly: boolean;
}

// Schema files that are definitions-only or already embedded in common_types
const SKIP_STRUCT_GENERATION = new Set([
  'common.schema.json',
  'source-anchor.schema.json',
]);

/**
 * Map of definition names from common.schema.json to their base types.
 * Updated when common schema is loaded.
 */
export const COMMON_DEFINITIONS: Map<string, { type: string; enum?: string[] }> = new Map();

/**
 * Load all schema files from schemas/ directory.
 * Excludes non-schema files.
 */
export function loadAllSchemas(): LoadedSchema[] {
  const schemasDir = resolveFromRoot('schemas');
  logger.info(`Loading schemas from: ${schemasDir}`);

  const pattern = path.join(schemasDir, '**/*.schema.json');
  const files = globSync(pattern);

  if (files.length === 0) {
    logger.error('No schema files found');
    return [];
  }

  logger.info(`Found ${files.length} schema files`);

  const loadedSchemas: LoadedSchema[] = [];

  // First pass: load common schema to populate COMMON_DEFINITIONS
  const commonSchemaPath = path.join(schemasDir, 'common', 'common.schema.json');
  if (fs_existsSync(commonSchemaPath)) {
    const commonContent = readJSON<Record<string, unknown>>(commonSchemaPath);
    const definitions = commonContent.definitions as Record<string, unknown> | undefined;
    if (definitions) {
      for (const [name, def] of Object.entries(definitions)) {
        const d = def as Record<string, unknown>;
        COMMON_DEFINITIONS.set(name, {
          type: (d.type as string) || 'string',
          enum: d.enum as string[] | undefined,
        });
      }
      logger.info(`Loaded ${COMMON_DEFINITIONS.size} common type definitions`);
    }
  }

  for (const filePath of files) {
    const fileName = path.basename(filePath);
    const typeName = schemaToTypeName(fileName);
    const schemaContent = readJSON<Record<string, unknown>>(filePath);
    const schemaVersion = extractSchemaVersion(schemaContent);

    // Determine if this schema should be skipped for struct generation
    const properties = schemaContent.properties as Record<string, unknown> | undefined;
    const isDefinitionsOnly = SKIP_STRUCT_GENERATION.has(fileName)
      || !properties || Object.keys(properties).length === 0;

    loadedSchemas.push({
      filePath,
      fileName,
      typeName,
      schemaVersion,
      schemaContent,
      isDefinitionsOnly,
    });

    logger.info(`  Loaded: ${fileName} -> ${typeName} (v${schemaVersion})${isDefinitionsOnly ? ' [definitions-only]' : ''}`);
  }

  return loadedSchemas;
}

/**
 * Validate schema has required top-level fields.
 */
export function validateSchemaStructure(schema: LoadedSchema): boolean {
  const requiredFields = ['$schema', '$id', 'schema_version', 'title', 'type'];

  for (const field of requiredFields) {
    if (!(field in schema.schemaContent)) {
      logger.error(`Schema ${schema.fileName} missing required field: ${field}`);
      return false;
    }
  }

  return true;
}

/**
 * Resolve a $ref URI to a definition name.
 * Handles both local refs (#/definitions/X) and full URIs.
 */
export function resolveRef(ref: string): string | null {
  // Full URI ref: https://nexus.42ch.io/schemas/common/common.schema.json#/definitions/BundleId
  if (ref.includes('#/definitions/')) {
    const match = ref.match(/#\/definitions\/(\w+)$/);
    if (match) {
      return match[1];
    }
  }
  // Local ref: #/definitions/X
  const localMatch = ref.match(/^#\/definitions\/(\w+)$/);
  if (localMatch) {
    return localMatch[1];
  }
  // Whole-schema ref (e.g., source-anchor.schema.json)
  if (ref.includes('source-anchor.schema.json')) {
    return 'SourceAnchor';
  }
  return null;
}

/**
 * Check if a common definition name is an enum type.
 */
export function isCommonEnum(defName: string): boolean {
  const def = COMMON_DEFINITIONS.get(defName);
  return !!def && !!def.enum;
}

/**
 * Get enum values for a common definition.
 */
export function getCommonEnumValues(defName: string): string[] {
  const def = COMMON_DEFINITIONS.get(defName);
  return def?.enum || [];
}

/**
 * Get the base type for a common definition.
 */
export function getCommonBaseType(defName: string): string {
  const def = COMMON_DEFINITIONS.get(defName);
  if (!def) return 'string';
  if (def.enum) return defName; // It's an enum, use the name
  switch (def.type) {
    case 'integer':
      if (defName === 'SchemaVersion') return 'number'; // u32 in Rust
      return 'number';
    default:
      return 'string';
  }
}

// Polyfill for fs.existsSync (avoid needing to import fs in this module)
function fs_existsSync(filePath: string): boolean {
  const { existsSync } = require('fs');
  return existsSync(filePath);
}
