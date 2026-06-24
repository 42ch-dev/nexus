import { LoadedSchema, COMMON_DEFINITIONS } from './schema-loader';
import { resolveRef, isCommonEnum, getCommonBaseType } from './schema-loader';
import { resolveFromRoot, writeFile, logger, maxSchemaVersion } from './utils';
import path from 'path';
import fs from 'fs';

/**
 * Compute the TS folder segments (raw schemas/ folder names, hyphens preserved) for a schema.
 * Mirrors the consumer-scope `schemas/` tree. E.g. `platform/http-bff/context-assembly-v1.schema.json` → `['platform', 'http-bff']`.
 */
function tsFolderSegments(schema: LoadedSchema): string[] {
  return schema.modulePath.slice(0, -1).map((seg, i) => {
    // modulePath folder segments are underscored; restore hyphens from the original relPath folders
    const raw = schema.relPath.split('/').slice(0, -1);
    return raw[i] ?? seg;
  });
}

/**
 * Compute a relative TS import path (no extension) from one schema's folder to another type's file.
 * E.g. from ['platform','http-bff'] to type in ['common'] named 'CommonTypes' → '../../common/CommonTypes'.
 */
function relativeTsImport(fromFolder: string[], toFolder: string[], toTypeName: string): string {
  let common = 0;
  while (common < fromFolder.length && common < toFolder.length && fromFolder[common] === toFolder[common]) {
    common++;
  }
  const ups = fromFolder.length - common;
  const downs = toFolder.slice(common);
  const prefix = '../'.repeat(ups) || './';
  return prefix + [...downs, toTypeName].join('/');
}

/**
 * Check if a schema has object definitions with properties.
 */
function schemaHasObjectDefinitions(schema: LoadedSchema): boolean {
  const definitions = getDefinitions(schema);
  if (!definitions) return false;
  return Object.values(definitions).some(
    (def: any) => def.type === 'object' && def.properties && Object.keys(def.properties).length > 0,
  );
}

/**
 * Get definitions from a schema (supports both `definitions` and `$defs`).
 */
function getDefinitions(schema: LoadedSchema): Record<string, Record<string, unknown>> | undefined {
  const d = schema.schemaContent.definitions || schema.schemaContent.$defs;
  return d as Record<string, Record<string, unknown>> | undefined;
}

/**
 * Generate TypeScript types from schemas.
 *
 * Emits a nested file tree mirroring the consumer-scope `schemas/` layout:
 *   generated/{common,domain,platform/{http-bff,sync},local-api/compute}/<PascalType>.ts
 * The root `index.ts` re-exports every leaf module so the package public API stays flat.
 */
