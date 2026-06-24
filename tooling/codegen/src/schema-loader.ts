import fs from 'fs';
import { globSync } from 'glob';
import path from 'path';
import { readJSON, resolveFromRoot, extractSchemaVersion, schemaToTypeName, toSnakeCase, logger } from './utils';

export interface LoadedSchema {
  filePath: string;
  fileName: string;
  /** Path relative to schemas/, POSIX slashes (e.g. `platform/http-bff/context-assembly-v1.schema.json`) */
  relPath: string;
  /** PascalCase type name derived from file name */
  typeName: string;
  /**
   * Generated module path segments derived from the schema's folder location, mirroring the
   * `schemas/` tree (e.g. `['platform', 'http_bff', 'context_assembly_v1']`).
   * Folder hyphens become underscores; the leaf segment is `toSnakeCase(typeName)`.
   */
  modulePath: string[];
  /** Integer schema version */
  schemaVersion: number;
  schemaContent: Record<string, unknown>;
  /** Whether this schema is definitions-only (no own top-level properties) */
  isDefinitionsOnly: boolean;
  /** Whether this schema is a standalone enum (type: "string" + enum, no properties) */
  isStandaloneEnum: boolean;
  /** Whether this schema is in an explicit skip list and should not generate any types */
  isExplicitlySkipped: boolean;
}

// Schema files that are definitions-only or already embedded in common_types
const SKIP_STRUCT_GENERATION = new Set([
  'common.schema.json',
  'source-anchor.schema.json',
]);

/**
 * Schema paths (relative to schemas/, POSIX slashes) that must not emit TS/Rust structs.
 * Used when a JSON Schema refines another file with the same basename (e.g. platform/sync/bundle-refinement
 * allOf platform/sync/bundle): codegen only produces types from the canonical envelope schema.
 */
const SKIP_STRUCT_GENERATION_REL_PATHS = new Set(['platform/sync/bundle-refinement.schema.json']);

/**
 * Compute the generated module path segments for a schema from its path relative to `schemas/`.
 * Mirrors the consumer-scope `schemas/` tree: folder hyphens → underscores, leaf = snake_case type name.
 * Examples:
 *   `domain/world.schema.json`                          → `['domain', 'world']`
 *   `common/version-ref.schema.json`                    → `['common', 'version_ref']`
 *   `platform/http-bff/context-assembly-v1.schema.json` → `['platform', 'http_bff', 'context_assembly_v1']`
 *   `platform/sync/bundle.schema.json`                  → `['platform', 'sync', 'bundle']`
 *   `local-api/compute/compute-input.schema.json`       → `['local_api', 'compute', 'compute_input']`
 */
export function computeModulePath(relPath: string, typeName: string): string[] {
  const parts = relPath.split('/');
  const folders = parts.slice(0, -1).map(seg => seg.replace(/-/g, '_'));
  return [...folders, toSnakeCase(typeName)];
}

/** Canonical module path for the synthetic common_types module (aggregates common.schema.json definitions). */
export const COMMON_TYPES_MODULE_PATH = ['common', 'common_types'];

/**
 * Map of definition names from common.schema.json to their base types.
 * Updated when common schema is loaded.
 */
export const COMMON_DEFINITIONS: Map<string, { type: string; enum?: string[]; description?: string }> = new Map();

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
  if (fs.existsSync(commonSchemaPath)) {
    const commonContent = readJSON<Record<string, unknown>>(commonSchemaPath);
    const definitions = commonContent.definitions as Record<string, unknown> | undefined;
    if (definitions) {
      for (const [name, def] of Object.entries(definitions)) {
        const d = def as Record<string, unknown>;
        COMMON_DEFINITIONS.set(name, {
          type: (d.type as string) || 'string',
          enum: d.enum as string[] | undefined,
          description: d.description as string | undefined,
        });
      }
      logger.info(`Loaded ${COMMON_DEFINITIONS.size} common type definitions`);
    }
  }

  const sortedFiles = [...files].sort((a, b) => {
    const ra = path.relative(schemasDir, a).replace(/\\/g, '/');
    const rb = path.relative(schemasDir, b).replace(/\\/g, '/');
    return ra.localeCompare(rb);
  });

  for (const filePath of sortedFiles) {
    const fileName = path.basename(filePath);
    const relPath = path.relative(schemasDir, filePath).replace(/\\/g, '/');
    const typeName = schemaToTypeName(fileName);
    const schemaContent = readJSON<Record<string, unknown>>(filePath);
    const schemaVersion = extractSchemaVersion(schemaContent);

    // Determine if this schema should be skipped for struct generation
    const properties = schemaContent.properties as Record<string, unknown> | undefined;
    const hasProperties = properties && Object.keys(properties).length > 0;
    const isExplicitlySkipped = SKIP_STRUCT_GENERATION.has(fileName)
      || SKIP_STRUCT_GENERATION_REL_PATHS.has(relPath);

    // Detect standalone enum schemas: type is "string" with an enum array, no properties
    const isStandaloneEnum = !isExplicitlySkipped
      && !hasProperties
      && schemaContent.type === 'string'
      && Array.isArray(schemaContent.enum)
      && (schemaContent.enum as string[]).length > 0;

    const isDefinitionsOnly = !isExplicitlySkipped && !hasProperties && !isStandaloneEnum;

    loadedSchemas.push({
      filePath,
      fileName,
      relPath,
      typeName,
      modulePath: computeModulePath(relPath, typeName),
      schemaVersion,
      schemaContent,
      isDefinitionsOnly,
      isStandaloneEnum,
      isExplicitlySkipped,
    });

    logger.info(`  Loaded: ${fileName} -> ${typeName} (v${schemaVersion})${isDefinitionsOnly ? ' [definitions-only]' : ''}${isStandaloneEnum ? ' [standalone-enum]' : ''}`);
  }

  assertUniqueTypeNames(loadedSchemas);

  return loadedSchemas;
}

/**
 * Fail fast if two emitting schema files map to the same PascalCase type name (basename collision).
 * Skipped schemas (e.g. cloud-sync refinements of a domain envelope) may share a basename with the canonical file.
 */
function assertUniqueTypeNames(schemas: LoadedSchema[]): void {
  const byType = new Map<string, string>();
  for (const s of schemas) {
    if (s.isExplicitlySkipped) {
      continue;
    }
    const prev = byType.get(s.typeName);
    if (prev) {
      logger.error(
        `Duplicate generated type name "${s.typeName}": ${prev} and ${s.filePath} — rename one of the schema files.`,
      );
      process.exit(1);
    }
    byType.set(s.typeName, s.filePath);
  }
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
  // Full URI ref: https://nexus42.invalid/schemas/common/common.schema.json#/definitions/BundleId
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
  // Relative file ref (e.g. bundle.schema.json → "delta.schema.json")
  if (/^[A-Za-z0-9_.-]+\.schema\.json$/.test(ref)) {
    return schemaToTypeName(ref);
  }
  // Whole-schema HTTPS URI: .../fork-branch.schema.json (no #/definitions)
  const fileOnlyMatch = ref.match(/\/([A-Za-z0-9_.-]+\.schema\.json)$/);
  if (fileOnlyMatch && !ref.includes('#/definitions/')) {
    return schemaToTypeName(fileOnlyMatch[1]);
  }
  // Whole-schema ref (legacy substring checks)
  if (ref.includes('source-anchor.schema.json')) {
    return 'SourceAnchor';
  }
  if (ref.includes('delta.schema.json')) {
    return 'Delta';
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

