import { LoadedSchema, COMMON_DEFINITIONS, COMMON_TYPES_MODULE_PATH } from './schema-loader';
import { resolveRef, isCommonEnum } from './schema-loader';
import { resolveFromRoot, writeFile, logger, toSnakeCase, maxSchemaVersion } from './utils';
import path from 'path';
import fs from 'fs';

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
 * Wrap code-like identifiers in a doc-comment string with backticks.
 * Clippy's `doc_markdown` lint requires code identifiers in doc comments
 * to be wrapped in backticks for proper Markdown rendering.
 *
 * Matches:
 * - PascalCase/CamelCase words (e.g. KeyBlock, SchemaVersion)
 * - snake_case words containing underscores (e.g. mark_all, schema_version)
 * - Words adjacent to parentheses like data-model-v1.md references
 *
 * Preserves existing backtick-wrapped content.
 */
function backtickDocIdentifiers(text: string): string {
  // Split on existing backtick sections to preserve them
  const parts = text.split(/(`[^`]+`)/);
  return parts.map(part => {
    // Don't modify content already inside backticks
    if (part.startsWith('`') && part.endsWith('`')) return part;
    // Wrap PascalCase/CamelCase identifiers (e.g. KeyBlock, SchemaVersion, DeltaBundle)
    // Also wrap snake_case identifiers (e.g. mark_all, schema_version)
    return part.replace(/\b([A-Z][a-zA-Z0-9]*(?:_[A-Z][a-zA-Z0-9]*)*|[a-z]+_[a-z][a-zA-Z0-9_]*)\b/g, '`$1`');
  }).join('');
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
 * Build a map from PascalCase type name → generated module path segments, for resolving
 * cross-module `use` statements. Includes common types (→ common_types module) and SourceAnchor.
 */
function buildTypeModuleMap(schemas: LoadedSchema[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const s of schemas) {
    if (s.isExplicitlySkipped) continue;
    map.set(s.typeName, s.modulePath);
    // Also register any inline definition struct names emitted in the same module file
    const defs = s.schemaContent.definitions || s.schemaContent.$defs;
    if (defs && typeof defs === 'object') {
      for (const defName of Object.keys(defs as Record<string, unknown>)) {
        if (!map.has(defName)) map.set(defName, s.modulePath);
      }
    }
  }
  // Common definitions (type aliases + enums) and SourceAnchor live in common_types
  for (const name of COMMON_DEFINITIONS.keys()) {
    map.set(name, COMMON_TYPES_MODULE_PATH);
  }
  map.set('SourceAnchor', COMMON_TYPES_MODULE_PATH);
  map.set('SourceSummaryRef', COMMON_TYPES_MODULE_PATH);
  return map;
}

/**
 * Generate Rust types from schemas.
 *
 * Emits a nested module tree mirroring the consumer-scope `schemas/` layout:
 *   generated/{common,domain,platform/{http_bff,sync},local_api/compute}/<module>.rs
 * Each folder gets a `mod.rs` declaring its children and re-exporting them; the root
 * `mod.rs` additionally re-exports every leaf type flat so `generated::TypeName` still resolves.
 */
export function generateRustTypes(schemas: LoadedSchema[]): void {
  const outputDir = resolveFromRoot('crates', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating Rust types to: ${outputDir}`);

  const typeModuleMap = buildTypeModuleMap(schemas);
  const eqEligibility = computeEqEligibility(schemas);

  // Generate common types module first (under generated/common/)
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
      generateRustEnumFile(schema, outputDir, typeModuleMap);
    } else {
      // Generate individual type files into nested folders
      generateRustTypeFile(schema, outputDir, hasTopLevel, hasDefs, typeModuleMap, eqEligibility);
    }
  }

  // Generate hierarchical mod.rs files (root + per-folder)
  generateRustModTree(schemasWithTypes, outputDir);

  // Clean stale files across the whole generated tree (recursive)
  cleanupStaleRustFiles(outputDir, schemasWithTypes);

  logger.success(`Generated Rust types for ${schemasWithTypes.length} schema(s) (+ common types)`);
}