export function generateTSTypes(schemas: LoadedSchema[]): void {
  const outputDir = resolveFromRoot('packages', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating TypeScript types to: ${outputDir}`);

  // CommonTypes file under generated/common/
  generateCommonTypesFile(outputDir);

  // Collect schemas that produce type files
  const schemasWithTypes: LoadedSchema[] = [];

  // Build map: typeName → tsFolderSegments for relative import resolution
  const typeFolderMap = new Map<string, string[]>();
  typeFolderMap.set('SourceAnchor', ['common']);
  typeFolderMap.set('SourceSummaryRef', ['common']);

  for (const schema of schemas) {
    if (schema.isExplicitlySkipped) continue;

    const hasTopLevel = !schema.isDefinitionsOnly && !schema.isStandaloneEnum;
    const hasDefs = schemaHasObjectDefinitions(schema);

    if (!hasTopLevel && !hasDefs && !schema.isStandaloneEnum) continue;

    schemasWithTypes.push(schema);
    typeFolderMap.set(schema.typeName, tsFolderSegments(schema));
  }

  for (const schema of schemasWithTypes) {
    if (schema.isStandaloneEnum) {
      generateTSEnumFile(schema, outputDir, typeFolderMap);
    } else {
      generateTSTypeFile(schema, outputDir, hasTopLevelFor(schema), hasDefsFor(schema), typeFolderMap);
    }
  }

  // Generate index.ts with flat re-exports (nested relative paths)
  generateTSIndex(schemasWithTypes, outputDir);

  // Clean stale files recursively
  cleanupStaleTsFiles(outputDir, schemasWithTypes);

  logger.success(`Generated TypeScript types for ${schemasWithTypes.length} schema(s) (+ common types)`);
}

// Helpers to re-derive hasTopLevel/hasDefs without a second pass (kept inline-equivalent)
function hasTopLevelFor(schema: LoadedSchema): boolean {
  return !schema.isDefinitionsOnly && !schema.isStandaloneEnum;
}
function hasDefsFor(schema: LoadedSchema): boolean {
  return schemaHasObjectDefinitions(schema);
}

/**
 * Walk the generated TS tree and remove any .ts file not in the expected output.
 */
function cleanupStaleTsFiles(outputDir: string, schemasWithTypes: LoadedSchema[]): void {
  const keep = new Set<string>([path.join(outputDir, 'index.ts'), path.join(outputDir, 'common', 'CommonTypes.ts')]);
  for (const s of schemasWithTypes) {
    keep.add(path.join(outputDir, ...tsFolderSegments(s), `${s.typeName}.ts`));
  }
  const root = resolveFromRoot();
  let removed = 0;
  const walk = (dir: string) => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const ent of entries) {
      const full = path.join(dir, ent.name);
      if (ent.isDirectory()) {
        walk(full);
      } else if (ent.isFile() && ent.name.endsWith('.ts') && !keep.has(full)) {
        fs.unlinkSync(full);
        logger.warn(`Removed stale generated file: ${path.relative(root, full)}`);
        removed++;
      }
    }
  };
  walk(outputDir);
  if (removed > 0) logger.info(`Stale TS file cleanup removed ${removed} file(s)`);
}

/**
 * Generate index.ts with flat re-exports using nested relative paths.
 */
function generateTSIndex(schemas: LoadedSchema[], outputDir: string): void {
  const lines: string[] = [];
  lines.push(`/**`);
  lines.push(` * Nexus Wire Contracts - Generated TypeScript Types`);
  lines.push(` *`);
  lines.push(` * AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY`);
  lines.push(` * Source: schemas/*.schema.json`);
  lines.push(` * Generated by: pnpm run codegen`);
  lines.push(` */`);
  lines.push('');
  lines.push(`// Common types (type aliases, enums, SourceAnchor)`);
  lines.push(`export * from './common/CommonTypes';`);
  lines.push('');
  lines.push(`// Per-schema modules (stable order: sorted schema paths at load time)`);
  for (const s of schemas) {
    const rel = relativeTsImport([], tsFolderSegments(s), s.typeName);
    lines.push(`export * from '${rel}';`);
  }
  lines.push('');
  lines.push(`// Schema version constants`);
  lines.push(`export const SCHEMA_VERSIONS: Record<string, number> = {`);
  for (const s of schemas) {
    lines.push(`  ${s.typeName}: ${s.schemaVersion},`);
  }
  lines.push(`};`);
  lines.push('');
  lines.push(`// Highest schema_version among emitted contract schemas`);
  lines.push(`export const LATEST_SCHEMA_VERSION = ${maxSchemaVersion(schemas.map(s => s.schemaVersion))};`);

  writeFile(path.join(outputDir, 'index.ts'), lines.join('\n') + '\n');
}

/**
 * Generate TypeScript type alias file for a standalone enum schema.
 *
 * Produces a `type X = 'a' | 'b' | 'c';` from a JSON Schema with
 * `type: "string"` and `enum: [...]`.
 */
function generateTSEnumFile(schema: LoadedSchema, outputDir: string, _typeFolderMap: Map<string, string[]>): void {
  const values = schema.schemaContent.enum as string[];
  const content = `/**
 * ${schema.schemaContent.title || schema.typeName}
 *
 * ${schema.schemaContent.description || 'Generated from JSON Schema'}
 *
 * @schema_version ${schema.schemaVersion}
 * @source ${schema.fileName}
 */

/** ${schema.schemaContent.description || schema.typeName} */
export type ${schema.typeName} = ${values.map(v => `'${v}'`).join(' | ')};
`;

  writeFile(path.join(outputDir, ...tsFolderSegments(schema), `${schema.typeName}.ts`), content);
}

/**
 * Generate TypeScript type file(s) for a schema.
 *
 * Handles three cases:
 * 1. Top-level properties only → single interface
 * 2. Definitions only → one interface per definition
 * 3. Both → main interface + definition interfaces (all in same file)
 */
function generateTSTypeFile(
  schema: LoadedSchema,
  outputDir: string,
  hasTopLevel: boolean,
  hasDefs: boolean,
  typeFolderMap: Map<string, string[]>,
): void {
  const thisFolder = tsFolderSegments(schema);
  let content = `/**
 * ${schema.schemaContent.title || schema.typeName}
 *
 * ${schema.schemaContent.description || 'Generated from JSON Schema'}
 *
 * @schema_version ${schema.schemaVersion}
 * @source ${schema.fileName}
 */
`;

  const localDefinitions = hasDefs ? getDefinitions(schema) : undefined;
  const commonTypeImports: Set<string> = new Set(['SchemaVersion']);
  const crossFileImports: Set<string> = new Set(); // For types like Delta that are in separate files
  const allInlineEnums: Map<string, string[]> = new Map();
  const definitionNames: string[] = [];

  // Check if SourceAnchor is referenced anywhere in this schema
  const schemaJSON = JSON.stringify(schema.schemaContent);
  if (schemaJSON.includes('source-anchor.schema.json')) {
    commonTypeImports.add('SourceAnchor');
  }

  // Generate main interface from top-level properties
  if (hasTopLevel) {
    const { fieldsText, commonImports: fieldCommon, crossSchemaImports: fieldCross } =
      generateTSTypeFields(
        schema.schemaContent,
        schema.typeName,
        commonTypeImports,
        allInlineEnums,
        localDefinitions,
      );
    for (const imp of fieldCommon) {
      commonTypeImports.add(imp);
    }
    for (const imp of fieldCross) {
      crossFileImports.add(imp);
    }
    content += fieldsText + '\n';
  }

  // Generate interfaces from definitions
  if (hasDefs && localDefinitions) {
    for (const [defName, defContent] of Object.entries(localDefinitions)) {
      const def = defContent as Record<string, unknown>;
      if (def.type !== 'object' || !def.properties) continue;

      definitionNames.push(defName);
      const { fieldsText, commonImports: fieldCommon, crossSchemaImports: fieldCross } =
        generateTSTypeFields(
          def,
          defName,
          commonTypeImports,
          allInlineEnums,
          localDefinitions,
        );
      for (const imp of fieldCommon) {
        commonTypeImports.add(imp);
      }
      for (const imp of fieldCross) {
        crossFileImports.add(imp);
      }
      content += fieldsText + '\n';
    }
  }

  // Build imports with nested relative paths
  const commonImports = [...commonTypeImports].sort();
  const crossImports = [...crossFileImports].sort();

  if (commonImports.length > 0) {
    const commonRel = relativeTsImport(thisFolder, ['common'], 'CommonTypes');
    content = `import type { ${commonImports.join(', ')} } from '${commonRel}';\n` + content;
  }
  if (crossImports.length > 0) {
    const crossLines = crossImports.map(name => {
      const targetFolder = typeFolderMap.get(name) ?? ['common'];
      const rel = relativeTsImport(thisFolder, targetFolder, name);
      return `import type { ${name} } from '${rel}';`;
    });
    content = `${crossLines.join('\n')}\n` + content;
  }

  // Define inline enums as type aliases
  let enumBlock = '';
  for (const [enumName, values] of allInlineEnums) {
    enumBlock += `\n/** Inline enum type */\nexport type ${enumName} = ${values.map(v => `'${v}'`).join(' | ')};\n`;
  }

  // Insert enum block after imports
  if (enumBlock) {
    const importEnd = content.indexOf('*/', content.indexOf('import type'));
    if (importEnd !== -1) {
      const afterImport = content.indexOf('\n', importEnd) + 1;
      content = content.slice(0, afterImport) + enumBlock + '\n' + content.slice(afterImport);
    }
  }

  writeFile(path.join(outputDir, ...thisFolder, `${schema.typeName}.ts`), content);
}

/**
 * Generate TypeScript interface fields from a schema or definition object.
 * Returns the interface text and any common type imports needed.
 */
function generateTSTypeFields(
  schemaContent: Record<string, unknown>,
  typeName: string,
  existingImports: Set<string>,
  inlineEnums: Map<string, string[]>,
  localDefinitions?: Record<string, Record<string, unknown>>,
): {
  fieldsText: string;
  commonImports: Set<string>;
  crossSchemaImports: Set<string>;
} {
  const properties = (schemaContent.properties || {}) as Record<string, unknown>;
  const requiredFields = (schemaContent.required || []) as string[];
  const commonImports = new Set<string>();
  const crossSchemaImports = new Set<string>();
  const fields: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    const def = propDef as Record<string, unknown>;
    const isRequired = requiredFields.includes(propName);
    const { tsType, commonRef, crossSchemaRef } = resolveTSType(
      def,
      propName,
      inlineEnums,
      localDefinitions,
      typeName,
    );
    if (commonRef) {
      commonImports.add(commonRef);
    }
    if (crossSchemaRef) {
      crossSchemaImports.add(crossSchemaRef);
    }
    const optionalMark = isRequired ? '' : '?';
    // Quote property names that contain hyphens (invalid in unquoted TS identifiers)
    const tsPropName = /-/.test(propName) ? `'${propName}'` : propName;
    fields.push(`  ${tsPropName}${optionalMark}: ${tsType};`);
  }

  const desc = (schemaContent.description || typeName) as string;
  const fieldsText = `/** ${desc} */\nexport interface ${typeName} {\n${fields.join('\n')}\n}`;
  return { fieldsText, commonImports, crossSchemaImports };
}

