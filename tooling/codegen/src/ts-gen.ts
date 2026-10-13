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
import { randomUUID } from 'crypto';
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
import { resolvePrepPaths } from './schema-prep';

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

/**
 * Compile options — mirrors the sibling spoke codegen (run.mjs generateTypeScript).
 *
 * `ignoreMinAndMaxItems`: collapses `minItems`/`maxItems` tuple unions back to
 * plain `T[]`. json-schema-to-typescript otherwise expands bounded arrays into
 * pathological fixed-length tuple unions (e.g. `worlds` maxItems:20 → a 1738-line
 * union of 0..20-element tuples). Runtime validation (AJV/zod/etc.) already
 * enforces these bounds at the wire layer — TS tuple structural enforcement is
 * redundant here and explodes the generated `.d.ts` bundle size (~18%).
 */
const COMPILE_OPTS = {
  bannerComment: '',
  unreachableDefinitions: true,
  enableConstEnums: true,
  strictIndexSignatures: true,
  declareExternallyReferenced: true,
  ignoreMinAndMaxItems: true,
};

/** Exact-string patterns (`^literal$`) become singleton enums so TS emits literals.
 * Rust typify still consumes the source `pattern` discriminants (it panics on
 * per-arm `enum`/`const` inside oneOf). This rewrite is TypeScript-compile only.
 */
export function rewriteExactStringPatterns(node: unknown): void {
  if (Array.isArray(node)) {
    for (const item of node) rewriteExactStringPatterns(item);
    return;
  }
  if (node === null || typeof node !== 'object') {
    return;
  }
  const rec = node as Record<string, unknown>;
  const pattern = rec.pattern;
  if (typeof pattern === 'string' && rec.type === 'string') {
    const exact = pattern.match(/^\^([A-Za-z0-9_]+)\$$/);
    if (exact) {
      rec.enum = [exact[1]];
      delete rec.pattern;
    }
  }
  for (const value of Object.values(rec)) rewriteExactStringPatterns(value);
}

/**
 * Draft-07 annotation keywords: carry metadata, never a type constraint. Their
 * values are example data (arbitrary JSON), not schemas — the walker below never
 * descends into them.
 */
const ANNOTATION_KEYWORDS: Record<string, true> = {
  $comment: true,
  title: true,
  description: true,
  default: true,
  examples: true,
  deprecated: true,
  readOnly: true,
  writeOnly: true,
  // Nexus extension marker (x-nexus-tri-state): boundary-parser metadata,
  // never a type constraint. Without it the deliberately unrestricted
  // tri-state carriers (patch-work-request world_id/story_ref,
  // update-finding-request rule_suggestion) miss the unrestricted rewrite
  // and compile to an object index signature that rejects the legal
  // null/string wire values.
  'x-nexus-tri-state': true,
};

/** Keywords whose value is a single subschema (or a tuple array of subschemas for `items`). */
const SUBSCHEMA_KEYWORDS: Record<string, true> = {
  items: true,
  additionalProperties: true,
  additionalItems: true,
  contains: true,
  propertyNames: true,
  not: true,
  if: true,
  then: true,
  else: true,
};

/** Keywords whose value is an array of subschemas. */
const SUBSCHEMA_LIST_KEYWORDS: Record<string, true> = {
  allOf: true,
  anyOf: true,
  oneOf: true,
};

/** Keywords whose value maps names to subschemas (the keys are property names, not keywords). */
const SUBSCHEMA_MAP_KEYWORDS: Record<string, true> = {
  properties: true,
  patternProperties: true,
  $defs: true,
  definitions: true,
};

/** True when a subschema carries only annotation keywords — i.e. it is unrestricted ("any JSON value"). */
function isUnrestrictedSubschema(schema: unknown): boolean {
  if (typeof schema !== 'object' || schema === null || Array.isArray(schema)) return false;
  return Object.keys(schema).every(key => ANNOTATION_KEYWORDS[key] === true);
}

/** Rewrite a subschema at a known schema position, or descend when it stays an object. */
function rewriteSchemaPosition(value: unknown, replace: (next: unknown) => void): void {
  if (isUnrestrictedSubschema(value)) {
    replace(true);
  } else {
    rewriteUnrestrictedJsonValues(value);
  }
}

