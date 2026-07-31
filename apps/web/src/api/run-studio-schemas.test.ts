/**
 * toInvocationSchema structural guard (V1.147 P1, qc1 W-003 fix).
 *
 * The adapter narrows the generated `ModuleDetail.schemas.invocation` open
 * index signature onto the `@42ch/nexus-ui` structural InvocationSchema; it
 * must reject shape drift instead of silently passing it through.
 */
import { describe, expect, it } from 'vitest';

import { toInvocationSchema } from '@/api/run-studio-schemas';

describe('toInvocationSchema', () => {
  it('returns null for non-object values', () => {
    expect(toInvocationSchema(null)).toBeNull();
    expect(toInvocationSchema(undefined)).toBeNull();
    expect(toInvocationSchema('object')).toBeNull();
    expect(toInvocationSchema(42)).toBeNull();
    expect(toInvocationSchema(['properties'])).toBeNull();
  });

  it('rejects a schema whose type is not object', () => {
    expect(toInvocationSchema({ type: 'array', items: { type: 'string' } })).toBeNull();
    expect(toInvocationSchema({ type: 'string' })).toBeNull();
  });

  it('rejects malformed properties and required shapes', () => {
    expect(toInvocationSchema({ type: 'object', properties: [] })).toBeNull();
    expect(toInvocationSchema({ type: 'object', properties: 'nope' })).toBeNull();
    expect(toInvocationSchema({ type: 'object', properties: null })).toBeNull();
    expect(toInvocationSchema({ type: 'object', required: 'attacker_id' })).toBeNull();
    expect(toInvocationSchema({ type: 'object', required: ['ok', 42] })).toBeNull();
  });

  it('narrows a valid schema and drops unknown fields', () => {
    const narrow = toInvocationSchema({
      type: 'object',
      title: 'Combat invocation',
      properties: {
        attacker_id: { type: 'string', description: 'Who attacks' },
        rounds: { type: 'integer', minimum: 1 },
      },
      required: ['attacker_id'],
      'x-nexus-extra': true,
    });
    expect(narrow).toEqual({
      type: 'object',
      properties: {
        attacker_id: { type: 'string', description: 'Who attacks' },
        rounds: { type: 'integer', minimum: 1 },
      },
      required: ['attacker_id'],
    });
  });

  it('accepts a schema without an explicit type (properties-only fragment)', () => {
    expect(toInvocationSchema({ properties: { note: { type: 'string' } } })).toEqual({
      properties: { note: { type: 'string' } },
    });
  });

  it('accepts an empty properties object (form renders its empty state)', () => {
    expect(toInvocationSchema({ type: 'object', properties: {} })).toEqual({
      type: 'object',
      properties: {},
    });
  });
});
