import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { parseAdmittedRecipe, parseProcessIdentity } from '../dist/index.js';

function resolvePython() {
  const which = execFileSync('which', ['python3'], { encoding: 'utf8' }).trim();
  return realpathSync(which);
}

describe('parseAdmittedRecipe boundary', () => {
  test('rejects relative cwd and non-absolute executable', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'nexus-recipe-'));
    const base = {
      provider_id: 'mock',
      recipe_generation: 'g1',
      executable: resolvePython(),
      args: [],
      env: {},
    };
    assert.throws(
      () => parseAdmittedRecipe({ recipe: { ...base, cwd: 'relative/path' } }),
      /invalid_recipe/,
    );
    assert.throws(
      () => parseAdmittedRecipe({ recipe: { ...base, cwd: workspace, executable: 'python3' } }),
      /invalid_recipe/,
    );
  });

  test('accepts canonical absolute cwd directory', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'nexus-recipe-'));
    const recipe = parseAdmittedRecipe({
      recipe: {
        provider_id: 'mock',
        recipe_generation: 'g1',
        executable: resolvePython(),
        args: [],
        env: {},
        cwd: resolve(workspace),
      },
    });
    assert.equal(recipe.cwd, resolve(workspace));
  });

  test('rejects malformed process_identity shapes', () => {
    assert.throws(() => parseProcessIdentity({ pid: -1 }), /invalid_recipe/);
    assert.throws(() => parseProcessIdentity({ pid: 1, extra: true }), /invalid_recipe/);
    assert.throws(() => parseProcessIdentity({ pid: 1.5 }), /invalid_recipe/);
    assert.equal(parseProcessIdentity(null), null);
    assert.deepEqual(parseProcessIdentity({ pid: 42, process_birth: '1', group_id: '42' }), {
      pid: 42,
      process_birth: '1',
      group_id: '42',
    });
  });

  test('rejects direct malformed callback payload without nested recipe', () => {
    assert.throws(() => parseAdmittedRecipe({ provider_id: 'x' }), /invalid_recipe/);
  });
});