/**
 * Rewrite unrestricted JSON-value subschemas (`{}` or annotation-only objects) to the
 * boolean `true` schema so jstt emits `unknown` instead of an object index signature.
 * Without this, a typeless wire field (e.g. `core-tool-execute-response.result`,
 * mirroring `serde_json::Value` on the Rust side) compiles to
 * `{ [k: string]: unknown }`, which wrongly rejects scalar/array/null JSON values.
 * The boolean `true` schema compiles to `unknown` — the same representation the
 * handwritten contracts use for arbitrary JSON (`payload: unknown` in desktop ipc).
 *
 * Only known schema positions are descended: annotation values (`default`/`examples`)
 * are opaque data, `properties`/`definitions` keys are property names, and unknown
 * keywords (e.g. `schema_version`) are left untouched. The root schema is never
 * rewritten — jstt cannot compile a boolean root.
 */
export function rewriteUnrestrictedJsonValues(node: unknown): void {
  if (Array.isArray(node)) {
    for (let i = 0; i < node.length; i++) {
      rewriteSchemaPosition(node[i], next => {
        node[i] = next;
      });
    }
    return;
  }
  if (node === null || typeof node !== 'object') return;
  const slot = node as Record<string, unknown>;
  for (const [key, value] of Object.entries(slot)) {
    if (ANNOTATION_KEYWORDS[key] === true) continue;
    if (SUBSCHEMA_KEYWORDS[key] === true) {
      rewriteSchemaPosition(value, next => {
        slot[key] = next;
      });
    } else if (SUBSCHEMA_LIST_KEYWORDS[key] === true) {
      rewriteUnrestrictedJsonValues(value);
    } else if (
      SUBSCHEMA_MAP_KEYWORDS[key] === true &&
      typeof value === 'object' &&
      value !== null &&
      !Array.isArray(value)
    ) {
      for (const [name, sub] of Object.entries(value)) {
        rewriteSchemaPosition(sub, next => {
          (value as Record<string, unknown>)[name] = next;
        });
      }
    }
  }
}

/** POSIX-relative schema paths (relative to the localized tree root). */
function posix(rel: string): string {
  return rel.split(path.sep).join('/');
}

/** A generated consumer-scope schema module: its contract type name and file. */
export interface SchemaModule {
  /** Basename-derived contract type name (`schemaToTypeName`). */
  name: string;
  /** POSIX dir of the module, relative to the generated root. */
  relDir: string;
  /** File base name without the extension. */
  base: string;
  /** Type names the module declares at its top level, as compiled. */
  declared: Set<string>;
}

/**
 * Canonical owner of a declared type name, or `undefined` when the name has no
 * single canonical home (it then stays exactly as `json-schema-to-typescript`
 * emitted it).
 *
 * A name is owned when exactly one generated module both *declares* it and is its
 * family home: the module whose root type *is* the name (`WorkPoolEntry`), or — for
 * a nested type derived from an inlined `$ref` body — a module whose root type name
 * prefixes it. Declaring the name is required: a mere prefix match
 * (`CharacterActorRef` vs the `Character` module, which does not declare it) must
 * not invent a home. Ambiguous matches and the module itself resolve to `undefined`.
 */
function ownerOf(
  modules: SchemaModule[],
  typeName: string,
  self: SchemaModule,
): SchemaModule | undefined {
  const candidates = modules.filter(
    module =>
      module.declared.has(typeName) &&
      (module.name === typeName || typeName.startsWith(module.name)),
  );
  if (candidates.length !== 1) return undefined;
  const owner = candidates[0];
  return owner.base === self.base && owner.relDir === self.relDir ? undefined : owner;
}

/** Top-level `export` declarations that can carry a `$ref`-target copy. */
const DECLARATION_START = /^export (?:interface|const enum|enum|type) (\w+)\b/;

/** Type names a compiled module declares at its top level. */
function declaredNames(source: string): string[] {
  const names: string[] = [];
  for (const line of source.split('\n')) {
    const match = DECLARATION_START.exec(line);
    if (match) names.push(match[1]);
  }
  return names;
}

/**
 * Last line index of the top-level declaration that starts at `start`.
 *
 * Braces inside string / template literals (`close: "}"`, `` raw: `a}b` ``) are data,
 * not structure — as are braces inside comments, which property-level JSDoc emits
 * freely — so the scan tracks quote / block-comment state across lines and only counts
 * the braces outside both. A template literal's `${ … }` re-enters code, so its braces
 * (and any nested template/quote inside it) are tracked on a stack rather than treated
 * as template text; that keeps comment-looking text in a nested template literal — e.g.
 * `` `outer${`/*`}` `` — literal instead of an unterminated comment.
 */
