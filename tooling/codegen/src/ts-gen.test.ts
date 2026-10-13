import { describe, expect, it } from 'vitest';
import { compile } from 'json-schema-to-typescript';
import { spawnSync } from 'child_process';
import fs from 'fs';
import os from 'os';
import path from 'path';
import type { SchemaModule } from './ts-gen';
import {
  acquirePublicationLock,
  declarationEnd,
  dedupeRefTargets,
  rewriteExactStringPatterns,
  rewriteUnrestrictedJsonValues,
  stageAndSwap,
} from './ts-gen';

describe('nested discriminant literals', () => {
  it('rewrites exact patterns inside inlined oneOf $ref bodies', async () => {
    const schema = {
      title: 'ViewRequest',
      type: 'object',
      required: ['actor_ref'],
      properties: {
        actor_ref: {
          oneOf: [
            {
              title: 'CreatorActorRef',
              type: 'object',
              required: ['actor_kind', 'creator_id'],
              properties: {
                actor_kind: { type: 'string', pattern: '^creator$' },
                creator_id: { type: 'string' },
              },
            },
          ],
        },
      },
    };
    rewriteExactStringPatterns(schema);
    const ts = await compile(schema as never, 'ViewRequest', {
      bannerComment: '',
      unreachableDefinitions: true,
      declareExternallyReferenced: true,
    });
    expect(ts).toContain('actor_kind: "creator"');
    expect(ts).not.toMatch(/actor_kind: string/);
  });
});

describe('unrestricted JSON values', () => {
  it('compiles typeless schemas to unknown, not an object index signature', async () => {
    const schema = {
      title: 'Probe',
      type: 'object',
      required: ['result'],
      additionalProperties: false,
      properties: {
        // Annotation-only subschema — an unrestricted "any JSON value" field
        // (mirrors core-tool-execute-response.result / serde_json::Value).
        result: { description: 'Tool-produced JSON result; any JSON value.' },
        // Constraint-bearing subschemas must stay untouched.
        typed: { type: 'object', description: 'still an object' },
        nested: { type: 'array', items: { description: 'element is any JSON' } },
      },
    };
    rewriteUnrestrictedJsonValues(schema);
    const ts = await compile(schema as never, 'Probe', {
      bannerComment: '',
      strictIndexSignatures: true,
    });
    expect(ts).toContain('result: unknown;');
    expect(ts).toContain('nested?: unknown[];');
    expect(ts).toContain('typed?: {');
    expect(ts).not.toContain('result: {');
  });

  it('leaves annotation example data and property names alone', () => {
    const schema: Record<string, unknown> = {
      // `default`/`examples` values are opaque data, not schemas.
      default: {},
      examples: [{ a: 1 }],
      // `properties` keys are property names — a "title" property is not the
      // title annotation keyword.
      properties: { title: { type: 'string' } },
    };
    rewriteUnrestrictedJsonValues(schema);
    expect(schema.default).toEqual({});
    expect(schema.examples).toEqual([{ a: 1 }]);
    expect(schema.properties).toEqual({ title: { type: 'string' } });
  });
});

