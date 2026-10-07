import { describe, expect, it } from 'vitest';
import { compile } from 'json-schema-to-typescript';
import type { SchemaModule } from './ts-gen';
import { dedupeRefTargets, rewriteExactStringPatterns, rewriteUnrestrictedJsonValues } from './ts-gen';

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
});
