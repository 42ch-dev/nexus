# Codegen Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the schema-to-code generation pipeline that transforms JSON Schema files into TypeScript types (npm package) and Rust types (crate), establishing the single truth source workflow for Nexus wire contracts.

**Architecture:** Single codegen pass from `schemas/*.json` → TypeScript (`packages/nexus-contracts/src/generated/`) + Rust (`crates/nexus-contracts/src/generated/`). Uses `quicktype` for multi-language generation with custom templates for Nexus-specific requirements (schema_version, reference handling, etc.).

**Tech Stack:** Node.js 20+, quicktype-core, TypeScript 5.3+, Rust 1.75+, custom codegen templates

---

## Files to Create/Modify

**Create:**
- `tooling/codegen/package.json` (codegen tool manifest)
- `tooling/codegen/tsconfig.json` (codegen TS config)
- `tooling/codegen/src/index.ts` (main codegen orchestrator)
- `tooling/codegen/src/schema-loader.ts` (schema file loader)
- `tooling/codegen/src/ts-generator.ts` (TS type generator)
- `tooling/codegen/src/rust-generator.ts` (Rust type generator)
- `tooling/codegen/src/utils.ts` (shared utilities)
- `packages/nexus-contracts/src/generated/.gitkeep` (generated types placeholder)
- `crates/nexus-contracts/src/generated/.gitkeep` (generated types placeholder)

**Modify:**
- `package.json` (root) - add codegen script
- `.github/workflows/ci.yml` - add codegen verification step

---

## Task 1: Initialize Codegen Tooling Directory

**Files:**
- Create: `tooling/codegen/package.json`
- Create: `tooling/codegen/tsconfig.json`

- [ ] **Step 1: Create codegen directory structure**

Run: `mkdir -p tooling/codegen/src`

Expected: Directory created

- [ ] **Step 2: Create codegen package.json**

Create file: `tooling/codegen/package.json`

```json
{
  "name": "nexus-codegen",
  "version": "0.1.0",
  "private": true,
  "description": "Nexus schema-to-code generation pipeline (TS + Rust)",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": {
    "build": "tsup src/index.ts --format cjs,esm --dts",
    "dev": "tsup src/index.ts --format cjs,esm --dts --watch",
    "codegen": "node dist/index.js",
    "typecheck": "tsc --noEmit",
    "clean": "rm -rf dist"
  },
  "dependencies": {
    "quicktype-core": "^23.0.0",
    "glob": "^10.3.0",
    "chalk": "^4.1.2"
  },
  "devDependencies": {
    "typescript": "^5.3.0",
    "tsup": "^8.0.0",
    "@types/node": "^20.10.0"
  }
}
```

Expected: Package manifest created

- [ ] **Step 3: Create codegen tsconfig.json**

Create file: `tooling/codegen/tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "lib": ["ES2022"],
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

Expected: TypeScript config created

- [ ] **Step 4: Install codegen dependencies**

Run: `cd tooling/codegen && npm install`

Expected: Dependencies installed

- [ ] **Step 5: Commit codegen tooling setup**

Run: `git add tooling/codegen && git commit -m "feat(tooling): initialize codegen tooling structure"`

Expected: Commit successful

---

## Task 2: Implement Schema Loader

**Files:**
- Create: `tooling/codegen/src/schema-loader.ts`
- Create: `tooling/codegen/src/utils.ts`

- [ ] **Step 1: Create utilities module**

Create file: `tooling/codegen/src/utils.ts`

```typescript
import path from 'path';
import fs from 'fs';

/**
 * Resolve path relative to project root
 */
export function resolveFromRoot(...segments: string[]): string {
  const root = path.join(__dirname, '..', '..', '..');
  return path.join(root, ...segments);
}

/**
 * Ensure directory exists, create if not
 */
export function ensureDir(dirPath: string): void {
  if (!fs.existsSync(dirPath)) {
    fs.mkdirSync(dirPath, { recursive: true });
  }
}

/**
 * Write file with prettier formatting (placeholder - will add prettier later)
 */
export function writeFile(filePath: string, content: string): void {
  ensureDir(path.dirname(filePath));
  fs.writeFileSync(filePath, content, 'utf8');
}

/**
 * Read JSON file
 */
export function readJSON<T>(filePath: string): T {
  const content = fs.readFileSync(filePath, 'utf8');
  return JSON.parse(content);
}

/**
 * Extract schema version from schema object
 */
export function extractSchemaVersion(schema: any): string {
  return schema.schema_version || '0.0.0';
}

/**
 * Convert schema file name to type name
 * Example: "bundle.schema.json" -> "Bundle"
 */