describe('$ref target dedupe', () => {
  function schemaModule(
    relDir: string,
    name: string,
    declared: string[],
  ): SchemaModule {
    return {
      name,
      relDir,
      base: name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase(),
      declared: new Set(declared),
    };
  }

  const listResponse = schemaModule('core/works', 'WorkPoolListResponse', [
    'WorkPoolListResponse',
    'WorkPoolEntry',
  ]);
  const entry = schemaModule('core/works', 'WorkPoolEntry', ['WorkPoolEntry']);

  it('drops the inlined copy and imports the canonical module', () => {
    const source = [
      '/**',
      ' * Offset-paginated page.',
      ' */',
      'export interface WorkPoolListResponse {',
      '  entries: WorkPoolEntry[];',
      '}',
      '/**',
      ' * One authoring-pool entry.',
      ' */',
      'export interface WorkPoolEntry {',
      '  entry_id: string;',
      '}',
    ].join('\n');

    const out = dedupeRefTargets(source, listResponse, [listResponse, entry]);

    expect(out).toContain("import type { WorkPoolEntry } from './work-pool-entry';");
    expect(out).toContain('export interface WorkPoolListResponse {');
    expect(out).toContain('entries: WorkPoolEntry[];');
    expect(out).not.toContain('export interface WorkPoolEntry');
    expect(out).not.toContain('One authoring-pool entry.');
  });

  it('prefixes a descendant-directory import with ./', () => {
    // `path.posix.relative('core', 'core/works')` is a bare `works`; emitting
    // `works/work-pool-entry` would resolve as a package specifier.
    const parent = schemaModule('core', 'WorkPoolListResponse', [
      'WorkPoolListResponse',
      'WorkPoolEntry',
    ]);
    const child = schemaModule('core/works', 'WorkPoolEntry', ['WorkPoolEntry']);
    const source = [
      'export interface WorkPoolListResponse {',
      '  entries: WorkPoolEntry[];',
      '}',
      'export interface WorkPoolEntry {',
      '  entry_id: string;',
      '}',
    ].join('\n');

    const out = dedupeRefTargets(source, parent, [parent, child]);

    expect(out).toContain("import type { WorkPoolEntry } from './works/work-pool-entry';");
    expect(out).not.toContain("from 'works/");
  });

  it('keeps a declaration whose canonical module does not declare it', () => {
    // `Character` prefixes `CharacterActorRef`, but `character.ts` never declares it,
    // so it must not be treated as the canonical home.
    const character = schemaModule('domain', 'Character', ['Character']);
    const actorRef = schemaModule('domain', 'CharacterActorRef', ['CharacterActorRef']);
    const source = ['export interface CharacterActorRef {', '  actor_kind: string;', '}'].join('\n');

    expect(dedupeRefTargets(source, actorRef, [character, actorRef])).toBe(source);
  });

  it('removes a declaration whose body holds a nested template literal', () => {
    // The nested template's backtick must not close the outer one; otherwise the
    // literal `/*` reads as a block comment and the declaration end is missed.
    const pattern = schemaModule('core/works', 'Pattern', ['Pattern']);
    const listResponse = schemaModule('core/works', 'PatternListResponse', [
      'PatternListResponse',
      'Pattern',
    ]);
    const source = [
      'export interface PatternListResponse {',
      '  items: Pattern[];',
      '}',
      'export interface Pattern {',
      '  raw: `outer${`/*`}`;',
      '  n: number;',
      '}',
    ].join('\n');

    const out = dedupeRefTargets(source, listResponse, [listResponse, pattern]);

    expect(out).toContain("import type { Pattern } from './pattern';");
    // The complete declaration is gone and the next declaration survives.
    expect(out).not.toContain('export interface Pattern {');
    expect(out).not.toContain('raw: `outer${`/*`}`;');
    expect(out).toContain('export interface PatternListResponse {');
    expect(out).toContain('  items: Pattern[];');
  });
});

describe('declarationEnd brace scan', () => {
  it('ignores braces inside string literals', () => {
    const lines = [
      'export interface BracedQuery {',
      '  close: "}";',
      '  count: number;',
      '}',
      'export interface Next {',
      '}',
    ];
    // A naive counter takes the `}` in `"}"` as the declaration close (line 1).
    expect(declarationEnd(lines, 0)).toBe(3);
  });

  it('ignores braces inside template literals and comments', () => {
    const lines = [
      'export type Pattern = {',
      '  /**',
      "   * Closes with a literal `}` -- don't count it.",
      '   */',
      '  raw: `a}b`;',
      '  n: number;',
      '};',
    ];
    expect(declarationEnd(lines, 0)).toBe(6);
  });

  it('keeps comment-looking text inside a nested template literal literal', () => {
    const lines = [
      'export interface Pattern {',
      '  raw: `outer${`/*`}`;',
      '  n: number;',
      '}',
    ];
    expect(declarationEnd(lines, 0)).toBe(3);
  });
});

