import { describe, expect, it } from 'vitest';
import { compile } from 'json-schema-to-typescript';
import { rewriteExactStringPatterns } from './ts-gen';

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