export function schemaToTypeName(fileName: string): string {
  return fileName
    .replace('.schema.json', '')
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join('');
}

/**
 * Logger utility
 */
export const logger = {
  info: (msg: string) => console.log(`[INFO] ${msg}`),
  success: (msg: string) => console.log(`[OK] ${msg}`),
  error: (msg: string) => console.error(`[ERR] ${msg}`),
};
```

Expected: Utilities module created

- [ ] **Step 2: Create schema loader module**

Create file: `tooling/codegen/src/schema-loader.ts`

```typescript
import glob from 'glob';
import { readJSON, resolveFromRoot, extractSchemaVersion, schemaToTypeName, logger } from './utils';
import path from 'path';

export interface LoadedSchema {
  filePath: string;
  fileName: string;
  typeName: string;
  schemaVersion: string;
  schemaContent: any;
}

/**
 * Load all schema files from schemas/ directory
 */
export function loadAllSchemas(): LoadedSchema[] {
  const schemasDir = resolveFromRoot('schemas');
  logger.info(`Loading schemas from: ${schemasDir}`);

  // Find all .schema.json files (excluding meta/README)
  const pattern = path.join(schemasDir, '**/*.schema.json');
  const files = glob.sync(pattern);

  if (files.length === 0) {
    logger.error('No schema files found');
    return [];
  }

  logger.info(`Found ${files.length} schema files`);

  const loadedSchemas: LoadedSchema[] = [];

  for (const filePath of files) {
    const fileName = path.basename(filePath);
    const typeName = schemaToTypeName(fileName);
    const schemaContent = readJSON<any>(filePath);
    const schemaVersion = extractSchemaVersion(schemaContent);

    loadedSchemas.push({
      filePath,
      fileName,
      typeName,
      schemaVersion,
      schemaContent,
    });

    logger.info(`  Loaded: ${fileName} -> ${typeName} (v${schemaVersion})`);
  }

  return loadedSchemas;
}

/**
 * Group schemas by version
 */
export function groupSchemasByVersion(schemas: LoadedSchema[]): Map<string, LoadedSchema[]> {
  const versionMap = new Map<string, LoadedSchema[]>();

  for (const schema of schemas) {
    const version = schema.schemaVersion;
    if (!versionMap.has(version)) {
      versionMap.set(version, []);
    }
    versionMap.get(version)!.push(schema);
  }

  return versionMap;
}

/**
 * Validate schema structure (basic validation)
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
```

Expected: Schema loader created

- [ ] **Step 3: Commit schema loader**

Run: `git add tooling/codegen/src/utils.ts tooling/codegen/src/schema-loader.ts && git commit -m "feat(codegen): implement schema loader with utilities"`

Expected: Commit successful

---

## Task 3: Implement TypeScript Generator

**Files:**
- Create: `tooling/codegen/src/ts-generator.ts`
- Create: `packages/nexus-contracts/src/generated/.gitkeep`

- [ ] **Step 1: Create generated directory placeholder**

Run: `mkdir -p packages/nexus-contracts/src/generated && touch packages/nexus-contracts/src/generated/.gitkeep`

Expected: Placeholder created

- [ ] **Step 2: Create TypeScript generator module**

Create file: `tooling/codegen/src/ts-generator.ts`

```typescript
import { LoadedSchema, groupSchemasByVersion } from './schema-loader';
import { resolveFromRoot, writeFile, logger } from './utils';
import path from 'path';

/**
 * Generate TypeScript types from schemas
 */
