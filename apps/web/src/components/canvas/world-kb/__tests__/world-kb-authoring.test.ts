/**
 * World KB authoring vocabulary + pure module serialization — extraction
 * baseline (p4 T2 / `R-V1203P2QC1-S001`).
 *
 * The holder vocabulary and the `modules.mental` / `modules.belief` builders
 * moved out of the React component module; these cases pin their
 * inputs/outputs at the new lower-layer home and pin the import surface
 * (wire-contract types only — no component dependency), so the move stays
 * behaviour-neutral.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  HOLDER_BLOCK_TYPES,
  MENTAL_FIELD_ORDER,
  buildBeliefValue,
  buildMentalValue,
  modulesPatchFromForm,
  type BeliefRowForm,
  type EntityEditForm,
} from '../world-kb-authoring';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const AUTHORING_SOURCE = readFileSync(path.resolve(__dirname, '../world-kb-authoring.ts'), 'utf8');
const TIMELINE_INSPECTOR_SOURCE = readFileSync(
  path.resolve(__dirname, '../../timeline-canvas/timeline-inspector.tsx'),
  'utf8',
);

function emptyForm(overrides: Partial<EntityEditForm> = {}): EntityEditForm {
  return {
    title: 'Bo',
    bodyText: '',
    aliasesText: '',
    block_type: 'character',
    mental: Object.fromEntries(MENTAL_FIELD_ORDER.map((key) => [key, ''])) as EntityEditForm['mental'],
    mentalRawText: null,
    mentalExtras: [],
    beliefs: [],
    beliefRawText: null,
    ...overrides,
  };
}

function beliefRow(overrides: Partial<BeliefRowForm> = {}): BeliefRowForm {
  return {
    holder: '',
    proposition: '',
    order: '',
    truth: '',
    access: '',
    representation: '',
    content_type: '',
    source: '',
    context: '',
    ...overrides,
  };
}

describe('shared authoring vocabulary', () => {
  it('keeps the holder kinds and the locked mental field order unchanged', () => {
    expect(HOLDER_BLOCK_TYPES).toEqual(['character', 'faction', 'organization']);
    expect([...MENTAL_FIELD_ORDER]).toEqual([
      'identity',
      'beliefs',
      'attention',
      'goals',
      'intentions',
      'emotions',
      'dispositions',
      'norms',
      'constraints',
    ]);
  });
});

describe('buildMentalValue', () => {
  it('reports unparseable field text under the frozen mental.<field> key shape', () => {
    const form = emptyForm();
    form.mental.identity = '{not json';

    const built = buildMentalValue(form, (field) => `bad ${field}`);

    expect(built.errors).toEqual({ 'mental.identity': 'bad identity' });
    expect(built.value).toEqual({});
  });

  it('keeps an authored __proto__ extra as an own data property (QC1-F002)', () => {
    const form = emptyForm({ mentalExtras: [{ key: '__proto__', text: '{"polluted":true}' }] });

    const built = buildMentalValue(form, (field) => `bad ${field}`);

    expect(built.errors).toEqual({});
    expect(Object.getOwnPropertyDescriptor(built.value, '__proto__')).toBeDefined();
    expect(Object.keys(JSON.parse(JSON.stringify(built.value)))).toContain('__proto__');
  });
});

describe('buildBeliefValue', () => {
  it('maps every emitted element back to its form row (QC1-F003 wire indexes)', () => {
    const form = emptyForm({ beliefs: [beliefRow(), beliefRow({ proposition: 'The tide turns' })] });

    const built = buildBeliefValue(form, (field) => `bad ${field}`);

    expect(built.value).toEqual([{ proposition: 'The tide turns' }]);
    expect(built.wireToForm).toEqual([1]);
  });

  it('round-trips an untouched seeded row verbatim, unknown inner keys included', () => {
    const form = emptyForm({
      beliefs: [beliefRow({ holder: 'kb-bo', original: { holder: 'kb-bo', unknown_inner: 'kept' } })],
    });

    const built = buildBeliefValue(form, (field) => `bad ${field}`);

    expect(built.value).toEqual([{ holder: 'kb-bo', unknown_inner: 'kept' }]);
    expect(built.wireToForm).toEqual([0]);
  });
});

describe('modulesPatchFromForm', () => {
  it('emits only the requested dirty dialects (L2-T2-001 reapply scoping)', () => {
    const form = emptyForm();
    form.mental.identity = '{"role":"harbor_master"}';
    form.beliefs = [beliefRow({ proposition: 'The tide turns' })];

    expect(modulesPatchFromForm(form, ['mental'])).toEqual({ mental: { identity: { role: 'harbor_master' } } });
    expect(modulesPatchFromForm(form, ['belief'])).toEqual({ belief: [{ proposition: 'The tide turns' }] });
    expect(Object.keys(modulesPatchFromForm(form, ['mental', 'belief']) ?? {})).toEqual(['mental', 'belief']);
    expect(modulesPatchFromForm(form, [])).toBeUndefined();
  });
});

describe('import surface (R-V1203P2QC1-S001)', () => {
  it('depends on wire-contract types only — no React, no component module', () => {
    const importLines = AUTHORING_SOURCE.split('\n').filter((line) => /^\s*import\b/.test(line));

    expect(importLines.length).toBeGreaterThan(0);
    for (const line of importLines) {
      expect(line).toMatch(/from '@42ch\/nexus-contracts';$/);
    }
  });

  it('is where the timeline inspector takes the holder vocabulary from', () => {
    expect(TIMELINE_INSPECTOR_SOURCE).not.toMatch(/from '\.\.\/world-kb\/entity-inspector'/);
    expect(TIMELINE_INSPECTOR_SOURCE).toContain(
      "import { HOLDER_BLOCK_TYPES } from '../world-kb/world-kb-authoring';",
    );
  });
});