export function declarationEnd(lines: string[], start: number): number {
  const isTypeAlias = /^export type \b/.test(lines[start]);
  let depth = 0;
  let inBlockComment = false;
  // Delimiter of the literal the scan is inside (`'`, `"` or a backtick), if any.
  let quote: string | undefined;
  // One entry per open template `${`: the count of `{` opened inside it, so its own
  // matching `}` resumes the template instead of closing the declaration.
  const interpolations: number[] = [];
  for (let i = start; i < lines.length; i++) {
    const line = lines[i];
    for (let j = 0; j < line.length; j++) {
      const ch = line[j];
      const next = line[j + 1];
      if (inBlockComment) {
        if (ch === '*' && next === '/') {
          inBlockComment = false;
          j += 1;
        }
      } else if (quote) {
        if (ch === '\\') j += 1;
        else if (quote === '`' && ch === '$' && next === '{') {
          interpolations.push(0);
          quote = undefined;
          j += 1;
        } else if (ch === quote) quote = undefined;
      } else if (ch === '/' && next === '/') {
        break;
      } else if (ch === '/' && next === '*') {
        inBlockComment = true;
        j += 1;
      } else if (ch === "'" || ch === '"' || ch === '`') {
        quote = ch;
      } else if (ch === '{') {
        const open = interpolations.length - 1;
        if (open >= 0) interpolations[open] += 1;
        depth += 1;
      } else if (ch === '}') {
        const open = interpolations.length - 1;
        if (open < 0) depth -= 1;
        else if (interpolations[open] > 0) {
          interpolations[open] -= 1;
          depth -= 1;
        } else {
          interpolations.pop();
          quote = '`';
        }
      }
    }
    if (depth > 0) continue;
    if (isTypeAlias ? line.trimEnd().endsWith(';') : line.trimEnd().endsWith('}')) {
      return i;
    }
  }
  return start;
}

/** First line index of the doc comment directly above `start` (or `start`). */
function declarationBlockStart(lines: string[], start: number): number {
  let i = start;
  while (i > 0) {
    const previous = lines[i - 1].trim();
    if (previous.endsWith('*/')) {
      i -= 1;
      while (i > 0 && !lines[i].trimStart().startsWith('/*')) i -= 1;
      continue;
    }
    if (previous.startsWith('//')) {
      i -= 1;
      continue;
    }
    break;
  }
  return i;
}

/**
 * Drop the inline copy of every `$ref` target that has a canonical standalone
 * module, and import the type from that module instead.
 *
 * `json-schema-to-typescript` inlines a referenced schema into each referencing
 * file, so one wire shape ends up declared several times (`WorkPoolEntry` inside
 * both `work-pool-entry` and `work-pool-list-response`). Keeping exactly one
 * declaration makes the standalone file the single canonical import path for
 * consumers. Names with no canonical home are left untouched.
 */
export function dedupeRefTargets(source: string, self: SchemaModule, modules: SchemaModule[]): string {
  const lines = source.split('\n');
  const dropped = new Set<number>();
  const imports: string[] = [];

  for (let i = 0; i < lines.length; i++) {
    if (dropped.has(i)) continue;
    const match = DECLARATION_START.exec(lines[i]);
    if (!match) continue;
    const start = i;
    const end = declarationEnd(lines, start);
    i = end;
    const owner = ownerOf(modules, match[1], self);
    if (!owner) continue;
    for (let j = declarationBlockStart(lines, start); j <= end; j++) dropped.add(j);
    // `path.posix.relative` yields a bare `works` for a descendant directory; a bare
    // specifier would resolve as a package name, so only `..`-prefixed and empty
    // results are used verbatim.
    const relativeDir = path.posix.relative(self.relDir, owner.relDir);
    const dirPrefix =
      relativeDir === '' || relativeDir.startsWith('..') ? relativeDir : `./${relativeDir}`;
    const from = dirPrefix === '' ? `./${owner.base}` : `${dirPrefix}/${owner.base}`;
    imports.push(`import type { ${match[1]} } from '${from}';`);
  }

  if (imports.length === 0) return source;

  const kept = lines.filter((_, index) => !dropped.has(index));
  while (kept.length > 0 && kept[kept.length - 1].trim() === '') kept.pop();
  const block = [...new Set(imports)].sort().join('\n');
  return `${block}\n\n${kept.join('\n')}`;
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

  await stageAndSwap(outDir, stageDir => buildGeneratedTree(localizedDir, srcSchemasDir, stageDir));
}