export function generateTSTypes(schemas: LoadedSchema[]): void {
  const outputDir = resolveFromRoot('packages', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating TypeScript types to: ${outputDir}`);

  // Group schemas by version
  const versionMap = groupSchemasByVersion(schemas);

  // Generate index.ts with exports
  generateTSIndex(schemas, outputDir);

  // Generate individual type files
  for (const schema of schemas) {
    generateTSTypeFile(schema, outputDir);
  }

  // Generate common types file
  generateCommonTypesFile(outputDir);

  logger.success(`Generated TypeScript types for ${schemas.length} schemas`);
}

/**
 * Generate index.ts with exports
 */
function generateTSIndex(schemas: LoadedSchema[], outputDir: string): void {
  const imports: string[] = [];
  const exports: string[] = [];

  for (const schema of schemas) {
    const typeName = schema.typeName;
    imports.push(`import { ${typeName} } from './${typeName}';`);
    exports.push(typeName);
  }

  const content = `/**
 * Nexus Wire Contracts - Generated TypeScript Types
 *
 * AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
 * Source: schemas/*.schema.json
 */

import { CommonTypes, SchemaVersion, Timestamp, UUID, WorldRef, CreatorRef, KBRef, DeltaSequence, ManuscriptPhase, ManuscriptState, TimePolicy, Visibility } from './CommonTypes';

${imports.join('\n')}

// Common types
export * from './CommonTypes';

// Schema version constants
export const SCHEMA_VERSIONS = {
${schemas.map(s => `  ${s.typeName}: '${s.schemaVersion}',`).join('\n')}
};

// Re-export all generated types
export {
${exports.map(e => `  ${e},`).join('\n')}
};

// Version map
export const LATEST_SCHEMA_VERSION = '${schemas[0]?.schemaVersion || '0.0.0'}';
`;

  writeFile(path.join(outputDir, 'index.ts'), content);
}

/**
 * Generate individual type file for a schema
 */
function generateTSTypeFile(schema: LoadedSchema, outputDir: string): void {
  const typeName = schema.typeName;
  const schemaContent = schema.schemaContent;

  // Extract properties and required fields
  const properties = schemaContent.properties || {};
  const requiredFields = schemaContent.required || [];

  // Generate type interface
  const interfaceContent = generateTSInterface(typeName, properties, requiredFields, schemaContent);

  writeFile(path.join(outputDir, `${typeName}.ts`), interfaceContent);
}

/**
 * Generate TypeScript interface from JSON Schema properties
 */
function generateTSInterface(
  typeName: string,
  properties: any,
  requiredFields: string[],
  schemaContent: any
): string {
  const fields: string[] = [];
  const imports: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    const isRequired = requiredFields.includes(propName);
    const fieldType = mapJSONSchemaToTS(propDef, propName, imports);
    const optionalMark = isRequired ? '' : '?';

    fields.push(`  ${propName}${optionalMark}: ${fieldType};`);
  }

  const importBlock = imports.length > 0 ? `\nimport { ${imports.join(', ')} } from './CommonTypes';` : '';

  return `/**
 * ${schemaContent.title || typeName}
 *
 * ${schemaContent.description || 'Generated from JSON Schema'}
 *
 * @schema_version ${schema.schemaVersion}
 * @source ${schema.fileName}
 */

${importBlock}

export interface ${typeName} {
${fields.join('\n')}
}

// Default factory function
export function createDefault${typeName}(): Partial<${typeName}> {
  return {
    schema_version: '${schema.schemaVersion}',
${generateDefaultValues(properties, requiredFields).join('\n')}
  };
}
`;
}

/**
 * Map JSON Schema type to TypeScript type
 */
function mapJSONSchemaToTS(propDef: any, propName: string, imports: string[]): string {
  const type = propDef.type;
  const ref = propDef.$ref;

  // Handle $ref
  if (ref) {
    // Extract type name from $ref
    const refMatch = ref.match(/#\/definitions\/(\w+)$/);
    if (refMatch) {
      const refType = refMatch[1];
      imports.push(refType);
      return refType;
    }
    // Handle full URI refs (will resolve to external import)
    return 'any'; // Placeholder for now
  }

  // Handle basic types
  switch (type) {
    case 'string':
      if (propDef.enum) {
        return propDef.enum.map((v: string) => `'${v}'`).join(' | ');
      }
      if (propDef.format === 'date-time') return 'string'; // Timestamp
      return 'string';
    case 'number':
    case 'integer':
      return 'number';
    case 'boolean':
      return 'boolean';
    case 'array':
      if (propDef.items) {
        const itemType = mapJSONSchemaToTS(propDef.items, propName, imports);
        return `${itemType}[]`;
      }
      return 'any[]';
    case 'object':
      if (propDef.properties) {
        // Inline object type
        const inlineFields: string[] = [];
        const inlineRequired = propDef.required || [];
        for (const [k, v] of Object.entries(propDef.properties)) {
          const isReq = inlineRequired.includes(k);
          const ft = mapJSONSchemaToTS(v as any, k, imports);
          inlineFields.push(`${k}${isReq ? '' : '?'}: ${ft}`);
        }
        return `{ ${inlineFields.join('; ')} }`;
      }
      return 'Record<string, any>';
    default:
      return 'any';
  }
}

/**
 * Generate default values for required fields
 */
function generateDefaultValues(properties: any, requiredFields: string[]): string[] {
  const defaults: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    if (propName === 'schema_version') continue; // Already handled

    const isRequired = requiredFields.includes(propName);
    if (!isRequired) continue;

    const defaultValue = getDefaultValue(propDef as any);
    if (defaultValue !== null) {
      defaults.push(`    ${propName}: ${defaultValue},`);
    }
  }

  return defaults;
}

/**
 * Get default value for a property type
 */
function getDefaultValue(propDef: any): string | null {
  if (propDef.default !== undefined) {
    return JSON.stringify(propDef.default);
  }

  switch (propDef.type) {
    case 'string':
      if (propDef.enum) return `'${propDef.enum[0]}'`;
      return "''";
    case 'number':
    case 'integer':
      return '0';
    case 'boolean':
      return 'false';
    case 'array':
      return '[]';
    case 'object':
      return '{}';
    default:
      return null;
  }
}

/**
 * Generate CommonTypes.ts with shared type definitions
 */
function generateCommonTypesFile(outputDir: string): void {
  const content = `/**
 * Common Types
 *
 * Shared type definitions imported by all generated types.
 * Based on schemas/common/common.schema.json
 */

export type SchemaVersion = string;
export type Timestamp = string;
export type UUID = string;

export type WorldRef = string;
export type CreatorRef = string;
export type StoryRef = string;
export type KBRef = string;
export type DeltaSequence = number;

export type ManuscriptPhase = 'brainstorm' | 'write' | 'review' | 'provisional' | 'canon';
export type ManuscriptState = 'draft' | 'proposed' | 'confirmed' | 'published';
export type TimePolicy = 'linear' | 'branching' | 'mergeable';
export type Visibility = 'private' | 'shared' | 'public';

export interface CommonTypes {
  schema_version: SchemaVersion;
}
`;

  writeFile(path.join(outputDir, 'CommonTypes.ts'), content);
}
```

Expected: TypeScript generator created

- [ ] **Step 3: Commit TypeScript generator**

Run: `git add tooling/codegen/src/ts-generator.ts packages/nexus-contracts/src/generated && git commit -m "feat(codegen): implement TypeScript type generator"`

Expected: Commit successful

---

## Task 4: Implement Rust Generator

**Files:**
- Create: `tooling/codegen/src/rust-generator.ts`
- Create: `crates/nexus-contracts/src/generated/.gitkeep`

- [ ] **Step 1: Create generated directory placeholder**

Run: `mkdir -p crates/nexus-contracts/src/generated && touch crates/nexus-contracts/src/generated/.gitkeep`

Expected: Placeholder created

- [ ] **Step 2: Create Rust generator module**

Create file: `tooling/codegen/src/rust-generator.ts`

```typescript
import { LoadedSchema, groupSchemasByVersion } from './schema-loader';
import { resolveFromRoot, writeFile, logger } from './utils';
import path from 'path';

/**
 * Generate Rust types from schemas
 */
export function generateRustTypes(schemas: LoadedSchema[]): void {
  const outputDir = resolveFromRoot('crates', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating Rust types to: ${outputDir}`);

  // Generate mod.rs with module declarations
  generateRustMod(schemas, outputDir);

  // Generate individual type files
  for (const schema of schemas) {
    generateRustTypeFile(schema, outputDir);
  }

  // Generate common types module
  generateRustCommonTypes(outputDir);

  logger.success(`Generated Rust types for ${schemas.length} schemas`);
}