/**
 * Resolve a property definition to a TypeScript type string.
 * Returns { tsType, commonRef?, crossSchemaRef? } where commonRef maps to CommonTypes,
 * and crossSchemaRef names another generated schema module (e.g. Delta, ForkBranch).
 *
 * @param inlineEnumPathPrefix PascalCase path prefix for inline string enums (e.g. Creator, CreatorMetadata)
 *        so barrel `export *` does not collide on names like `Status` across modules.
 */
function resolveTSType(
  propDef: Record<string, unknown>,
  propName: string,
  inlineEnums: Map<string, string[]>,
  localDefinitions: Record<string, Record<string, unknown>> | undefined,
  inlineEnumPathPrefix: string,
): { tsType: string; commonRef?: string; crossSchemaRef?: string } {
  const ref = propDef.$ref as string | undefined;
  const type = propDef.type;

  // Handle $ref
  if (ref) {
    const defName = resolveRef(ref);
    if (defName === 'SourceAnchor') {
      return { tsType: 'SourceAnchor', commonRef: 'SourceAnchor' };
    }
    if (defName === 'Delta') {
      return { tsType: 'Delta', crossSchemaRef: 'Delta' };
    }
    if (defName && isCommonEnum(defName)) {
      return { tsType: defName, commonRef: defName };
    }
    // Check local definitions (e.g., #/definitions/AgentEntry within same schema)
    if (defName && localDefinitions && defName in localDefinitions) {
      return { tsType: defName };
    }
    if (defName && COMMON_DEFINITIONS.has(defName)) {
      return { tsType: getCommonBaseType(defName) };
    }
    if (defName) {
      return { tsType: defName, crossSchemaRef: defName };
    }
    return { tsType: 'unknown' };
  }

  // Handle type arrays (e.g., ["string", "null"], ["array", "null"])
  if (Array.isArray(type)) {
    const nonNullTypes = type.filter(t => t !== 'null');
    const hasNull = type.includes('null');
    if (nonNullTypes.length === 1 && nonNullTypes[0] === 'array' && propDef.items) {
      const items = propDef.items as Record<string, unknown>;
      if (items.type === 'object' && items.properties) {
        const path = `${inlineEnumPathPrefix}${toPascalCase(propName)}`;
        const inner = buildInlineObjectType(items, inlineEnums, localDefinitions, path);
        const tsType = `${inner}[]`;
        return { tsType: hasNull ? `${tsType} | null` : tsType };
      }
      const inner = resolveTSType(
        items,
        '',
        inlineEnums,
        localDefinitions,
        `${inlineEnumPathPrefix}${toPascalCase(propName)}`,
      );
      const tsType = `${inner.tsType}[]`;
      return {
        tsType: hasNull ? `${tsType} | null` : tsType,
        commonRef: inner.commonRef,
        crossSchemaRef: inner.crossSchemaRef,
      };
    }
    if (nonNullTypes.length === 1) {
      const base = resolveSingleTSType(
        nonNullTypes[0],
        propDef,
        propName,
        inlineEnums,
        localDefinitions,
        inlineEnumPathPrefix,
      );
      return { tsType: hasNull ? `${base} | null` : base };
    }
    return { tsType: 'unknown' };
  }

  // Array of $ref (e.g. MemoryType[]) must bubble commonRef / crossSchemaRef for imports
  if (type === 'array' && propDef.items) {
    const items = propDef.items as Record<string, unknown>;
    if (items.type === 'object' && items.properties) {
      const path = `${inlineEnumPathPrefix}${toPascalCase(propName)}`;
      const inner = buildInlineObjectType(items, inlineEnums, localDefinitions, path);
      return { tsType: `${inner}[]` };
    }
    const inner = resolveTSType(
      items,
      '',
      inlineEnums,
      localDefinitions,
      `${inlineEnumPathPrefix}${toPascalCase(propName)}`,
    );
    return {
      tsType: `${inner.tsType}[]`,
      commonRef: inner.commonRef,
      crossSchemaRef: inner.crossSchemaRef,
    };
  }

  return {
    tsType: resolveSingleTSType(
      type as string,
      propDef,
      propName,
      inlineEnums,
      localDefinitions,
      inlineEnumPathPrefix,
    ),
  };
}