describe('stage-and-swap tree write', () => {
  function scratch(tag: string): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), `nexus-ts-gen-${tag}-`));
  }

  it('keeps the existing tree when the build throws', async () => {
    const root = scratch('abort');
    const outDir = path.join(root, 'generated');
    fs.mkdirSync(outDir);
    fs.writeFileSync(path.join(outDir, 'kept.ts'), '// committed');

    await expect(
      stageAndSwap(outDir, stageDir => {
        fs.writeFileSync(path.join(stageDir, 'half.ts'), '// partial');
        throw new Error('boom');
      }),
    ).rejects.toThrow('boom');

    expect(fs.existsSync(path.join(outDir, 'kept.ts'))).toBe(true);
    expect(fs.existsSync(path.join(outDir, 'half.ts'))).toBe(false);
    expect(fs.existsSync(`${outDir}.staging`)).toBe(false);
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('replaces the existing tree on success', async () => {
    const root = scratch('swap');
    const outDir = path.join(root, 'generated');
    fs.mkdirSync(outDir);
    fs.writeFileSync(path.join(outDir, 'stale.ts'), '// stale');

    await stageAndSwap(outDir, stageDir => {
      fs.writeFileSync(path.join(stageDir, 'fresh.ts'), '// fresh');
    });

    expect(fs.existsSync(path.join(outDir, 'stale.ts'))).toBe(false);
    expect(fs.existsSync(path.join(outDir, 'fresh.ts'))).toBe(true);
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('restores the previous tree when publication fails', async () => {
    const root = scratch('publish');
    const outDir = path.join(root, 'generated');
    fs.mkdirSync(outDir);
    fs.writeFileSync(path.join(outDir, 'kept.ts'), '// committed');

    // Force the publish rename to fail by dropping the built tree before the swap.
    await expect(
      stageAndSwap(outDir, stageDir => {
        fs.rmSync(stageDir, { recursive: true, force: true });
      }),
    ).rejects.toThrow();

    expect(fs.existsSync(path.join(outDir, 'kept.ts'))).toBe(true);
    expect(fs.existsSync(`${outDir}.backup`)).toBe(false);
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('recovers an interrupted publication before another run publishes', async () => {
    const root = scratch('recover');
    const outDir = path.join(root, 'generated');
    // Simulate a kill after the old tree was retired but before the swap landed.
    fs.mkdirSync(`${outDir}.backup`, { recursive: true });
    fs.writeFileSync(path.join(`${outDir}.backup`, 'kept.ts'), '// committed');

    await expect(
      stageAndSwap(outDir, () => {
        throw new Error('boom');
      }),
    ).rejects.toThrow('boom');

    expect(fs.existsSync(path.join(outDir, 'kept.ts'))).toBe(true);
    expect(fs.existsSync(`${outDir}.backup`)).toBe(false);
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('refuses a second run over the same output while the first is building', async () => {
    const root = scratch('overlap');
    const outDir = path.join(root, 'generated');
    fs.mkdirSync(outDir);

    let releaseFirst!: () => void;
    const firstBuild = new Promise<void>(resolve => {
      releaseFirst = resolve;
    });
    const first = stageAndSwap(outDir, () => firstBuild);
    // The first run holds the output lock for its whole build, so the second run must
    // fail visibly rather than clear the staging tree out from under it.
    await expect(stageAndSwap(outDir, () => {})).rejects.toThrow(/another codegen run/);

    releaseFirst();
    await first;
    expect(fs.existsSync(`${outDir}.lock`)).toBe(false);

    // The lock is released with the first run: a follow-up run proceeds normally.
    await stageAndSwap(outDir, stageDir => {
      fs.writeFileSync(path.join(stageDir, 'second.ts'), '// second');
    });
    expect(fs.existsSync(path.join(outDir, 'second.ts'))).toBe(true);
    fs.rmSync(root, { recursive: true, force: true });
  });
});

describe('publication lock acquisition', () => {
  function scratch(tag: string): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), `nexus-ts-lock-${tag}-`));
  }

  /** A pid guaranteed dead: a child that has already exited (await-free, reaped). */
  function deadPid(): number {
    const child = spawnSync(process.execPath, ['-e', '']);
    if (!child.pid || child.pid === process.pid) {
      throw new Error('failed to spawn a helper process for a dead pid');
    }
    return child.pid;
  }

  /** Seed a lock directory holding an owner record. */
  function seedLock(lockDir: string, token: string): void {
    fs.mkdirSync(lockDir, { recursive: true });
    fs.writeFileSync(path.join(lockDir, 'owner'), `${token}\n`);
  }

  it('refuses a second contender against a held lock and removes nothing', () => {
    const root = scratch('held');
    const outDir = path.join(root, 'generated');
    const lockDir = `${outDir}.lock`;

    const release = acquirePublicationLock(outDir);
    const ownerBefore = fs.readFileSync(path.join(lockDir, 'owner'), 'utf8');

    // The competitor fails visibly and must not touch the holder's lock or its record.
    expect(() => acquirePublicationLock(outDir)).toThrow(/another codegen run/);
    expect(fs.readFileSync(path.join(lockDir, 'owner'), 'utf8')).toBe(ownerBefore);
    expect(fs.existsSync(lockDir)).toBe(true);

    release();
    expect(fs.existsSync(lockDir)).toBe(false);
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('leaves a stale lock from a dead pid in place and fails with the manual-removal message', () => {
    const root = scratch('stale');
    const outDir = path.join(root, 'generated');
    const lockDir = `${outDir}.lock`;
    const token = `${deadPid()}:stale:0`;
    seedLock(lockDir, token);

    // A dead owner is never probed or reclaimed: the run fails visibly instead.
    expect(() => acquirePublicationLock(outDir)).toThrow(/another codegen run/);
    expect(() => acquirePublicationLock(outDir)).toThrow(`remove ${lockDir} manually`);
    expect(fs.readFileSync(path.join(lockDir, 'owner'), 'utf8').trim()).toBe(token);
    expect(fs.existsSync(lockDir)).toBe(true);
    fs.rmSync(root, { recursive: true, force: true });
  });
});
