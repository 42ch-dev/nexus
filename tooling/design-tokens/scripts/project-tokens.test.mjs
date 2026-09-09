/**
 * @nexus/design-tokens — compiler regression tests.
 *
 * Defend the parser/resolver edges that a plausible compiler defect would
 * break: missing references, cyclic references, light/dark leaf-parity, and
 * that compound SOUL / embedded color-mix values project their scalar member
 * instead of leaking "{...}" text or "[object Object]".
 */
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveScalar, projectDesign, loadDesignPair } from './project-tokens.mjs';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const lightFixture = {
  version: '0.0.1',
  colors: {
    'blue-700': '#3263C7',
    'brand-white': '#FFFFFF',
  },
  spacing: { 'space-2': 8 },
  typography: {
    'button-14': { fontFamily: 'system-ui', fontSize: 14, lineHeight: 1.25 },
  },
  elevation: {
    'elevation-1': '#181F2910',
    'shadow-card': '{elevation.elevation-1}',
  },
  components: {
    'badge-status-pill': {
      soft: {
        running: {
          backgroundColor: 'color-mix(in srgb, {colors.blue-700} 16%, transparent)',
          textColor: '{colors.blue-700}',
        },
      },
    },
    'soul-narrative-prose': '{typography.button-14} @ {colors.brand-white}',
  },
};

const baseDark = structuredClone(lightFixture);
baseDark.colors['blue-700'] = '#8EB1F4';

function scalarFor(value, doc, sourcePath = 'test') {
  const index = new Map();
  const walk = (node, prefix = '') => {
    for (const [k, v] of Object.entries(node)) {
      const p = prefix ? `${prefix}.${k}` : k;
      index.set(p, v);
      if (v !== null && typeof v === 'object' && !Array.isArray(v)) walk(v, p);
    }
  };
  walk(doc);
  return resolveScalar(value, doc, index, sourcePath);
}

test('embedded color-mix reference resolves to the scalar hex', () => {
  const value = 'color-mix(in srgb, {colors.blue-700} 16%, transparent)';
  const out = scalarFor(value, baseDark);
  assert.equal(out, 'color-mix(in srgb, #8EB1F4 16%, transparent)');
});

test('whole-role object reference rejects as non-scalar (no object serialization)', () => {
  const value = '{typography.button-14}';
  assert.throws(() => scalarFor(value, baseDark), /non-scalar value/);
});

test('compound SOUL recipe projects the color member, not the object', () => {
  const value = '{typography.button-14} @ {colors.brand-white}';
  const out = scalarFor(value, baseDark);
  assert.equal(out, '#FFFFFF');
  assert.ok(!out.includes('[object Object]'));
  assert.ok(!out.includes('{typography.'));
});

test('missing reference throws', () => {
  const value = '{spacing.missing-step}';
  assert.throws(() => scalarFor(value, baseDark), /unresolved reference/);
});

test('cyclic reference throws', () => {
  const doc = structuredClone(baseDark);
  doc.elevation['shadow-a'] = '{elevation.shadow-b}';
  doc.elevation['shadow-b'] = '{elevation.shadow-a}';
  assert.throws(() => scalarFor('{elevation.shadow-a}', doc), /cyclic reference/);
});

test('differing light/dark leaf paths are rejected', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'nexus-token-'));
  const light = structuredClone(baseDark);
  light.colors['extra-light-only'] = '#000';
  await writeFile(join(dir, 'DESIGN.md'), `---\n${Object.entries(light).map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`).join('\n')}\n---\n`);
  await writeFile(join(dir, 'DESIGN.dark.md'), `---\n${Object.entries(baseDark).map(([k, v]) => `${k}: ${typeof v === 'object' ? JSON.stringify(v) : String(v)}`).join('\n')}\n---\n`);
  await assert.rejects(() => loadDesignPair(dir), /leaf-path parity mismatch/);
  await rm(dir, { recursive: true, force: true });
});

test('projectDesign emits a stable deterministic double-theme css block', async () => {
  const realPair = await loadDesignPair(process.cwd());
  const out = projectDesign(realPair);
  assert.ok(out.css.includes(':root {'));
  assert.ok(out.css.includes('.dark {'));
  const second = projectDesign(realPair);
  assert.equal(out.css, second.css);
});