/**
 * Resolve a single JSON Schema type to TypeScript.
 */
function resolveSingleTSType(
  type: string,
  propDef: Record<string, unknown>,
  propName: string,
  inlineEnums: Map<string, string[]>,
  localDefinitions: Record<string, Record<string, unknown>> | undefined,
  inlineEnumPathPrefix: string,
): string {
  switch (type) {
    case 'string':
      if (propDef.enum) {
        const values = propDef.enum as string[];
        // Empty propName: caller already folded the segment (e.g. array item type for `memory_kinds`).
        const enumName =
          propName === '' ? inlineEnumPathPrefix : `${inlineEnumPathPrefix}${toPascalCase(propName)}`;
        inlineEnums.set(enumName, values);
        return enumName;
      }
      return 'string';
    case 'number':
    case 'integer':
      return 'number';
    case 'boolean':
      return 'boolean';
    case 'array': {
      if (propDef.items) {
        const items = propDef.items as Record<string, unknown>;
        if (items.type === 'object' && items.properties) {
          const path = `${inlineEnumPathPrefix}${toPascalCase(propName)}`;
          return buildInlineObjectType(items, inlineEnums, localDefinitions, path) + '[]';
        }
        const { tsType } = resolveTSType(
          items,
          '',
          inlineEnums,
          localDefinitions,
          `${inlineEnumPathPrefix}${toPascalCase(propName)}`,
        );
        return `${tsType}[]`;
      }
      return 'unknown[]';
    }
    case 'object': {
      if (propDef.properties) {
        const path = `${inlineEnumPathPrefix}${toPascalCase(propName)}`;
        return buildInlineObjectType(propDef, inlineEnums, localDefinitions, path);
      }
      return 'Record<string, unknown>';
    }
    default:
      return 'unknown';
  }
}

