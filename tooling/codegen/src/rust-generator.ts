import { LoadedSchema, COMMON_DEFINITIONS } from './schema-loader';
import { resolveRef, isCommonEnum } from './schema-loader';
import { resolveFromRoot, writeFile, logger, toSnakeCase, removeStaleGeneratedFiles, maxSchemaVersion } from './utils';
import path from 'path';

/** Rust reserved words that cannot be used as field names without r# prefix */
const RUST_RESERVED_WORDS = new Set([
  'type', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn',
  'else', 'enum', 'extern', 'fn', 'for', 'if', 'impl', 'in', 'let',
  'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return',
  'self', 'Self', 'static', 'struct', 'super', 'trait', 'unsafe', 'use',
  'where', 'while', 'yield',
]);

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
 * Singularize a snake_case name for use as a struct name for inline array items.
 * e.g., "key_blocks" -> "key_block", "story_summaries" -> "story_summary"
 */
function singularize(snakeName: string): string {
  if (snakeName.endsWith('ies')) return snakeName.slice(0, -3) + 'y';
  if (snakeName.endsWith('ses') && snakeName.length > 4) return snakeName;
  if (snakeName.endsWith('s') && !snakeName.endsWith('ss')) return snakeName.slice(0, -1);
  return snakeName;
}

/**
 * Derive a unique name for an inline array item struct.
 * Uses `{ParentTypeName}{SingularPropertyName}` to avoid name collisions
 * with top-level schema types.
 */
function inlineItemTypeName(parentTypeName: string, propName: string): string {
  const singular = singularize(propName);
  const pascal = toPascalCase(singular);
  return `${parentTypeName}${pascal}`;
}

/**
 * Convert a snake_case name to PascalCase.
 * e.g., "key_block" -> "KeyBlock"
 */
function toPascalCase(snakeStr: string): string {
  return snakeStr
    .split('_')
    .map(w => w.charAt(0).toUpperCase() + w.slice(1))
    .join('');
}

/**
 * Check if a property name contains characters that need serde renaming.
 */
function needsSerdeRename(propName: string): boolean {
  return propName.startsWith('$') || propName.includes('-') || RUST_RESERVED_WORDS.has(propName);
}

/**
 * Get the Rust-safe field name for a JSON property.
 */
function rustFieldName(propName: string): string {
  if (propName.startsWith('$')) {
    return `dollar_${propName.slice(1)}`;
  }
  if (RUST_RESERVED_WORDS.has(propName)) {
    return `r#${propName}`;
  }
  // Hyphenated names: replace hyphens with underscores for Rust identifier
  return propName.replace(/-/g, '_');
}

/**
 * Generate Rust types from schemas.
 */
