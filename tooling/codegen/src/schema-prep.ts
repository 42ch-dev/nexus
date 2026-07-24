/**
 * Schema localization + dereference layer.
 *
 * Two-stage preparation ported from the sibling spoke repo's codegen orchestrator
 * (spoke tooling/codegen/src/run.mjs): the only adaptation is the base URI
 * (spoke42.invalid → nexus42.invalid).
 *
 *   1. localize  — rewrite every schema's `$id` and any `$ref` that points at the
 *                  nexus42.invalid base URI into a POSIX-relative file path, mirroring
 *                  the `schemas/` tree under `.schemas-localized/`. Bare-relative refs
 *                  (e.g. `delta.schema.json`) are already filesystem-relative and are
 *                  left untouched; `$RefParser` resolves them natively.
 *   2. dereference — feed each localized schema through `@apidevtools/json-schema-ref-parser`
 *                  to inline every cross-file `$ref`, writing self-contained schemas to
 *                  `.schemas-dereferenced/`.
 *
 * Why dereference: the downstream Rust generator (typify, P1) needs self-contained
 * schemas and cannot resolve cross-file `$ref` itself. `json-schema-to-typescript` (T2)
 * can resolve refs itself, but resolving them against the localized tree first makes
 * `cwd`-relative resolution robust.
 *
 * Run directly for a smoke check: `tsx tooling/codegen/src/schema-prep.ts`.
 */
import $RefParser, { type FileInfo, type ParserOptions } from '@apidevtools/json-schema-ref-parser';
import { glob } from 'glob';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'node:url';
import { resolveFromRoot, logger } from './utils';

/**
 * Base URI used in committed schemas' `$id` / `$ref`
 * (RFC 6761 `.invalid` reserved; production domain TBD — see schemas/AGENTS.md).
 */
export const NEXUS_SCHEMA_BASE = 'https://nexus42.invalid/schemas/';

/** Resolved prep-layer paths (env-overridable per the architect-locked contract). */
export interface SchemaPrepPaths {
  /** Repository root (drives default resolution for the other paths). */
  repoRoot: string;
  /** Source JSON Schema tree (`schemas/`). */
  srcSchemasDir: string;
  /** Intermediate localized tree (internal scratch dir, not env-configurable). */
  localizedDir: string;
  /** Dereferenced output tree, consumed by downstream generators (P1 typify / T2 jstt). */
  derefSchemasDir: string;
}

/**
 * Resolve prep paths from env vars:
 *  - `NEXUS_REPO_ROOT`      repo root (default: `<src>/../../..`, i.e. monorepo root).
 *  - `NEXUS_SRC_SCHEMAS_DIR` source schemas dir (default: `<repoRoot>/schemas`).
 *  - `NEXUS_DEREF_SCHEMAS_DIR` deref output dir for P1 (default: `<codegen>/.schemas-dereferenced`).
 *
 * The localized tree (`.schemas-localized/`) is an internal scratch dir and is not
 * env-configurable — it always sits next to the deref tree under `tooling/codegen/`.
 */
export function resolvePrepPaths(): SchemaPrepPaths {
  const repoRoot = process.env.NEXUS_REPO_ROOT ?? resolveFromRoot();
  const srcSchemasDir = process.env.NEXUS_SRC_SCHEMAS_DIR ?? path.join(repoRoot, 'schemas');
  const codegenDir = path.resolve(__dirname, '..');
  const localizedDir = path.join(codegenDir, '.schemas-localized');
  const derefSchemasDir = process.env.NEXUS_DEREF_SCHEMAS_DIR ?? path.join(codegenDir, '.schemas-dereferenced');
  return { repoRoot, srcSchemasDir, localizedDir, derefSchemasDir };
}

/**
 * Rewrite a schema object's base-URI `$id` and any base-URI `$ref` into
 * POSIX-relative file paths. Mutates and returns the given object.
 *
 * Bare-relative refs (e.g. `delta.schema.json`) do not start with the base URI and
 * pass through unchanged; `$RefParser` resolves them against the file's location.
 *
 * @param schemaObj    parsed schema (mutated in place; pass a clone to preserve the original)
 * @param relSchemaPath path relative to `schemas/`, POSIX slashes (e.g. `domain/fork-branch.schema.json`)
 */
export function localizeSchemaRefs(
  schemaObj: Record<string, unknown>,
  relSchemaPath: string,
): Record<string, unknown> {
  const schemaDir = path.dirname(relSchemaPath);

  if (typeof schemaObj.$id === 'string' && schemaObj.$id.startsWith(NEXUS_SCHEMA_BASE)) {
    schemaObj.$id = relSchemaPath;
  }

  const visit = (node: unknown): void => {
    if (Array.isArray(node)) {
      node.forEach(visit);
      return;
    }
    if (node === null || typeof node !== 'object') {
      return;
    }
    const obj = node as Record<string, unknown>;
    const ref = obj.$ref;
    if (typeof ref === 'string' && ref.startsWith(NEXUS_SCHEMA_BASE)) {
      const [filePart, fragment] = ref.slice(NEXUS_SCHEMA_BASE.length).split('#');
      let localized = path.relative(schemaDir, filePart).split(path.sep).join('/');
      if (!localized.startsWith('.')) {
        localized = `./${localized}`;
      }
      obj.$ref = fragment ? `${localized}#${fragment}` : localized;
    }
    for (const value of Object.values(obj)) {
      visit(value);
    }
  };

  visit(schemaObj);
  return schemaObj;
}

