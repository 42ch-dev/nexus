/**
 * TypeScript generation via `json-schema-to-typescript`.
 *
 * Consumes the localized schema tree produced by `schema-prep.ts` (T1). For each
 * non-skip schema, compiles the schema to TypeScript, overriding the schema `title`
 * to the basename-derived PascalCase type name. Nexus schemas carry a `Nexus <Name>`
 * product prefix in their `title`; the canonical contract type name is derived from
 * the file basename (via `schemaToTypeName`) so consumer imports and `SCHEMA_VERSIONS`
 * keys stay stable regardless of how titles are phrased.
 *
 * Why `compile()` (object form) instead of `compileFromFile()`: the title override
 * must happen before compilation, and `compileFromFile` reads the file as-is. Passing
 * the mutated schema object to `compile()` keeps `$ref` resolution working via `cwd`
 * (verified: cross-file refs resolve identically to `compileFromFile`).
 *
 * Skip-list (`SKIP_LIST`): `common.schema.json` (definitions-only), `source-anchor.schema.json`
 * (shared value object referenced by many schemas), and `platform/sync/bundle-refinement.schema.json`
 * (canonical-skip). These must NOT emit a standalone consumer-scope type file. Because
 * `common` + `source-anchor` are referenced by emitted schemas and imported by consumers
 * (e.g. `SchemaVersion`, `SourceAnchor`), their types are emitted into a synthetic
 * `common/CommonTypes.ts` so they remain barrel-importable (brief Step 3 escape hatch).
 *
 * Barrel strategy: each leaf subdir gets an `index.ts` of named root exports
 * (`export type { TypeName } from './<base>'`) — mirroring the sibling spoke codegen's
 * single-word-title branch. Named (not `export *`) per-file re-exports prevent
 * `declareExternallyReferenced` inline declarations (e.g. a repeated `SourceAnchor`)
 * from colliding at the barrel. The root `index.ts` does `export * from './<subdir>'`
 * for each consumer-scope subdir plus the `SCHEMA_VERSIONS` / `LATEST_SCHEMA_VERSION` stamp.
 *
 * P1 hook: Rust generation (typify) will consume the dereferenced tree separately; this
 * module only touches the TypeScript output tree.
 */
import { compile } from 'json-schema-to-typescript';
import { glob } from 'glob';
import fs from 'fs';
import path from 'path';
import {
  resolveFromRoot,
  writeFile,
  ensureDir,
  logger,
  schemaToTypeName,
  maxSchemaVersion,
  readJSON,
  extractSchemaVersion,
} from './utils';
import { resolvePrepPaths, runPrep } from './schema-prep';

/** Derive the compile() schema param type without depending on `@types/json-schema`. */
type JsonSchema = Parameters<typeof compile>[0];

const BANNER = `/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */`;

/**
 * Schemas that must NOT produce a standalone consumer-scope type file.
 * Locked by the architect (Q5): they exist in the prep trees but are filtered here.
 *  - common.schema.json: definitions-only container.
 *  - source-anchor.schema.json: shared value object; emitted into common/CommonTypes.ts.
 *  - platform/sync/bundle-refinement.schema.json: canonical-skip.
 */
const SKIP_LIST = new Set<string>([
  'common/common.schema.json',
  'common/source-anchor.schema.json',
  'platform/sync/bundle-refinement.schema.json',
]);

/** Compile options — mirrors the sibling spoke codegen (run.mjs generateTypeScript). */
const COMPILE_OPTS = {
  bannerComment: '',
  unreachableDefinitions: true,
  enableConstEnums: true,
  strictIndexSignatures: true,
  declareExternallyReferenced: true,
};

/** POSIX-relative schema paths (relative to the localized tree root). */
function posix(rel: string): string {
  return rel.split(path.sep).join('/');
}

/**
 * Generate the full TypeScript contract tree under
 * `packages/nexus-contracts/src/generated/`. Assumes `runPrep()` has already built the
 * localized tree (the orchestrator in `index.ts` runs prep first).
 */
export async function generateTSTypes(): Promise<void> {
  const { localizedDir, srcSchemasDir } = resolvePrepPaths();
  const outDir = resolveFromRoot('packages', 'nexus-contracts', 'src', 'generated');
  logger.info(`Generating TypeScript types to: ${outDir}`);

  const allRel = (await glob('**/*.schema.json', { cwd: localizedDir })).map(posix).sort();
  const emitRel = allRel.filter(rel => !SKIP_LIST.has(rel));
  const skipped = allRel.filter(rel => SKIP_LIST.has(rel));

  // Reset the generated tree (regeneration is authoritative).
  fs.rmSync(outDir, { recursive: true, force: true });
  ensureDir(outDir);

  // 1. Synthetic common module: common definitions + SourceAnchor (skip-listed but referenced).
  await generateCommonTypesModule(localizedDir, outDir);

  // 2. Per-schema files.
  const versionRows: Array<{ typeName: string; version: number; relDir: string; base: string; rel: string }> = [];
  for (const rel of emitRel) {
    const fileName = path.basename(rel);
    const base = path.basename(rel, '.schema.json');
    const relDir = posix(path.dirname(rel));
    const typeName = schemaToTypeName(fileName);
    const ts = await compileSchema(localizedDir, rel, typeName);
    writeFile(path.join(outDir, ...relDir.split('/'), `${base}.ts`), ts);
    const originalSchema = readJSON<Record<string, unknown>>(path.join(srcSchemasDir, rel));
    versionRows.push({ typeName, version: extractSchemaVersion(originalSchema), relDir, base, rel });
  }

  // 3. Per-subdir barrel index.ts files (named root exports only).
  writeSubdirBarrels(outDir, emitRel);

  // 4. Root index.ts: flat subdir re-exports + SCHEMA_VERSIONS + LATEST_SCHEMA_VERSION.
  writeRootIndex(outDir, versionRows);

  logger.success(
    `Generated TypeScript for ${emitRel.length} schema(s) (+ common module); skipped ${skipped.length}: ${skipped.join(', ')}`,
  );
}