/**
 * Build `outDir` in a sibling staging directory and swap it into place only after
 * `build` succeeds, so a failed or aborted run cannot prune the existing tree.
 *
 * Replacement is *recoverable*, not atomic — Node exposes no portable directory
 * exchange, so this is two operations with a documented window. The committed tree is
 * retired to a sibling `.backup` path via rename (never a recursive delete of the only
 * copy), the staged tree is renamed into place, and the retired copy is dropped last.
 * Concretely, a hard kill leaves one of three states, all recoverable on the next run:
 * the kill happens before the swap → `outDir` is absent and the committed tree sits at
 * `.backup` (restored by `recoverInterruptedPublication`); after the swap → the new
 * tree is committed and the stale backup is dropped; or mid-swap → the backup is
 * restored in place. The committed path is thus briefly absent during a swap, but the
 * tree is never observably pruned or partially deleted.
 *
 * The entire recover → build → publish lifecycle is serialized per `outDir` by
 * `acquirePublicationLock`: two runs targeting the same output cannot share the
 * staging/backup siblings, and the loser fails visibly instead of publishing the
 * other run's incomplete tree.
 */
export async function stageAndSwap(
  outDir: string,
  build: (stageDir: string) => Promise<void> | void,
): Promise<void> {
  const releaseLock = acquirePublicationLock(outDir);
  try {
    const stageDir = `${outDir}.staging`;
    recoverInterruptedPublication(outDir);
    fs.rmSync(stageDir, { recursive: true, force: true });
    ensureDir(stageDir);
    try {
      await build(stageDir);
    } catch (err) {
      fs.rmSync(stageDir, { recursive: true, force: true });
      throw err;
    }
    publishTree(stageDir, outDir);
  } finally {
    releaseLock();
  }
}

/** Owner record written inside the lock directory: `<pid>:<nonce>:<startedAt>`. */
type LockOwner = string;

/**
 * Read the owner record from a lock directory, or `undefined` when it is missing or
 * unreadable (the directory was just created, or a run died before writing it). Used
 * only to name the holder in the failure message and to guard release — never to decide
 * whether a lock may be reclaimed.
 */
function readOwner(lockDir: string): LockOwner | undefined {
  try {
    return fs.readFileSync(path.join(lockDir, 'owner'), 'utf8').trim() || undefined;
  } catch {
    return undefined;
  }
}

/**
 * Publish a claim atomically: `mkdir` is the portable atomic-exclusive primitive
 * (`EEXIST` = held). The owner record is written inside the claimed directory
 * immediately after, so a live claim is always identifiable.
 *
 * @returns true when this claim now owns `lockDir`.
 */
function tryMkdirClaim(lockDir: string, token: string): boolean {
  try {
    fs.mkdirSync(lockDir);
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'EEXIST') return false;
    throw err;
  }
  try {
    fs.writeFileSync(path.join(lockDir, 'owner'), `${token}\n`);
  } catch (err) {
    // We own the directory but recorded no token, so nothing could ever release it —
    // drop it rather than strand the lock for manual removal.
    fs.rmSync(lockDir, { recursive: true, force: true });
    throw err;
  }
  return true;
}

/** Remove the lock directory only while it still carries this claim's owner record. */
function releaseOwned(lockDir: string, token: string): void {
  if (readOwner(lockDir) === token) {
    fs.rmSync(lockDir, { recursive: true, force: true });
  }
}

/**
 * Take the exclusive per-output-directory publication lock, or fail visibly when any
 * run already holds it. Node exposes no advisory file lock, so the lock directory
 * itself is the lock: acquisition is a single atomic `mkdir` and nothing else. There is
 * no stale takeover — a lock left behind by a crashed run (or one whose owner record is
 * missing) is never removed automatically, because no liveness check can exclude the
 * race between observing an owner and removing it. Any such lock fails the run visibly
 * and is removed by an operator, deliberately, once no codegen run is active.
 *
 * @returns the release hook; it only removes a lock this claim still owns.
 */
export function acquirePublicationLock(outDir: string): () => void {
  const lockDir = `${outDir}.lock`;
  const token = `${process.pid}:${randomUUID()}:${Date.now()}`;
  if (tryMkdirClaim(lockDir, token)) {
    return () => releaseOwned(lockDir, token);
  }
  const owner = readOwner(lockDir);
  throw new Error(
    `codegen: another codegen run holds the publication lock for ${outDir} ` +
      `(recorded owner ${owner ?? 'unknown'}); it serializes recover→build→publish and ` +
      `is never reclaimed automatically — if no codegen run is active, remove ${lockDir} manually`,
  );
}

/**
 * Restore or clear the recovery material a previous interrupted publication left, so a
 * later run never removes it unrecovered. No-op unless a backup exists.
 */