/**
 * Build an inline object type from a property definition with properties.
 */
function buildInlineObjectType(
  objDef: Record<string, unknown>,
  inlineEnums: Map<string, string[]>,
  localDefinitions: Record<string, Record<string, unknown>> | undefined,
  /** PascalCase path to this anonymous object (parent prefix + property segment). */
  objectPathPrefix: string,
): string {
  const props = (objDef.properties || {}) as Record<string, unknown>;
  const required = (objDef.required || []) as string[];
  const parts: string[] = [];

  for (const [k, v] of Object.entries(props)) {
    const isReq = required.includes(k);
    const { tsType } = resolveTSType(v as Record<string, unknown>, k, inlineEnums, localDefinitions, objectPathPrefix);
    parts.push(`${k}${isReq ? '' : '?'}: ${tsType}`);
  }

  return `{ ${parts.join('; ')} }`;
}

/**
 * Convert a snake_case property name to PascalCase.
 * e.g., "bundle_type" -> "BundleType", "delta_type" -> "DeltaType"
 */
function toPascalCase(snakeStr: string): string {
  return snakeStr
    .split('_')
    .map(w => w.charAt(0).toUpperCase() + w.slice(1))
    .join('');
}

/**
 * Generate CommonTypes.ts with shared type definitions.
 * Driven from COMMON_DEFINITIONS map (populated from common.schema.json).
 */
