import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const lifecycleDist = join(serviceRoot, 'dist', 'lifecycle.js');
assert.ok(
  existsSync(lifecycleDist),
  'dist/lifecycle.js is missing — run `pnpm --filter @42ch/nexus-service run test` (or `… run build`) first',
);
const { startExecutionOwnerWithCompensation } = await import(lifecycleDist);

const IDENTITY = { creatorId: 'c1', workspaceSlug: 'default' };
const NOT_ESTABLISHED = { engine_epoch: null, provider_ready: false };
const ESTABLISHED = { engine_epoch: 7, provider_ready: true };

/** Run `work` while collecting everything the helper writes to stderr. */
async function captureStderr(work) {
  const original = process.stderr.write;
  const lines = [];
  process.stderr.write = (chunk) => {
    lines.push(String(chunk));
    return true;
  };
  try {
    return { result: await work(), lines: lines.join('') };
  } finally {
    process.stderr.write = original;
  }
}

describe('pre-IA home startup compensation (I-000124)', () => {
  test('selects the published workspace once and publishes the retried owner facts', async () => {
    const starts = [NOT_ESTABLISHED, ESTABLISHED];
    const selections = [];
    let startCalls = 0;
    const core = {
      startExecutionOwner: async () => starts[startCalls++],
      selectWorkspace: async (request) => {
        selections.push(request);
        return { creator_id: 'c1', workspace_slug: 'default' };
      },
    };

    const { result, lines } = await captureStderr(() =>
      startExecutionOwnerWithCompensation(core, IDENTITY),
    );

    assert.deepEqual(result, ESTABLISHED);
    assert.deepEqual(selections, [{ creator_id: 'c1', workspace_slug: 'default' }]);
    assert.equal(startCalls, 2);
    assert.match(lines, /pre-IA home: selected default workspace to start the execution owner/);
  });

  test('never compensates when the owner was established on the first start', async () => {
    const selections = [];
    let startCalls = 0;
    const core = {
      startExecutionOwner: async () => {
        startCalls += 1;
        return ESTABLISHED;
      },
      selectWorkspace: async (request) => {
        selections.push(request);
        return {};
      },
    };

    const { result, lines } = await captureStderr(() =>
      startExecutionOwnerWithCompensation(core, IDENTITY),
    );

    assert.deepEqual(result, ESTABLISHED);
    assert.equal(startCalls, 1);
    assert.deepEqual(selections, []);
    assert.equal(lines, '');
  });

  test('degrades to the tolerated owner shape when the selection rejects', async () => {
    let startCalls = 0;
    const core = {
      startExecutionOwner: async () => {
        startCalls += 1;
        return NOT_ESTABLISHED;
      },
      selectWorkspace: async () => {
        throw new Error('admission pin failed');
      },
    };

    const { result, lines } = await captureStderr(() =>
      startExecutionOwnerWithCompensation(core, IDENTITY),
    );

    assert.deepEqual(result, NOT_ESTABLISHED);
    assert.equal(startCalls, 1);
    assert.match(lines, /workspace selection compensation failed: admission pin failed/);
  });

  test('degrades when the retried owner start rejects', async () => {
    let startCalls = 0;
    const core = {
      startExecutionOwner: async () => {
        startCalls += 1;
        if (startCalls === 1) return NOT_ESTABLISHED;
        throw new Error('owner start failed');
      },
      selectWorkspace: async () => ({ creator_id: 'c1', workspace_slug: 'default' }),
    };

    const { result, lines } = await captureStderr(() =>
      startExecutionOwnerWithCompensation(core, IDENTITY),
    );

    assert.deepEqual(result, NOT_ESTABLISHED);
    assert.equal(startCalls, 2);
    assert.match(lines, /workspace selection compensation failed: owner start failed/);
  });
});