function recoverInterruptedPublication(outDir: string): void {
  const backupDir = `${outDir}.backup`;
  if (!fs.existsSync(backupDir)) return;
  if (fs.existsSync(outDir)) {
    // The replacement landed; the retired tree is stale litter.
    fs.rmSync(backupDir, { recursive: true, force: true });
    return;
  }
  // Killed before the replacement landed; the retired tree is the committed one.
  fs.renameSync(backupDir, outDir);
}

/** Retire the committed tree, land the staged one, then drop the retired copy. */
function publishTree(stageDir: string, outDir: string): void {
  const backupDir = `${outDir}.backup`;
  if (fs.existsSync(outDir)) fs.renameSync(outDir, backupDir);
  try {
    fs.renameSync(stageDir, outDir);
  } catch (err) {
    if (fs.existsSync(backupDir)) fs.renameSync(backupDir, outDir);
    throw err;
  }
  fs.rmSync(backupDir, { recursive: true, force: true });
}

/**
 * Build the tree under `outDir`, which is the staging dir during a normal run.
 * Regeneration is authoritative — the caller swaps the whole directory in, so a
 * failure here leaves the previously generated tree untouched.
 */
async function buildGeneratedTree(
  localizedDir: string,
  srcSchemasDir: string,
  outDir: string,
): Promise<void> {
  // v1.184 P1 T3 fix1: rewrite exact-string pattern discriminants across the
  // whole localized tree BEFORE compiling. compile() resolves cross-file $ref
  // bodies from disk, so rewriting only the root schema left nested oneOf
  // discriminants (actor_kind / owner kind) widened to plain `string`.
  const allRel = (await glob('**/*.schema.json', { cwd: localizedDir })).map(posix).sort();
  for (const rel of allRel) {
    const abs = path.join(localizedDir, ...rel.split('/'));
    const raw = readJSON<Record<string, unknown>>(abs);
    rewriteExactStringPatterns(raw);
    rewriteUnrestrictedJsonValues(raw);
    fs.writeFileSync(abs, `${JSON.stringify(raw, null, 2)}\n`);
  }
  const emitRel = allRel.filter(rel => !SKIP_LIST.has(rel));
  const skipped = allRel.filter(rel => SKIP_LIST.has(rel));

  // Canonical owner index for the `$ref`-target dedupe pass: one entry per emitted
  // schema, keyed by its basename-derived contract type name. The declared names are
  // filled in from the compiled output below — the pass may only point a type at a
  // module that actually declares it.
  const modules: SchemaModule[] = emitRel.map(rel => ({
    name: schemaToTypeName(path.basename(rel)),
    relDir: posix(path.dirname(rel)),
    base: path.basename(rel, '.schema.json'),
    declared: new Set<string>(),
  }));

  // 1. Synthetic common module: common definitions + SourceAnchor (skip-listed but referenced).
  await generateCommonTypesModule(localizedDir, outDir);

  // 2. Per-schema files: compile the whole tree first, so the dedupe pass sees every
  // module's declarations before any file is written.
  const compiled = new Map<string, string>();
  const versionRows: Array<{ typeName: string; version: number; relDir: string; base: string; rel: string }> = [];
  for (const rel of emitRel) {
    const fileName = path.basename(rel);
    const base = path.basename(rel, '.schema.json');
    const relDir = posix(path.dirname(rel));
    const typeName = schemaToTypeName(fileName);
    const self = modules.find(module => module.base === base && module.relDir === relDir);
    if (!self) {
      throw new Error(`codegen: no module index entry for ${rel}`);
    }
    const ts = await compileSchema(localizedDir, rel, typeName);
    compiled.set(rel, ts);
    for (const name of declaredNames(ts)) self.declared.add(name);
    const originalSchema = readJSON<Record<string, unknown>>(path.join(srcSchemasDir, rel));
    versionRows.push({ typeName, version: extractSchemaVersion(originalSchema), relDir, base, rel });
  }

  for (const rel of emitRel) {
    const base = path.basename(rel, '.schema.json');
    const relDir = posix(path.dirname(rel));
    const self = modules.find(module => module.base === base && module.relDir === relDir);
    if (!self) {
      throw new Error(`codegen: no module index entry for ${rel}`);
    }
    const ts = dedupeRefTargets(compiled.get(rel) ?? '', self, modules);
    writeFile(path.join(outDir, ...relDir.split('/'), `${base}.ts`), `${BANNER}\n\n${ts}\n`);
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
  rewriteExactStringPatterns(schema);
  const ts = await compile(schema, typeName, { ...COMPILE_OPTS, cwd: path.dirname(abs) });
  return ts.trim();
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