/**
 * Generate mod.rs with module declarations
 */
function generateRustMod(schemas: LoadedSchema[], outputDir: string): void {
  const modules: string[] = ['pub mod common_types;'];

  for (const schema of schemas) {
    const moduleName = schema.typeName.toLowerCase();
    modules.push(`pub mod ${moduleName};`);
  }

  const content = `//! Nexus Wire Contracts - Generated Rust Types
//!
//! AUTO-GENERATED FROM JSON SCHEMA - DO NOT MODIFY MANUALLY
//! Source: schemas/*.schema.json

${modules.join('\n')}

// Re-export all types
pub use common_types::*;

${schemas.map(s => `pub use ${s.typeName.toLowerCase()}::*;`).join('\n')}

/// Schema version constants
pub const SCHEMA_VERSIONS: &[(&str, &str)] = &[
${schemas.map(s => `    ("${s.typeName}", "${s.schemaVersion}"),`).join('\n')}
];

/// Latest schema version
pub const LATEST_SCHEMA_VERSION: &str = "${schemas[0]?.schemaVersion || '0.0.0'}";
`;

  writeFile(path.join(outputDir, 'mod.rs'), content);
}

/**
 * Generate individual type file for a schema
 */
function generateRustTypeFile(schema: LoadedSchema, outputDir: string): void {
  const typeName = schema.typeName;
  const moduleName = typeName.toLowerCase();
  const schemaContent = schema.schemaContent;

  // Extract properties and required fields
  const properties = schemaContent.properties || {};
  const requiredFields = schemaContent.required || [];

  // Generate Rust struct
  const structContent = generateRustStruct(typeName, properties, requiredFields, schemaContent);

  writeFile(path.join(outputDir, `${moduleName}.rs`), structContent);
}

