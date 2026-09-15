import { describe, expect, it } from 'vitest';
import { compile } from 'json-schema-to-typescript';
import { rewriteExactStringPatterns, rewriteUnrestrictedJsonValues } from './ts-gen';

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
