import path from 'path';
import fs from 'fs';

/**
 * Resolve path relative to project root.
 * Works from both dist/ (compiled) and src/ (ts-node) contexts.
 */
export function resolveFromRoot(...segments: string[]): string {
  // When compiled: __dirname = tooling/codegen/dist → go up 3 levels
  // When source: __dirname = tooling/codegen/src → go up 3 levels
  const root = path.resolve(__dirname, '..', '..', '..');
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
 * Write file, creating parent directories as needed
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
 * Extract schema_version from schema object.
 * Schema version is always an integer in Nexus schemas.
 */
export function extractSchemaVersion(schema: Record<string, unknown>): number {
  const v = schema.schema_version;
  return typeof v === 'number' ? v : 1;
}

/**
 * Convert schema file name to PascalCase type name.
 * Example: "bundle.schema.json" -> "Bundle"
 * Example: "world-membership.schema.json" -> "WorldMembership"
 */
export function schemaToTypeName(fileName: string): string {
  return fileName
    .replace('.schema.json', '')
    .split('-')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join('');
}

/**
 * Convert PascalCase to snake_case for Rust module names.
 * Example: "WorldMembership" -> "world_membership"
 */
export function toSnakeCase(str: string): string {
  return str
    .replace(/([a-z])([A-Z])/g, '$1_$2')
    .toLowerCase();
}

/**
 * Simple logger utility
 */
export const logger = {
  info: (msg: string) => console.log(`[INFO] ${msg}`),
  success: (msg: string) => console.log(`[OK] ${msg}`),
  warn: (msg: string) => console.warn(`[WARN] ${msg}`),
  error: (msg: string) => console.error(`[ERR] ${msg}`),
};