/**
 * Generate Rust struct from JSON Schema properties
 */
function generateRustStruct(
  typeName: string,
  properties: any,
  requiredFields: string[],
  schemaContent: any
): string {
  const fields: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    const isRequired = requiredFields.includes(propName);
    const fieldType = mapJSONSchemaToRust(propDef);
    const serdeAttr = isRequired ? '' : '#[serde(skip_serializing_if = "Option::is_none")]';
    const optionalWrap = isRequired ? fieldType : `Option<${fieldType}>`;

    if (serdeAttr) {
      fields.push(`    ${serdeAttr}`);
    }
    fields.push(`    pub ${propName}: ${optionalWrap},`);
  }

  return `//! ${schemaContent.title || typeName}
//!
//! ${schemaContent.description || 'Generated from JSON Schema'}
//!
//! @schema_version ${schema.schemaVersion}
//! @source ${schema.fileName}

use serde::{Deserialize, Serialize};
use crate::generated::common_types::*;

/// ${schemaContent.title || typeName}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ${typeName} {
${fields.join('\n')}
}

impl Default for ${typeName} {
    fn default() -> Self {
        Self {
            schema_version: "${schema.schemaVersion}".to_string(),
${generateRustDefaults(properties, requiredFields).join('\n')}
        }
    }
}
`;
}

/**
 * Map JSON Schema type to Rust type
 */