/**
 * Walk the generated tree and remove any .rs file (or now-empty subtree) that is not part of
 * the expected output. Expected outputs: mod.rs at each folder, common_types.rs under common/,
 * and one <module>.rs per emitting schema at its nested location.
 */
function cleanupStaleRustFiles(outputDir: string, schemasWithTypes: LoadedSchema[]): void {
  // Build the allowlist of expected absolute file paths
  const keep = new Set<string>([path.join(outputDir, 'mod.rs')]);
  // common_types under common/
  keep.add(path.join(outputDir, 'common', 'mod.rs'));
  keep.add(path.join(outputDir, 'common', 'common_types.rs'));
  for (const s of schemasWithTypes) {
    const rel = [...s.modulePath.slice(0, -1), `${s.modulePath[s.modulePath.length - 1]}.rs`];
    keep.add(path.join(outputDir, ...rel));
  }
  // Every intermediate folder needs a mod.rs
  const folderSet = new Set<string>();
  folderSet.add(outputDir);
  folderSet.add(path.join(outputDir, 'common'));
  for (const s of schemasWithTypes) {
    let acc = outputDir;
    for (const seg of s.modulePath.slice(0, -1)) {
      acc = path.join(acc, seg);
      folderSet.add(acc);
    }
  }
  for (const f of folderSet) keep.add(path.join(f, 'mod.rs'));

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
      } else if (ent.isFile() && ent.name.endsWith('.rs') && !keep.has(full)) {
        fs.unlinkSync(full);
        logger.warn(`Removed stale generated file: ${path.relative(root, full)}`);
        removed++;
      }
    }
  };
  walk(outputDir);
  if (removed > 0) logger.info(`Stale Rust file cleanup removed ${removed} file(s)`);
}

/**
 * Generate the hierarchical mod.rs tree: root mod.rs + one mod.rs per folder.
 * Root mod.rs declares the top-level consumer-scope submodules and re-exports all leaf types flat.
 */
