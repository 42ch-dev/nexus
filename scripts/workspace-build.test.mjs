import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { existsSync, watch } from 'node:fs';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const root = fileURLToPath(new URL('../', import.meta.url));
const run = promisify(execFile);

test('the root build produces shared dist directories without replacing them mid-build', async (t) => {
  const outputs = ['nexus-contracts', 'nexus-ui'].map((name) => {
    const dir = join(root, 'packages', name);
    const dist = join(dir, 'dist');
    // One creation, plus one initial removal if the checkout was already built.
    const allowedRenames = existsSync(dist) ? 2 : 1;
    const output = { name, dist, allowedRenames, renames: 0 };
    const watcher = watch(dir, (event, filename) => {
      if (event === 'rename' && filename === 'dist') output.renames++;
    });
    t.after(() => watcher.close());
    return output;
  });

  const buildError = await run('pnpm', ['run', 'build'], { cwd: root }).then(
    () => null,
    (error) => error,
  );
  for (const { name, dist, allowedRenames, renames } of outputs) {
    assert.ok(
      renames <= allowedRenames,
      `${name}/dist was removed or created ${renames} times (expected at most ${allowedRenames})`,
    );
    assert.ok(existsSync(join(dist, 'index.d.ts')), `${name} declarations were not built`);
  }
  assert.ifError(buildError);
});