function mapJSONSchemaToRust(propDef: any): string {
  const type = propDef.type;
  const ref = propDef.$ref;

  // Handle $ref
  if (ref) {
    const refMatch = ref.match(/#\/definitions\/(\w+)$/);
    if (refMatch) {
      const refType = refMatch[1];
      // Map common types to Rust equivalents
      const rustTypeMap: Record<string, string> = {
        'SchemaVersion': 'String',
        'Timestamp': 'String',
        'UUID': 'String',
        'WorldRef': 'String',
        'CreatorRef': 'String',
        'StoryRef': 'String',
        'KBRef': 'String',
        'DeltaSequence': 'u64',
        'ManuscriptPhase': 'ManuscriptPhase',
        'ManuscriptState': 'ManuscriptState',
        'TimePolicy': 'TimePolicy',
        'Visibility': 'Visibility',
      };
      return rustTypeMap[refType] || refType;
    }
    return 'serde_json::Value'; // Placeholder for complex refs
  }

  // Handle basic types
  switch (type) {
    case 'string':
      if (propDef.enum) {
        // Will generate enum separately
        return 'String'; // Placeholder, will replace with enum
      }
      return 'String';
    case 'number':
      return 'f64';
    case 'integer':
      if (propDef.minimum === 0) return 'u64';
      return 'i64';
    case 'boolean':
      return 'bool';
    case 'array':
      if (propDef.items) {
        const itemType = mapJSONSchemaToRust(propDef.items);
        return `Vec<${itemType}>`;
      }
      return 'Vec<serde_json::Value>';
    case 'object':
      return 'serde_json::Value';
    default:
      return 'serde_json::Value';
  }
}

/**
 * Generate default values for required fields in Rust
 */
function generateRustDefaults(properties: any, requiredFields: string[]): string[] {
  const defaults: string[] = [];

  for (const [propName, propDef] of Object.entries(properties)) {
    if (propName === 'schema_version') continue;

    const isRequired = requiredFields.includes(propName);
    if (!isRequired) {
      defaults.push(`            ${propName}: None,`);
      continue;
    }

    const defaultValue = getRustDefaultValue(propDef as any);
    defaults.push(`            ${propName}: ${defaultValue},`);
  }

  return defaults;
}

/**
 * Get default value for Rust type
 */
function getRustDefaultValue(propDef: any): string {
  if (propDef.default !== undefined) {
    if (typeof propDef.default === 'string') {
      return `"${propDef.default}".to_string()`;
    }
    return JSON.stringify(propDef.default);
  }

  switch (propDef.type) {
    case 'string':
      if (propDef.enum) return `"${propDef.enum[0]}".to_string()`;
      return 'String::new()';
    case 'number':
      return '0.0';
    case 'integer':
      return '0';
    case 'boolean':
      return 'false';
    case 'array':
      return 'Vec::new()';
    case 'object':
      return 'serde_json::Value::Null';
    default:
      return 'Default::default()';
  }
}

/**
 * Generate common_types.rs with shared Rust types
 */
function generateRustCommonTypes(outputDir: string): void {
  const content = `//! Common Types
//!
//! Shared type definitions imported by all generated types.
//! Based on schemas/common/common.schema.json

use serde::{Deserialize, Serialize};

// Basic types (aliases for clarity)
pub type SchemaVersion = String;
pub type Timestamp = String;
pub type UUID = String;

pub type WorldRef = String;
pub type CreatorRef = String;
pub type StoryRef = String;
pub type KBRef = String;
pub type DeltaSequence = u64;

// Enum types

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptPhase {
    Brainstorm,
    Write,
    Review,
    Provisional,
    Canon,
}

impl Default for ManuscriptPhase {
    fn default() -> Self {
        ManuscriptPhase::Brainstorm
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptState {
    Draft,
    Proposed,
    Confirmed,
    Published,
}

impl Default for ManuscriptState {
    fn default() -> Self {
        ManuscriptState::Draft
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimePolicy {
    Linear,
    Branching,
    Mergeable,
}

impl Default for TimePolicy {
    fn default() -> Self {
        TimePolicy::Linear
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Shared,
    Public,
}

impl Default for Visibility {
    fn default() -> Self {
        Visibility::Private
    }
}
`;

  writeFile(path.join(outputDir, 'common_types.rs'), content);
}
```

Expected: Rust generator created

- [ ] **Step 3: Update nexus-contracts lib.rs to import generated**

Read `crates/nexus-contracts/src/lib.rs`, replace placeholder with:

```rust
//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.

pub mod generated;

// Re-export all generated types
pub use generated::*;

// Placeholder module removed - all types now in generated/
```

Expected: lib.rs updated

- [ ] **Step 4: Commit Rust generator**

Run: `git add tooling/codegen/src/rust-generator.ts crates/nexus-contracts/src/generated crates/nexus-contracts/src/lib.rs && git commit -m "feat(codegen): implement Rust type generator"`

Expected: Commit successful

---

## Task 5: Implement Main Codegen Orchestrator

**Files:**
- Create: `tooling/codegen/src/index.ts`

- [ ] **Step 1: Create main codegen orchestrator**

Create file: `tooling/codegen/src/index.ts`

```typescript
import { loadAllSchemas, validateSchemaStructure } from './schema-loader';
import { generateTSTypes } from './ts-generator';
import { generateRustTypes } from './rust-generator';
import { logger } from './utils';

/**
 * Main codegen orchestrator
 *
 * Runs full pipeline:
 * 1. Load all schemas from schemas/
 * 2. Validate schema structure
 * 3. Generate TypeScript types
 * 4. Generate Rust types
 */
export async function runCodegen(): Promise<void> {
  logger.info('Starting Nexus Codegen Pipeline');
  logger.info('==============================');

  // Step 1: Load schemas
  const schemas = loadAllSchemas();

  if (schemas.length === 0) {
    logger.error('No schemas to generate');
    process.exit(1);
  }

  // Step 2: Validate schemas
  logger.info('Validating schemas...');
  const invalidSchemas = schemas.filter(s => !validateSchemaStructure(s));

  if (invalidSchemas.length > 0) {
    logger.error(`Found ${invalidSchemas.length} invalid schemas`);
    process.exit(1);
  }
  logger.success('All schemas valid');

  // Step 3: Generate TypeScript types
  logger.info('\n--- Generating TypeScript Types ---');
  generateTSTypes(schemas);

  // Step 4: Generate Rust types
  logger.info('\n--- Generating Rust Types ---');
  generateRustTypes(schemas);

  logger.success('\n✓ Codegen complete');
  logger.info(`Generated ${schemas.length} schemas to TS + Rust`);
}

// Run if executed directly
if (require.main === module) {
  runCodegen().catch(err => {
    logger.error(`Codegen failed: ${err.message}`);
    process.exit(1);
  });
}
```

Expected: Main orchestrator created

- [ ] **Step 2: Build codegen tool**

Run: `cd tooling/codegen && npm run build`

Expected: Codegen tool compiled successfully

- [ ] **Step 3: Run codegen**

Run: `cd tooling/codegen && npm run codegen`

Expected: Codegen runs successfully, generates TS and Rust types

- [ ] **Step 4: Verify generated TypeScript types**

Run: `ls packages/nexus-contracts/src/generated`

Expected: Shows generated TS type files (index.ts, Bundle.ts, Creator.ts, etc.)

- [ ] **Step 5: Verify generated Rust types**

Run: `ls crates/nexus-contracts/src/generated`

Expected: Shows generated Rust type files (mod.rs, bundle.rs, creator.rs, etc.)

- [ ] **Step 6: Update packages/nexus-contracts/src/index.ts to import generated**

Read `packages/nexus-contracts/src/index.ts`, replace placeholder with:

```typescript
/**
 * Nexus Wire Contracts (Generated from JSON Schema)
 *
 * This package contains TypeScript type definitions generated from `schemas/` JSON Schema files.
 * All wire types are auto-generated - do not modify manually.
 */

// Re-export all generated types
export * from './generated';

// Schema version constant
export const SCHEMA_VERSION = "1.0.0";
```

Expected: index.ts updated

- [ ] **Step 7: Commit codegen orchestrator**

Run: `git add tooling/codegen/src/index.ts packages/nexus-contracts/src/index.ts && git commit -m "feat(codegen): implement main codegen orchestrator and update index exports"`

Expected: Commit successful

---

## Task 6: Update Root Scripts and CI

**Files:**
- Modify: `package.json` (root) - add codegen scripts
- Modify: `.github/workflows/ci.yml` - add codegen verification

- [ ] **Step 1: Add codegen scripts to root package.json**

Read `package.json`, add to scripts section:

```json
{
  "scripts": {
    "codegen": "cd tooling/codegen && npm run codegen",
    "codegen:watch": "cd tooling/codegen && npm run dev",
    "codegen:build": "cd tooling/codegen && npm run build"
  }
}
```

Expected: Scripts added

- [ ] **Step 2: Update CI workflow to verify codegen**

Read `.github/workflows/ci.yml`, add new job after `validate-schemas`:

```yaml
  verify-codegen:
    name: Verify Codegen Output
    runs-on: ubuntu-latest
    needs: validate-schemas
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - name: Install codegen dependencies
        run: cd tooling/codegen && npm install
      - name: Build codegen tool
        run: cd tooling/codegen && npm run build
      - name: Run codegen
        run: pnpm run codegen
      - name: Verify TypeScript generated
        run: test -d packages/nexus-contracts/src/generated && test -f packages/nexus-contracts/src/generated/index.ts
      - name: Verify Rust generated
        run: test -d crates/nexus-contracts/src/generated && test -f crates/nexus-contracts/src/generated/mod.rs
      - name: Archive generated types
        uses: actions/upload-artifact@v3
        with:
          name: generated-types
          path: |
            packages/nexus-contracts/src/generated/
            crates/nexus-contracts/src/generated/
```

Expected: CI workflow updated

- [ ] **Step 3: Run pnpm install to update lockfile**

Run: `pnpm install`

Expected: Lockfile updated

- [ ] **Step 4: Test full pipeline**

Run: `pnpm run validate-schemas && pnpm run codegen`

Expected: Validation passes, codegen generates types

- [ ] **Step 5: Commit scripts and CI update**

Run: `git add package.json .github/workflows/ci.yml pnpm-lock.yaml && git commit -m "feat(codegen): add codegen scripts to root package and CI verification"`

Expected: Commit successful

---

## Task 7: Create Codegen Documentation

**Files:**
- Create: `tooling/codegen/README.md`
- Create: `docs/CODEGEN.md`

- [ ] **Step 1: Create codegen tool README**

Create file: `tooling/codegen/README.md`

```markdown
# Nexus Codegen Tool

Schema-to-code generation pipeline for Nexus wire contracts.

## Purpose

Transform JSON Schema files in `schemas/` into:
- TypeScript types (`packages/nexus-contracts/src/generated/`)
- Rust types (`crates/nexus-contracts/src/generated/`)

## Usage

```bash
# Run full codegen pipeline
pnpm run codegen

# Build codegen tool
cd tooling/codegen && npm run build

# Watch mode (regenerate on schema changes)
pnpm run codegen:watch
```

## Workflow

1. **Load schemas** from `schemas/**/*.schema.json`
2. **Validate** each schema has required fields ($schema, $id, schema_version, title, type)
3. **Generate TypeScript** interfaces in `packages/nexus-contracts/src/generated/`
4. **Generate Rust** structs in `crates/nexus-contracts/src/generated/`

## Output Structure

### TypeScript
```
packages/nexus-contracts/src/generated/
├── index.ts           # Re-exports all types
├── CommonTypes.ts     # Shared types (UUID, Timestamp, enums)
├── Bundle.ts          # Bundle envelope type
├── Creator.ts         # Creator entity type
├── World.ts           # World entity type
└── ...
```

### Rust
```
crates/nexus-contracts/src/generated/
├── mod.rs             # Module declarations
├── common_types.rs    # Shared types and enums
├── bundle.rs          # Bundle envelope struct
├── creator.rs         # Creator entity struct
├── world.rs           # World entity struct
└── ...
```

## Do Not Modify Generated Types

All generated files are auto-generated. To change types:
1. Update schema in `schemas/`
2. Run `pnpm run codegen`
3. Commit schema + generated changes together

## CI Integration

CI workflow (`validate-schemas` → `verify-codegen`) ensures:
- Schemas are valid before codegen
- Generated types match schemas
- Generated files are archived as artifacts
```

Expected: README created

- [ ] **Step 2: Create user documentation**

Create file: `docs/CODEGEN.md`

```markdown
# Schema Code Generation

Nexus uses **JSON Schema as single truth source** for wire contracts.

## Philosophy

**One schema, two languages:**

```
schemas/*.schema.json → TypeScript + Rust types
```

All wire types are **generated**, not handwritten. This ensures:
- ✅ Consistency across CLI and platform
- ✅ Schema-driven versioning
- ✅ Automatic validation support
- ✅ No drift between implementations

## How It Works

### Define Schema

Write JSON Schema in `schemas/domain/*.schema.json`:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://nexus.42ch.io/schemas/domain/bundle.schema.json",
  "schema_version": "1.0.0",
  "title": "Nexus Bundle Envelope",
  "type": "object",
  "required": ["schema_version", "bundle_id", "world_ref"],
  "properties": {
    "schema_version": {"type": "string"},
    "bundle_id": {"type": "string"},
    "world_ref": {"$ref": "...#/definitions/WorldRef"}
  }
}
```

### Run Codegen

```bash
pnpm run codegen
```

### Generated Output

**TypeScript:** `packages/nexus-contracts/src/generated/Bundle.ts`

```typescript
export interface Bundle {
  schema_version: string;
  bundle_id: string;
  world_ref?: WorldRef;
}
```

**Rust:** `crates/nexus-contracts/src/generated/bundle.rs`

```rust
#[derive(Serialize, Deserialize)]
pub struct Bundle {
    pub schema_version: String,
    pub bundle_id: String,
    pub world_ref: Option<String>,
}
```

## Versioning

Schema version (`schema_version`) is embedded in generated types.

**Version bump rules:**
- **Major**: Breaking field changes
- **Minor**: New optional fields
- **Patch**: Documentation only

## Development Workflow

1. Update schema in `schemas/`
2. Run `pnpm run validate-schemas` (validate first)
3. Run `pnpm run codegen` (generate types)
4. Implement features using generated types
5. Test with Rust: `cargo test`
6. Test with TypeScript: `pnpm run typecheck`
7. Commit schema + generated changes together

## Never Edit Generated Files

Generated files have header: `AUTO-GENERATED - DO NOT MODIFY`

Edit schemas instead, then regenerate.

## CI Pipeline

CI ensures generated types match schemas:
1. `validate-schemas`: Validate JSON Schema syntax
2. `verify-codegen`: Run codegen and verify output exists

If codegen fails, CI fails - no drift allowed.
```