function generateRustModTree(schemas: LoadedSchema[], outputDir: string): void {
  // Group schemas by their folder path (modulePath minus the leaf segment)
  // Build the folder tree structure.
  type FolderNode = { children: Map<string, FolderNode>; schemas: LoadedSchema[] };
  const root: FolderNode = { children: new Map(), schemas: [] };

  const getFolder = (segs: string[]): FolderNode => {
    let node = root;
    for (const seg of segs) {
      let child = node.children.get(seg);
      if (!child) {
        child = { children: new Map(), schemas: [] };
        node.children.set(seg, child);
      }
      node = child;
    }
    return node;
  };

  for (const s of schemas) {
    const folderSegs = s.modulePath.slice(0, -1);
    const folder = getFolder(folderSegs);
    folder.schemas.push(s);
  }

  // Recursively write mod.rs for each folder.
  // `relSegs` is the path segments from outputDir to this folder (e.g. ['platform', 'http_bff']).
  const writeFolderMod = (node: FolderNode, relSegs: string[]) => {
    const folderDir = path.join(outputDir, ...relSegs);
    const lines: string[] = [];

    // Header
    if (relSegs.length === 0) {
      lines.push('//! Nexus Wire Contracts - Generated Rust Types');
      lines.push('//!');
      lines.push('//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY');
      lines.push('//! Source: schemas/*.schema.json');
      lines.push('//! Generated by: `pnpm run codegen`');
      lines.push('');
    }

    // Declare child modules
    const childNames = [...node.children.keys()].sort();
    for (const child of childNames) {
      lines.push(`pub mod ${child};`);
    }

    // Special: root declares common_types is under common/, but root itself has the synthetic
    // common_types only via the common/ subtree. Handle the common/ folder's common_types module.
    // For the common/ folder, also declare common_types module.
    if (relSegs.length === 1 && relSegs[0] === 'common') {
      // common_types.rs lives here but is not in schemas list; declare it explicitly
      if (!lines.includes('pub mod common_types;')) {
        // Insert common_types declaration (keep sorted-ish: common_types before version_ref)
        lines.splice(childNames.length === 0 ? lines.length : 0, 0, 'pub mod common_types;');
      }
    }

    // Declare leaf schema modules in this folder
    const leafModules = node.schemas.map(s => s.modulePath[s.modulePath.length - 1]).sort();
    for (const leaf of leafModules) {
      lines.push(`pub mod ${leaf};`);
    }

    lines.push('');

    // Re-exports: re-export each declared child module + leaf module contents
    for (const child of childNames) {
      lines.push(`pub use ${child}::*;`);
    }
    if (relSegs.length === 1 && relSegs[0] === 'common') {
      lines.push('pub use common_types::*;');
    }
    for (const leaf of leafModules) {
      lines.push(`pub use ${leaf}::*;`);
    }

    // Root-only: schema version constants
    if (relSegs.length === 0) {
      lines.push('');
      lines.push('/// Schema version constants');
      lines.push('pub const SCHEMA_VERSIONS: &[(&str, u32)] = &[');
      for (const s of schemas) {
        lines.push(`    ("${s.typeName}", ${s.schemaVersion}),`);
      }
      lines.push('];');
      lines.push('');
      lines.push(`/// Highest \`schema_version\` among emitted contract schemas`);
      lines.push(`pub const LATEST_SCHEMA_VERSION: u32 = ${maxSchemaVersion(schemas.map(s => s.schemaVersion))};`);
    }

    writeFile(path.join(folderDir, 'mod.rs'), lines.join('\n') + '\n');

    // Recurse into children
    for (const [childName, childNode] of node.children) {
      writeFolderMod(childNode, [...relSegs, childName]);
    }
  };

  writeFolderMod(root, []);
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
function generateRustEnumFile(schema: LoadedSchema, outputDir: string, _typeModuleMap: Map<string, string[]>): void {
  const values = schema.schemaContent.enum as string[];

  const variants = values.map((v, idx) => {
    const pascal = snakeToPascal(v);
    // Build doc comment from enumDescriptions if available
    const descriptions = schema.schemaContent.enumDescriptions as Record<string, string> | undefined;
    const desc = descriptions?.[v];
    const docComment = desc ? `    /// ${backtickDocIdentifiers(desc)}\n` : '';
    const defaultAttr = idx === 0 ? '    #[default]\n' : '';
    return `${docComment}${defaultAttr}    #[serde(rename = "${v}")]\n    ${pascal},`;
  });

  const content = `//! ${backtickDocIdentifiers(String(schema.schemaContent.title || schema.typeName))}
//!
//! ${backtickDocIdentifiers(String(schema.schemaContent.description || 'Generated from JSON Schema'))}
//!
//! \`@schema_version\` ${schema.schemaVersion}
//! \`@source\` ${schema.fileName}

use serde::{Deserialize, Serialize};

/// ${backtickDocIdentifiers(String(schema.schemaContent.description || schema.typeName))}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ${schema.typeName} {
${variants.join('\n')}
}
`;

  const leafFile = path.join(outputDir, ...schema.modulePath.slice(0, -1), `${schema.modulePath[schema.modulePath.length - 1]}.rs`);
  writeFile(leafFile, content);
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
  typeModuleMap: Map<string, string[]>,
  eqEligibility: Map<string, boolean>,
): void {
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
    const { structContent, commonImports, crossModuleImports } = generateRustStructContent(schema.typeName, schema.schemaContent, localDefinitions, eqEligibility);
    structs.push({ typeName: schema.typeName, content: structContent, commonImports, crossModuleImports });
  }

  // Generate structs from definitions
  if (hasDefs && localDefinitions) {
    for (const [defName, defContent] of Object.entries(localDefinitions)) {
      const def = defContent as Record<string, unknown>;
      if (def.type !== 'object' || !def.properties) continue;

      const { structContent, commonImports, crossModuleImports } = generateRustStructContent(defName, def, localDefinitions, eqEligibility);
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
  let content = `//! ${backtickDocIdentifiers(String(schema.schemaContent.title || schema.typeName))}
//!
//! ${backtickDocIdentifiers(String(schema.schemaContent.description || 'Generated from JSON Schema'))}
//!
//! \`@schema_version\` ${schema.schemaVersion}
//! \`@source\` ${schema.fileName}

use serde::{Deserialize, Serialize};
`;

  // Add use statements for common types (live in generated::common::common_types)
  if (allCommonImports.size > 0) {
    const importsArr = [...allCommonImports].sort();
    content += `use crate::generated::common::common_types::{${importsArr.join(', ')}};\n`;
  }

  // Add use statements for cross-module types (resolve each type's nested module path)
  if (allCrossModuleImports.size > 0) {
    for (const imp of [...allCrossModuleImports].sort()) {
      const modPath = typeModuleMap.get(imp);
      if (!modPath) {
        logger.warn(`Cross-module import "${imp}" in ${schema.fileName} has no known module path; emitting crate::generated root glob`);
        content += `use crate::generated::${imp};\n`;
      } else {
        content += `use crate::generated::${modPath.join('::')}::${imp};\n`;
      }
    }
  }

  content += '\n';

  for (const s of structs) {
    content += s.content + '\n';
  }

  const leafFile = path.join(outputDir, ...schema.modulePath.slice(0, -1), `${schema.modulePath[schema.modulePath.length - 1]}.rs`);
  writeFile(leafFile, content);
}

/**
 * Check whether a schema fragment contains a JSON Schema `number` type anywhere
 * in its shape. Used to decide whether a generated Rust struct can safely derive
 * `Eq` (Rust `f64` does not implement `Eq`).
 *
 * Only follows local structure (properties / items / inline object arrays).
 * `$ref` targets are assumed non-floating unless resolved in a future pass.
 */
function schemaFragmentHasF64(fragment: Record<string, unknown>): boolean {
  const type = fragment.type;
  if (type === 'number') return true;
  if (Array.isArray(type) && type.includes('number')) return true;
  if (type === 'array' && fragment.items) {
    return schemaFragmentHasF64(fragment.items as Record<string, unknown>);
  }
  if (type === 'object' && fragment.properties) {
    for (const prop of Object.values(fragment.properties as Record<string, unknown>)) {
      if (schemaFragmentHasF64(prop as Record<string, unknown>)) return true;
    }
  }
  return false;
}

/**
 * Compute Eq-derivability for every named struct fragment (main struct + object
 * definitions) across all schemas. A fragment cannot derive `Eq` if it contains a
 * JSON Schema `number` type or references (via `$ref`) another fragment that cannot
 * derive `Eq`. Computed with a fixed-point iteration so transitive references are
 * resolved.
 */
function computeEqEligibility(schemas: LoadedSchema[]): Map<string, boolean> {
  const eligible = new Map<string, boolean>();

  const fragmentNames = (schema: LoadedSchema): string[] => {
    const names: string[] = [];
    if (!schema.isDefinitionsOnly && !schema.isStandaloneEnum && schema.schemaContent.type === 'object') {
      names.push(schema.typeName);
    }
    const defs = getDefinitions(schema);
    if (defs) {
      for (const [defName, def] of Object.entries(defs)) {
        if ((def as Record<string, unknown>).type === 'object') {
          names.push(defName);
        }
      }
    }
    return names;
  };

  // Initialize all named fragments as eligible.
  for (const schema of schemas) {
    if (schema.isExplicitlySkipped) continue;
    for (const name of fragmentNames(schema)) {
      eligible.set(name, true);
    }
  }

  const propertyCanDeriveEq = (
    propDef: Record<string, unknown>,
    eligibilityMap: Map<string, boolean>,
  ): boolean => {
    const type = propDef.type;
    if (type === 'number') return false;
    if (Array.isArray(type) && type.includes('number')) return false;
    if (type === 'array' && propDef.items) {
      return propertyCanDeriveEq(propDef.items as Record<string, unknown>, eligibilityMap);
    }
    if (type === 'object' && propDef.properties) {
      for (const prop of Object.values(propDef.properties as Record<string, unknown>)) {
        if (!propertyCanDeriveEq(prop as Record<string, unknown>, eligibilityMap)) return false;
      }
      return true;
    }
    const ref = propDef.$ref as string | undefined;
    if (ref) {
      const defName = resolveRef(ref);
      if (defName && eligibilityMap.has(defName)) {
        return eligibilityMap.get(defName) ?? true;
      }
      // Unknown / common type refs assumed Eq-safe.
      return true;
    }
    return true;
  };

  const fragmentCanDeriveEq = (
    content: Record<string, unknown>,
    eligibilityMap: Map<string, boolean>,
  ): boolean => {
    const props = (content.properties || {}) as Record<string, unknown>;
    for (const prop of Object.values(props)) {
      if (!propertyCanDeriveEq(prop as Record<string, unknown>, eligibilityMap)) return false;
    }
    return true;
  };

  let changed = true;
  while (changed) {
    changed = false;
    for (const schema of schemas) {
      if (schema.isExplicitlySkipped || schema.isStandaloneEnum) continue;

      const fragments: Array<{ name: string; content: Record<string, unknown> }> = [];
      if (!schema.isDefinitionsOnly && schema.schemaContent.type === 'object') {
        fragments.push({ name: schema.typeName, content: schema.schemaContent });
      }
      const defs = getDefinitions(schema);
      if (defs) {
        for (const [defName, def] of Object.entries(defs)) {
          const defContent = def as Record<string, unknown>;
          if (defContent.type === 'object') {
            fragments.push({ name: defName, content: defContent });
          }
        }
      }

      for (const { name, content } of fragments) {
        if (eligible.get(name) === false) continue;
        if (!fragmentCanDeriveEq(content, eligible)) {
          eligible.set(name, false);
          changed = true;
        }
      }
    }
  }

  return eligible;
}
/**
 * Generate a Rust struct from a schema object (top-level or definition).
 *
 * Returns the struct definition as a string plus any imports needed, and a
 * boolean indicating whether the struct can safely derive `Eq`.
 */
function generateRustStructContent(
  typeName: string,
  schemaContent: Record<string, unknown>,
  localDefinitions: Record<string, Record<string, unknown>> | undefined,
  eqEligibility: Map<string, boolean>,
): {
  structContent: string;
  commonImports: Set<string>;
  crossModuleImports: Set<string>;
  canDeriveEq: boolean;
} {
  const properties = (schemaContent.properties || {}) as Record<string, unknown>;
  const requiredFields = (schemaContent.required || []) as string[];

  // Determine Eq-safety: any `number` type in the local shape, any inline array
  // item struct containing `number`, or a global eligibility pass that follows
  // cross-schema `$ref`s can all disable `Eq`.
  const directHasF64 = Object.values(properties).some(schemaFragmentHasF64);

  const fields: string[] = [];
  const commonImports: Set<string> = new Set();
  const crossModuleImports: Set<string> = new Set(); // For types like Delta that are in separate modules
  const inlineStructs: string[] = [];
  let inlineHasF64 = false;

  for (const [propName, propDef] of Object.entries(properties)) {
    const def = propDef as Record<string, unknown>;
    const isRequired = requiredFields.includes(propName);
    const { rustType, commonImport, inlineStruct, crossModuleImport, inlineStructHasF64 } =
      resolveRustTypeFull(def, propName, typeName, localDefinitions);

    if (inlineStructHasF64) {
      inlineHasF64 = true;
    }
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

  const globallyEligible = eqEligibility.get(typeName) ?? true;
  const canDeriveEq = globallyEligible && !directHasF64 && !inlineHasF64;
  const deriveTraits = canDeriveEq
    ? 'Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq'
    : 'Debug, Clone, Default, Serialize, Deserialize, PartialEq';
  const desc = (schemaContent.description || typeName) as string;
  const result = `/// ${backtickDocIdentifiers(desc)}
#[derive(${deriveTraits})]
#[serde(rename_all = "snake_case")]
pub struct ${typeName} {
${fields.join('\n')}
}`;

  // Prepend inline structs (for array items that are inline objects)
  let structContent = result;
  if (inlineStructs.length > 0) {
    structContent = inlineStructs.join('\n') + '\n' + result;
  }

  return { structContent, commonImports, crossModuleImports, canDeriveEq };
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
): { rustType: string; commonImport?: string; inlineStruct?: string; crossModuleImport?: string; inlineStructHasF64?: boolean } {
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
      return {
        rustType: hasNull ? stripOuterOption(base.rustType) : base.rustType,
        inlineStruct: base.inlineStruct,
        inlineStructHasF64: base.inlineStructHasF64,
      };
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
): { rustType: string; commonImport?: string; inlineStruct?: string; crossModuleImport?: string; inlineStructHasF64?: boolean } {
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
          const { content: inlineStruct, hasF64 } = generateInlineArrayItemStruct(
            itemTypeName,
            items,
            localDefinitions,
          );
          return { rustType: `Vec<${itemTypeName}>`, inlineStruct, inlineStructHasF64: hasF64 };
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
): { content: string; hasF64: boolean } {
  const properties = (objDef.properties || {}) as Record<string, unknown>;
  const requiredFields = (objDef.required || []) as string[];
  const fields: string[] = [];
  const commonImports: Set<string> = new Set();
  let inlineHasF64 = false;

  for (const [propName, propDef] of Object.entries(properties)) {
    const def = propDef as Record<string, unknown>;
    const isRequired = requiredFields.includes(propName);
    const { rustType, commonImport, inlineStructHasF64 } = resolveRustTypeFull(def, propName, itemTypeName, localDefinitions);

    if (inlineStructHasF64) {
      inlineHasF64 = true;
    }
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
    useLine = `use crate::generated::common::common_types::{${importsArr.join(', ')}};\n`;
  }

  const hasF64 = inlineHasF64 || schemaFragmentHasF64(objDef);
  const deriveTraits = hasF64
    ? 'Debug, Clone, Default, Serialize, Deserialize, PartialEq'
    : 'Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq';

  const content = `${useLine}/// Inline array item type (auto-generated from schema)
#[derive(${deriveTraits})]
#[serde(rename_all = "snake_case")]
pub struct ${itemTypeName} {
${fields.join('\n')}
}`;
  return { content, hasF64 };
}

/**
 * Generate common_types.rs with shared Rust types and enums.
 * Driven from COMMON_DEFINITIONS map (populated from common.schema.json).
 *
 * Emitted at `generated/common/common_types.rs` to mirror `schemas/common/common.schema.json`.
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
//! Shared type definitions extracted from \`schemas/common/common.schema.json\`
//!
//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
//! Generated by: \`pnpm run codegen\`

use serde::{Deserialize, Serialize};

// ── Type aliases ──────────────────────────────────────────────────────

`;

  for (const alias of typeAliases) {
    content += `/// ${backtickDocIdentifiers(alias.desc)}\npub type ${alias.name} = ${alias.rustType};\n\n`;
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
    content += `/// ${backtickDocIdentifiers(en.desc)}\n#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]\n#[serde(rename_all = "snake_case")]\npub enum ${en.name} {\n${variants.map((v, i) => `${i === 0 ? '    #[default]\n' : ''}    ${v},`).join('\n')}\n}\n\n`;
  }

  content += `// ── SourceAnchor (from source-anchor.schema.json) ─────────────────────

/// Source anchor for provenance — references platform Story summary entities.
/// Source: schemas/common/source-anchor.schema.json
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct SourceSummaryRef {
    pub story_manifest_id: String,
    pub summary_unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_kind: Option<String>,
}
`;

  writeFile(path.join(outputDir, 'common', 'common_types.rs'), content);
}