export function generateRustTypes(schemas: LoadedSchema[]): void {
  const outputDir = resolveFromRoot('crates', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating Rust types to: ${outputDir}`);

  // Generate common types module first
  generateRustCommonTypes(outputDir);

  // Collect schemas that produce type files
  const schemasWithTypes: LoadedSchema[] = [];

  for (const schema of schemas) {
    if (schema.isExplicitlySkipped) continue;

    const hasTopLevel = !schema.isDefinitionsOnly && !schema.isStandaloneEnum;
    const hasDefs = schemaHasObjectDefinitions(schema);

    if (!hasTopLevel && !hasDefs && !schema.isStandaloneEnum) continue;

    schemasWithTypes.push(schema);

    if (schema.isStandaloneEnum) {
      generateRustEnumFile(schema, outputDir);
    } else {
      // Generate individual type files
      generateRustTypeFile(schema, outputDir, hasTopLevel, hasDefs);
    }
  }

  // Generate mod.rs with module declarations
  generateRustMod(schemasWithTypes, outputDir);

  const keepRs = new Set(['mod.rs', 'common_types.rs']);
  for (const s of schemasWithTypes) {
    keepRs.add(`${toSnakeCase(s.typeName)}.rs`);
  }
  removeStaleGeneratedFiles(outputDir, keepRs, '.rs');

  logger.success(`Generated Rust types for ${schemasWithTypes.length} schema(s) (+ common types)`);
}

/**
 * Generate mod.rs with module declarations and re-exports.
 */
function generateRustMod(schemas: LoadedSchema[], outputDir: string): void {
  const modules: string[] = ['pub mod common_types;'];

  for (const schema of schemas) {
    const moduleName = toSnakeCase(schema.typeName);
    modules.push(`pub mod ${moduleName};`);
  }

  const content = `//! Nexus Wire Contracts - Generated Rust Types
//!
//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
//! Source: schemas/*.schema.json
//! Generated by: pnpm run codegen

${modules.join('\n')}

// Re-export all types at the generated module level
pub use common_types::*;

${schemas.map(s => `pub use ${toSnakeCase(s.typeName)}::*;`).join('\n')}

/// Schema version constants
pub const SCHEMA_VERSIONS: &[(&str, u32)] = &[
${schemas.map(s => `    ("${s.typeName}", ${s.schemaVersion}),`).join('\n')}
];

/// Highest schema_version among emitted contract schemas
pub const LATEST_SCHEMA_VERSION: u32 = ${maxSchemaVersion(schemas.map(s => s.schemaVersion))};
`;

  writeFile(path.join(outputDir, 'mod.rs'), content);
}

/**
 * Convert a snake_case enum variant value to PascalCase.
 * e.g., "local_only" -> "LocalOnly"
 */
function snakeToPascal(snakeStr: string): string {
  return snakeStr
    .split('_')
    .map(w => w.charAt(0).toUpperCase() + w.slice(1))
    .join('');
}

/**
 * Generate Rust enum file for a standalone enum schema.
 *
 * Produces a `#[derive(...)] pub enum X { A, B, C }` from a JSON Schema with
 * `type: "string"` and `enum: [...]`.
 */
function generateRustEnumFile(schema: LoadedSchema, outputDir: string): void {
  const values = schema.schemaContent.enum as string[];
  const moduleName = toSnakeCase(schema.typeName);

  const variants = values.map(v => {
    const pascal = snakeToPascal(v);
    // Build doc comment from enumDescriptions if available
    const descriptions = schema.schemaContent.enumDescriptions as Record<string, string> | undefined;
    const desc = descriptions?.[v];
    const docComment = desc ? `    /// ${desc}\n` : '';
    return `${docComment}    #[serde(rename = "${v}")]\n    ${pascal},`;
  });

  const content = `//! ${schema.schemaContent.title || schema.typeName}
//!
//! ${schema.schemaContent.description || 'Generated from JSON Schema'}
//!
//! @schema_version ${schema.schemaVersion}
//! @source ${schema.fileName}

use serde::{Deserialize, Serialize};

/// ${schema.schemaContent.description || schema.typeName}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ${schema.typeName} {
${variants.join('\n')}
}
`;

  writeFile(path.join(outputDir, `${moduleName}.rs`), content);
}

/**
 * Generate individual Rust type file for a schema.
 *
 * Handles three cases:
 * 1. Top-level properties only → single struct
 * 2. Definitions only → one struct per definition
 * 3. Both → main struct + definition structs (all in same file)
 */
function generateRustTypeFile(
  schema: LoadedSchema,
  outputDir: string,
  hasTopLevel: boolean,
  hasDefs: boolean,
): void {
  const moduleName = toSnakeCase(schema.typeName);
  const localDefinitions = hasDefs ? getDefinitions(schema) : undefined;

  // Collect all structs to generate
  const structs: Array<{
    typeName: string;
    content: string;
    commonImports: Set<string>;
    crossModuleImports: Set<string>;
  }> = [];

  // Generate main struct from top-level properties
  if (hasTopLevel) {
    const { structContent, commonImports, crossModuleImports } = generateRustStructContent(schema.typeName, schema.schemaContent, localDefinitions);
    structs.push({ typeName: schema.typeName, content: structContent, commonImports, crossModuleImports });
  }

  // Generate structs from definitions
  if (hasDefs && localDefinitions) {
    for (const [defName, defContent] of Object.entries(localDefinitions)) {
      const def = defContent as Record<string, unknown>;
      if (def.type !== 'object' || !def.properties) continue;

      const { structContent, commonImports, crossModuleImports } = generateRustStructContent(defName, def, localDefinitions);
      structs.push({ typeName: defName, content: structContent, commonImports, crossModuleImports });
    }
  }

  // Collect all imports from all structs
  const allCommonImports: Set<string> = new Set();
  const allCrossModuleImports: Set<string> = new Set();
  for (const s of structs) {
    for (const imp of s.commonImports) allCommonImports.add(imp);
    for (const imp of s.crossModuleImports) allCrossModuleImports.add(imp);
  }

  // Build file content
  let content = `//! ${schema.schemaContent.title || schema.typeName}
//!
//! ${schema.schemaContent.description || 'Generated from JSON Schema'}
//!
//! @schema_version ${schema.schemaVersion}
//! @source ${schema.fileName}

use serde::{Deserialize, Serialize};
`;

  // Add use statements for common types
  if (allCommonImports.size > 0) {
    const importsArr = [...allCommonImports].sort();
    content += `use crate::generated::common_types::{${importsArr.join(', ')}};\n`;
  }

  // Add use statements for cross-module types (e.g., Delta)
  if (allCrossModuleImports.size > 0) {
    for (const imp of [...allCrossModuleImports].sort()) {
      content += `use crate::generated::${toSnakeCase(imp)}::${imp};\n`;
    }
  }

  content += '\n';

  for (const s of structs) {
    content += s.content + '\n';
  }

  writeFile(path.join(outputDir, `${moduleName}.rs`), content);
}

/**
 * Generate a Rust struct from a schema object (top-level or definition).
 *
 * Returns the struct definition as a string plus any imports needed.
 */
function generateRustStructContent(
  typeName: string,
  schemaContent: Record<string, unknown>,
  localDefinitions?: Record<string, Record<string, unknown>>,
): {
  structContent: string;
  commonImports: Set<string>;
  crossModuleImports: Set<string>;
} {
  const properties = (schemaContent.properties || {}) as Record<string, unknown>;
  const requiredFields = (schemaContent.required || []) as string[];

  const fields: string[] = [];
  const commonImports: Set<string> = new Set();
  const crossModuleImports: Set<string> = new Set(); // For types like Delta that are in separate modules
  const inlineStructs: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    const def = propDef as Record<string, unknown>;
    const isRequired = requiredFields.includes(propName);
    const { rustType, commonImport, inlineStruct, crossModuleImport } = resolveRustTypeFull(def, propName, typeName, localDefinitions);

    if (commonImport) {
      commonImports.add(commonImport);
    }
    if (crossModuleImport) {
      crossModuleImports.add(crossModuleImport);
    }
    if (inlineStruct) {
      inlineStructs.push(inlineStruct);
    }

    // Handle nullable types (type: ["string", "null"])
    const isNullable = Array.isArray(def.type) && def.type.includes('null');
    const finalType = isNullable ? stripOuterOption(rustType) : rustType;
    const optionalWrap = isRequired ? finalType : `Option<${finalType}>`;

    // serde attributes
    const serdeSkip = !isRequired ? '#[serde(skip_serializing_if = "Option::is_none")]' : '';

    if (needsSerdeRename(propName)) {
      fields.push(`    #[serde(rename = "${propName}")]`);
    }
    if (serdeSkip) {
      fields.push(`    ${serdeSkip}`);
    }
    fields.push(`    pub ${rustFieldName(propName)}: ${optionalWrap},`);
  }

  // Build use statements
  const importsArr = [...commonImports].sort();
  const crossImportsArr = [...crossModuleImports].sort();
  let useCommon = '';
  if (importsArr.length > 0) {
    useCommon = `use crate::generated::common_types::{${importsArr.join(', ')}};\n`;
  }
  let useCross = '';
  if (crossImportsArr.length > 0) {
    useCross = crossImportsArr.map(t => `use crate::generated::${toSnakeCase(t)}::${t};\n`).join('');
  }
  const useStatements = useCommon + useCross;

  const desc = (schemaContent.description || typeName) as string;
  const result = `/// ${desc}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ${typeName} {
${fields.join('\n')}
}`;

  // Prepend inline structs (for array items that are inline objects)
  let structContent = result;
  if (inlineStructs.length > 0) {
    structContent = inlineStructs.join('\n') + '\n' + result;
  }

  return { structContent, commonImports, crossModuleImports };
}

/**
 * Fully resolve a property definition, returning the Rust type and any
 * associated metadata (common imports, inline struct definitions, cross-module imports).
 */
function resolveRustTypeFull(
  propDef: Record<string, unknown>,
  propName: string,
  parentTypeName: string,
  localDefinitions?: Record<string, Record<string, unknown>>,
): { rustType: string; commonImport?: string; inlineStruct?: string; crossModuleImport?: string } {
  const ref_ = propDef.$ref as string | undefined;
  const type = propDef.type;

  // Handle $ref
  if (ref_) {
    const defName = resolveRef(ref_);
    if (defName === 'SourceAnchor') {
      return { rustType: 'SourceAnchor', commonImport: 'SourceAnchor' };
    }
    if (defName === 'Delta') {
      return { rustType: 'Delta', crossModuleImport: 'Delta' };
    }
    if (defName && isCommonEnum(defName)) {
      return { rustType: defName, commonImport: defName };
    }
    // Check local definitions (e.g., #/definitions/AgentEntry within same schema)
    if (defName && localDefinitions && defName in localDefinitions) {
      return { rustType: defName };
    }
    if (defName && COMMON_DEFINITIONS.has(defName)) {
      return { rustType: getCommonRustType(defName) };
    }
    if (defName) {
      return { rustType: defName, crossModuleImport: defName };
    }
    return { rustType: 'serde_json::Value' };
  }

  // Handle type arrays (e.g., ["string", "null"])
  if (Array.isArray(type)) {
    const nonNullTypes = type.filter(t => t !== 'null');
    const hasNull = type.includes('null');
    if (nonNullTypes.length === 1) {
      const base = resolveSingleRustType(nonNullTypes[0], propDef, propName, parentTypeName, localDefinitions);
      return { rustType: hasNull ? stripOuterOption(base.rustType) : base.rustType, inlineStruct: base.inlineStruct };
    }
    return { rustType: 'serde_json::Value' };
  }

  return resolveSingleRustType(type as string, propDef, propName, parentTypeName, localDefinitions);
}

/**
 * Strip outer Option<...> from a Rust type string.
 */
function stripOuterOption(rustType: string): string {
  if (rustType.startsWith('Option<') && rustType.endsWith('>')) {
    return rustType.slice(7, -1);
  }
  return rustType;
}

/**
 * Resolve a single JSON Schema type to Rust.
 */
function resolveSingleRustType(
  type: string,
  propDef: Record<string, unknown>,
  propName: string,
  parentTypeName: string,
  localDefinitions?: Record<string, Record<string, unknown>>,
): { rustType: string; commonImport?: string; inlineStruct?: string; crossModuleImport?: string } {
  switch (type) {
    case 'string':
      // Inline enum — use String for now (schema-first generates enums via COMMON_DEFINITIONS)
      if (propDef.enum) {
        return { rustType: 'String' };
      }
      return { rustType: 'String' };
    case 'number':
      return { rustType: 'f64' };
    case 'integer':
      if (propName === 'schema_version') return { rustType: 'u32' };
      if (propDef.minimum === 0) return { rustType: 'u64' };
      return { rustType: 'i64' };
    case 'boolean':
      return { rustType: 'bool' };
    case 'array': {
      if (propDef.items) {
        const items = propDef.items as Record<string, unknown>;
        if (items.type === 'object' && items.properties) {
          // Complex inline object in array → generate a named struct
          const itemTypeName = inlineItemTypeName(parentTypeName, propName);
          const inlineStruct = generateInlineArrayItemStruct(
            itemTypeName,
            items,
            localDefinitions,
          );
          return { rustType: `Vec<${itemTypeName}>`, inlineStruct };
        }
        const { rustType, commonImport, crossModuleImport } = resolveRustTypeFull(items, propName, parentTypeName, localDefinitions);
        return { rustType: `Vec<${rustType}>`, commonImport, crossModuleImport };
      }
      return { rustType: 'Vec<serde_json::Value>' };
    }
    case 'object': {
      if (propDef.properties) {
        // Inline object → serde_json::Value (top-level inline objects are rare in Nexus schemas)
        return { rustType: 'serde_json::Value' };
      }
      if (propDef.additionalProperties) {
        const ap = propDef.additionalProperties as Record<string, unknown>;
        if (ap.type === 'string') {
          return { rustType: 'std::collections::HashMap<String, String>' };
        }
      }
      return { rustType: 'serde_json::Value' };
    }
    default:
      return { rustType: 'serde_json::Value' };
  }
}

/**
 * Get Rust type for a common.schema.json definition (non-enum).
 * Enum definitions are handled via isCommonEnum before this is called.
 */
function getCommonRustType(defName: string): string {
  const def = COMMON_DEFINITIONS.get(defName);
  if (!def) {
    return 'serde_json::Value';
  }
  if (def.type === 'integer') {
    return defName === 'SchemaVersion' ? 'u32' : 'u64';
  }
  return 'String';
}

/**
 * Generate a named struct for an inline array item object.
 */
function generateInlineArrayItemStruct(
  itemTypeName: string,
  objDef: Record<string, unknown>,
  localDefinitions?: Record<string, Record<string, unknown>>,
): string {
  const properties = (objDef.properties || {}) as Record<string, unknown>;
  const requiredFields = (objDef.required || []) as string[];
  const fields: string[] = [];
  const commonImports: Set<string> = new Set();

  for (const [propName, propDef] of Object.entries(properties)) {
    const def = propDef as Record<string, unknown>;
    const isRequired = requiredFields.includes(propName);
    const { rustType, commonImport } = resolveRustTypeFull(def, propName, itemTypeName, localDefinitions);

    if (commonImport) {
      commonImports.add(commonImport);
    }

    const isNullable = Array.isArray(def.type) && def.type.includes('null');
    const finalType = isNullable ? stripOuterOption(rustType) : rustType;
    const optionalWrap = isRequired ? finalType : `Option<${finalType}>`;
    const serdeSkip = !isRequired ? '#[serde(skip_serializing_if = "Option::is_none")]' : '';

    if (needsSerdeRename(propName)) {
      fields.push(`    #[serde(rename = "${propName}")]`);
    }
    if (serdeSkip) {
      fields.push(`    ${serdeSkip}`);
    }
    fields.push(`    pub ${rustFieldName(propName)}: ${optionalWrap},`);
  }

  const importsArr = [...commonImports].sort();
  let useLine = '';
  if (importsArr.length > 0) {
    useLine = `use crate::generated::common_types::{${importsArr.join(', ')}};\n`;
  }

  return `${useLine}/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ${itemTypeName} {
${fields.join('\n')}
}`;
}

/**
 * Generate common_types.rs with shared Rust types and enums.
 * Driven from COMMON_DEFINITIONS map (populated from common.schema.json).
 */
function generateRustCommonTypes(outputDir: string): void {
  const typeAliases: Array<{ name: string; rustType: string; desc: string }> = [];
  const enums: Array<{ name: string; values: string[]; desc: string }> = [];

  for (const [name, def] of COMMON_DEFINITIONS.entries()) {
    if (def.enum) {
      enums.push({
        name,
        values: def.enum,
        desc: def.description || `Enum type ${name}`,
      });
    } else {
      let rustType = 'String';
      if (def.type === 'integer') {
        rustType = name === 'SchemaVersion' ? 'u32' : 'u64';
      }
      typeAliases.push({
        name,
        rustType,
        desc: def.description || `Type alias ${name}`,
      });
    }
  }

  let content = `//! Nexus Common Types
//!
//! Shared type definitions extracted from schemas/common/common.schema.json
//!
//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
//! Generated by: pnpm run codegen

use serde::{Deserialize, Serialize};

// ── Type aliases ──────────────────────────────────────────────────────

`;

  for (const alias of typeAliases) {
    content += `/// ${alias.desc}\npub type ${alias.name} = ${alias.rustType};\n\n`;
  }

  content += '// ── Enums ─────────────────────────────────────────────────────────────\n\n';

  for (const en of enums) {
    const variants = en.values.map(v => {
      // Convert snake_case enum values to PascalCase
      return v
        .split('_')
        .map(w => w.charAt(0).toUpperCase() + w.slice(1))
        .join('');
    });
    content += `/// ${en.desc}\n#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]\n#[serde(rename_all = "snake_case")]\npub enum ${en.name} {\n${variants.map((v, i) => `${i === 0 ? '    #[default]\n' : ''}    ${v},`).join('\n')}\n}\n\n`;
  }

  content += `// ── SourceAnchor (from source-anchor.schema.json) ─────────────────────

/// Source anchor for provenance — references platform Story summary entities.
/// Source: schemas/common/source-anchor.schema.json
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SourceAnchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_summary_refs: Option<Vec<SourceSummaryRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Reference to a platform Story summary entity.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct SourceSummaryRef {
    pub story_manifest_id: String,
    pub summary_unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_kind: Option<String>,
}
`;

  writeFile(path.join(outputDir, 'common_types.rs'), content);
}