function generateCommonTypesFile(outputDir: string): void {
  const typeAliases: Array<{ name: string; type: string; desc: string }> = [];
  const enums: Array<{ name: string; values: string[]; desc: string }> = [];

  for (const [name, def] of COMMON_DEFINITIONS.entries()) {
    if (def.enum) {
      enums.push({
        name,
        values: def.enum,
        desc: def.description || `Enum type ${name}`,
      });
    } else {
      const tsType = def.type === 'integer' ? 'number' : 'string';
      typeAliases.push({
        name,
        type: tsType,
        desc: def.description || `Type alias ${name}`,
      });
    }
  }

  let content = `/**
 * Nexus Common Types
 *
 * Shared type definitions extracted from schemas/common/common.schema.json
 *
 * AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
 * Generated by: pnpm run codegen
 */

`;

  for (const alias of typeAliases) {
    content += `/** ${alias.desc} */\nexport type ${alias.name} = ${alias.type};\n\n`;
  }

  for (const en of enums) {
    content += `/** ${en.desc} */\nexport type ${en.name} = ${en.values.map(v => `'${v}'`).join(' | ')};\n\n`;
  }

  content += `/**
 * SourceAnchor - Value object for referencing platform Story summary entities.
 * Source: schemas/common/source-anchor.schema.json
 */
export interface SourceAnchor {
  story_summary_refs?: SourceSummaryRef[];
  excerpt?: string;
  summary?: string;
}

export interface SourceSummaryRef {
  story_manifest_id: string;
  summary_unit_id: string;
  unit_kind?: string;
}
`;

  writeFile(path.join(outputDir, 'common', 'CommonTypes.ts'), content);
}