/**
 * Build the localized schema tree: glob `schemas/`, rewrite each schema's base-URI
 * `$id`/`$ref` to POSIX-relative paths, and write the result mirroring the `schemas/`
 * tree under `localizedDir`. Returns the sorted list of schema paths (relative to the
 * source schemas dir, POSIX slashes) for downstream stages.
 */
export async function buildLocalizedSchemaTree(
  srcSchemasDir: string,
  localizedDir: string,
): Promise<string[]> {
  fs.rmSync(localizedDir, { recursive: true, force: true });
  const schemaPaths = await glob('**/*.schema.json', { cwd: srcSchemasDir });
  for (const relSchema of schemaPaths) {
    const raw = JSON.parse(fs.readFileSync(path.join(srcSchemasDir, relSchema), 'utf8'));
    const localized = localizeSchemaRefs(structuredClone(raw), relSchema);
    const outPath = path.join(localizedDir, relSchema);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(localized, null, 2));
  }
  return schemaPaths.sort();
}

const FILE_PROTOCOL_PATTERN = /^file:/i;
const NON_FILE_PROTOCOL_PATTERN = /^[a-z][a-z0-9+.-]*:/i;

/**
 * True when a ref URL is a local filesystem path or `file:` URL (not http/https).
 */
export function isLocalFileRefUrl(url: string): boolean {
  if (FILE_PROTOCOL_PATTERN.test(url)) {
    return true;
  }
  return !NON_FILE_PROTOCOL_PATTERN.test(url);
}

/**
 * Convert a ref-parser file URL to an absolute filesystem path (hash/query stripped).
 */
export function refUrlToFilePath(url: string): string {
  const withoutFragment = url.split('#')[0] ?? url;
  const withoutQuery = withoutFragment.split('?')[0] ?? withoutFragment;
  if (FILE_PROTOCOL_PATTERN.test(withoutQuery)) {
    return fileURLToPath(withoutQuery);
  }
  return path.resolve(withoutQuery);
}

/**
 * Reject resolved paths that escape `confinedRoot` (prefix check after realpath).
 */
export function assertPathWithinRoot(confinedRoot: string, candidatePath: string): void {
  const rootReal = fs.realpathSync.native(path.resolve(confinedRoot));
  const rootPrefix = rootReal.endsWith(path.sep) ? rootReal : `${rootReal}${path.sep}`;

  let resolved: string;
  try {
    resolved = fs.realpathSync.native(candidatePath);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException).code;
    if (code !== 'ENOENT') {
      throw err;
    }
    resolved = path.resolve(candidatePath);
  }

  if (resolved !== rootReal && !resolved.startsWith(rootPrefix)) {
    throw new Error(
      `Refusing to dereference $ref outside localized schema tree: ${candidatePath}`,
    );
  }
}

/**
 * Dereference options that disable HTTP(S) fetch and confine file reads to `localizedDir`.
 */
export function createConfinedDereferenceOptions(localizedDir: string): ParserOptions {
  const confinedRoot = fs.realpathSync.native(path.resolve(localizedDir));

  return {
    resolve: {
      http: false,
      file: {
        canRead: isLocalFileRefUrl,
        async read(file: FileInfo) {
          const filePath = refUrlToFilePath(file.url);
          assertPathWithinRoot(confinedRoot, filePath);
          return fs.promises.readFile(filePath);
        },
      },
    },
  };
}

/**
 * Build the dereferenced schema tree: for each localized schema, inline every
 * cross-file `$ref` via `@apidevtools/json-schema-ref-parser`, writing self-contained
 * schemas to `derefSchemasDir`. Output schemas carry no cross-file `$ref` and no
 * `nexus42.invalid` URIs.
 */
export async function buildDereferencedSchemaTree(
  schemaPaths: string[],
  localizedDir: string,
  derefSchemasDir: string,
): Promise<void> {
  fs.rmSync(derefSchemasDir, { recursive: true, force: true });
  const derefOptions = createConfinedDereferenceOptions(localizedDir);
  for (const relSchema of schemaPaths) {
    const inputPath = path.join(localizedDir, relSchema);
    const dereferenced = await $RefParser.dereference(inputPath, derefOptions);
    const outPath = path.join(derefSchemasDir, relSchema);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(dereferenced, null, 2));
  }
}

/**
 * Run the full localization + dereference prep on the real `schemas/` tree.
 * Logs resolved paths and counts. Intended to run via `tsx tooling/codegen/src/schema-prep.ts`.
 */
export async function runPrep(): Promise<SchemaPrepPaths> {
  const paths = resolvePrepPaths();
  logger.info('Schema prep: localize + dereference');
  logger.info(`  src:   ${paths.srcSchemasDir}`);
  logger.info(`  local: ${paths.localizedDir}`);
  logger.info(`  deref: ${paths.derefSchemasDir}`);

  const schemaPaths = await buildLocalizedSchemaTree(paths.srcSchemasDir, paths.localizedDir);
  logger.success(`Localized ${schemaPaths.length} schema(s)`);

  await buildDereferencedSchemaTree(schemaPaths, paths.localizedDir, paths.derefSchemasDir);
  logger.success(`Dereferenced ${schemaPaths.length} schema(s)`);

  return paths;
}

// Run if executed directly (tsx / node dist). Mirrors index.ts's guard shape.
// eslint-disable-next-line @typescript-eslint/no-require-imports
if (require.main === module) {
  runPrep().catch((err: Error) => {
    logger.error(`Schema prep failed: ${err.message}`);
    if (process.env.DEBUG) {
      console.error(err);
    }
    process.exit(1);
  });
}