Expected: User documentation created

- [ ] **Step 3: Commit documentation**

Run: `git add tooling/codegen/README.md docs/CODEGEN.md && git commit -m "docs(codegen): add codegen tool and user documentation"`

Expected: Commit successful

---

## Verification

- [ ] **Final verification: Run full codegen pipeline**

Run: `pnpm run validate-schemas && pnpm run codegen`

Expected: No errors, all types generated

- [ ] **Verify TypeScript compilation**

Run: `cd packages/nexus-contracts && pnpm run typecheck`

Expected: TypeScript compiles without errors

- [ ] **Verify Rust compilation**

Run: `cargo check --workspace`

Expected: Rust workspace compiles without errors

- [ ] **Verify generated file count**

Run: `ls packages/nexus-contracts/src/generated/*.ts | wc -l && ls crates/nexus-contracts/src/generated/*.rs | wc -l`

Expected: Shows count matching schema count (6 schemas → 6 TS files + 1 mod.rs + 1 common_types.rs)

---

## Completion

After all tasks complete:
- [ ] Update `.agents/plans/status.json` with completion status
- [ ] Create git tag: `git tag v0.1.0-codegen -a -m "Phase 0: Codegen pipeline implemented"`
- [ ] Push to remote: `git push origin main --tags`

---

**Plan saved to:** `.agents/plans/2025-04-05-codegen-pipeline.md`