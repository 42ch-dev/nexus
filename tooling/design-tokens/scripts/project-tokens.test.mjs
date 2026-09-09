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

test('null mapped scalar fails closed (no silent empty omission)', async () => {
  const doc = structuredClone(baseDark);
  doc.colors['brand-white'] = null;
  assert.throws(() => scalarFor(doc.colors['brand-white'], doc), /fail closed|empty value/);
  // Use the real SSOT pair so a null brand leaf fails projectDesign (not a
  // missing-source-path artifact of the minimal fixture).
  const real = await loadDesignPair(process.cwd());
  const light = structuredClone(real.light);
  light.colors['brand-white'] = null;
  assert.throws(() => projectDesign({ light, dark: structuredClone(real.dark) }), /fail closed|empty value/);
});

test('empty mapped scalar string fails closed (no silent var drop)', async () => {
  const doc = structuredClone(baseDark);
  doc.colors['blue-700'] = '';
  assert.throws(() => scalarFor(doc.colors['blue-700'], doc), /empty scalar/);
  const real = await loadDesignPair(process.cwd());
  const light = structuredClone(real.light);
  light.colors['brand-cyan'] = '';
  assert.throws(() => projectDesign({ light, dark: structuredClone(real.dark) }), /fail closed|empty scalar/);
});

test('missing brand snapshot key fails projectDesign (no empty generated literal)', async () => {
  const real = await loadDesignPair(process.cwd());
  const light = structuredClone(real.light);
  const dark = structuredClone(real.dark);
  // Remove from BOTH themes so leaf-parity holds and the brand-snapshot
  // existence check is what fails closed (not a parity artifact).
  delete light.colors['brand-white'];
  delete dark.colors['brand-white'];
  // Removing brand-white is a fail-closed failure: either the brand-snapshot
  // existence check, an unresolved ref elsewhere, or an empty-value guard —
  // projectDesign must never emit an empty generated literal. Assert generic
  // throw (any of these fail-closed paths is a correct rejection).
  assert.throws(() => projectDesign({ light, dark }), /fail closed|empty value|brand snapshot missing|unresolved reference/);
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

test('projection emits known theme-specific, reference, and compound outputs', async () => {
  // Known input (committed DESIGN.md / DESIGN.dark.md SSOT) -> known output:
  // assert the exact projected CSS strings for (a) distinct theme-specific
  // values, (b) an alias that resolves through a reference, and (c) a compound
  // recipe that projects its resolved color member. Without snapshotting the
  // whole fixture, these guard that a plausible compiler defect (defaulting a
  // value, dropping a reference, leaking "{ref}" / "[object Object]") would
  // change the emitted string and fail the test.
  const realPair = await loadDesignPair(process.cwd());
  const out = projectDesign(realPair);
  assert.ok(out.css.includes(':root {'));
  assert.ok(out.css.includes('.dark {'));
  // Theme-specific distinct values (blue-700 differs by theme in the SSOT).
  assert.ok(out.css.includes('--color-blue-700: #3263C7;'));
  assert.ok(out.css.includes('--color-blue-700: #8EB1F4;'));
  // Reference edge: shadow-card is an alias onto elevation-1; the projected
  // string carries the resolved shadow value, not "{elevation.elevation-1}".
  assert.ok(out.css.includes('--shadow-card: 0 1px 2px #181F2910;'));
  assert.ok(out.css.includes('--shadow-card: 0 1px 2px #080B1140;'));
  assert.ok(!out.css.includes('{elevation.'));
  // Compound edge: soul-narrative-prose "{typography.X} @ {colors.gray-900}"
  // projects the referenced color member, never the source or an object.
  assert.ok(out.css.includes('--color-soul-narrative-prose: #2B3441;'));
  assert.ok(out.css.includes('--color-soul-narrative-prose: #DDE2E9;'));
  assert.ok(!out.css.includes('{typography.'));
  assert.ok(!out.css.includes('[object Object]'));
});