/** Load a localized schema, override its `title` to the canonical type name, compile. */
async function compileSchema(localizedDir: string, rel: string, typeName: string): Promise<string> {
  const abs = path.join(localizedDir, ...rel.split('/'));
  const schema = readJSON<Record<string, unknown>>(abs) as JsonSchema;
  // Override `Nexus <Name>` product prefix → canonical basename-derived name.
  (schema as Record<string, unknown>).title = typeName;
  const ts = await compile(schema, typeName, { ...COMPILE_OPTS, cwd: path.dirname(abs) });
  return `${BANNER}\n\n${ts.trim()}\n`;
}

/**
 * Emit `common/CommonTypes.ts` from `common.schema.json` (52 definition-keyed types) and
 * `source-anchor.schema.json` (the `SourceAnchor` value object). Neither schema carries
 * cross-file `$ref`, so the two outputs concatenate without intra-file duplicate
 * declarations. These types stay barrel-importable via `common/index.ts` → root.
 */
async function generateCommonTypesModule(localizedDir: string, outDir: string): Promise<void> {
  const commonDir = path.join(localizedDir, 'common');

  const commonSchema = readJSON<Record<string, unknown>>(path.join(commonDir, 'common.schema.json')) as JsonSchema;
  (commonSchema as Record<string, unknown>).title = 'CommonTypes';
  const commonTs = await compile(commonSchema, 'CommonTypes', { ...COMPILE_OPTS, cwd: commonDir });

  const anchorSchema = readJSON<Record<string, unknown>>(
    path.join(commonDir, 'source-anchor.schema.json'),
  ) as JsonSchema;
  (anchorSchema as Record<string, unknown>).title = 'SourceAnchor';
  const anchorTs = await compile(anchorSchema, 'SourceAnchor', { ...COMPILE_OPTS, cwd: commonDir });

  const content = `${BANNER}\n\n${commonTs.trim()}\n\n${anchorTs.trim()}\n`;
  writeFile(path.join(outDir, 'common', 'CommonTypes.ts'), content);
}

/**
 * Write `<relDir>/index.ts` for every leaf subdir that has generated files. Each barrel
 * uses named root exports (one per file) so inline `declareExternallyReferenced`
 * declarations are not re-exported. The `common` subdir also re-exports `CommonTypes`.
 */
function writeSubdirBarrels(outDir: string, emitRel: string[]): void {
  const byDir = new Map<string, string[]>();
  for (const rel of emitRel) {
    const relDir = posix(path.dirname(rel));
    const base = path.basename(rel, '.schema.json');
    const list = byDir.get(relDir) ?? [];
    list.push(base);
    byDir.set(relDir, list);
  }

  for (const [relDir, bases] of byDir) {
    const lines: string[] = [];
    if (relDir === 'common') {
      lines.push(`export * from './CommonTypes';`);
    }
    for (const base of bases.sort()) {
      const typeName = schemaToTypeName(`${base}.schema.json`);
      lines.push(`export type { ${typeName} } from './${base}';`);
    }
    const dirSegments = relDir.split('/');
    writeFile(path.join(outDir, ...dirSegments, 'index.ts'), `${BANNER}\n\n${lines.join('\n')}\n`);
  }
}

/**
 * Root `index.ts`: `export *` from each consumer-scope subdir (including `common`),
 * then the `SCHEMA_VERSIONS` record (key = basename-derived type name, value = the
 * schema's `schema_version`) and `LATEST_SCHEMA_VERSION` (max). Rows are ordered by
 * schema path so output is stable.
 */
function writeRootIndex(
  outDir: string,
  versionRows: Array<{ typeName: string; version: number; relDir: string; base: string; rel: string }>,
): void {
  const subdirs = Array.from(new Set(['common', ...versionRows.map(r => r.relDir)])).sort();
  const sortedRows = [...versionRows].sort((a, b) => a.rel.localeCompare(b.rel));

  const lines: string[] = [BANNER, ''];
  for (const relDir of subdirs) {
    lines.push(`export * from './${relDir}';`);
  }
  lines.push('');
  lines.push('// Schema version constants');
  lines.push('export const SCHEMA_VERSIONS: Record<string, number> = {');
  for (const { typeName, version } of sortedRows) {
    lines.push(`  ${typeName}: ${version},`);
  }
  lines.push('};');
  lines.push('');
  lines.push('// Highest schema_version among emitted contract schemas');
  lines.push(`export const LATEST_SCHEMA_VERSION = ${maxSchemaVersion(sortedRows.map(r => r.version))};`);

  writeFile(path.join(outDir, 'index.ts'), lines.join('\n') + '\n');
}

// Run if executed directly (tsx / node dist). Runs prep first so ts-gen is independently smoke-testable.
// eslint-disable-next-line @typescript-eslint/no-require-imports
if (typeof require !== 'undefined' && require.main === module) {
  runPrep()
    .then(() => generateTSTypes())
    .catch((err: Error) => {
      logger.error(`TS generation failed: ${err.message}`);
      if (process.env.DEBUG) {
        console.error(err);
      }
      process.exit(1);
    });
}
