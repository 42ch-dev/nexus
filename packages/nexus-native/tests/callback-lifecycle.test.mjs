import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const require = createRequire(import.meta.url);
const nodePath = join(__dirname, '..', 'native', 'nexus_core_node.node');
const binding = require(nodePath);

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-callback-'));
  const seed = spawnSync('cargo', ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home], {
    cwd: root,
  });
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

function openWithProviders(providers) {
  const home = seedHome();
  binding.registerProviderCallbacks(providers);
  return binding.open(JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }));
}

describe('callback lifecycle', { concurrency: 1 }, () => {

test('sync throw is sanitized', async () => {
  const core = await openWithProviders({
    call: () => {
      throw new Error('sync-boom');
    },
    next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
  });
  await assert.rejects(
    () => core.providerCall(new TextEncoder().encode(JSON.stringify({ request_id: '1', method: 'probe', payload: {} }))),
  );
  await core.close();
});

test('rejected promise is sanitized', async () => {
  const core = await openWithProviders({
    call: async () => Promise.reject(new Error('reject-boom')),
    next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
  });
  await assert.rejects(
    () => core.providerCall(new TextEncoder().encode(JSON.stringify({ request_id: '2', method: 'probe', payload: {} }))),
  );
  await core.close();
});

test('callback after close is rejected', async () => {
  const core = await openWithProviders({
    call: async () => JSON.stringify({ request_id: '3', ok: true }),
    next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
  });
  await core.close();
  await assert.rejects(
    () => core.providerCall(new TextEncoder().encode(JSON.stringify({ request_id: '4', method: 'probe', payload: {} }))),
  );
});

test('reentrant provider graph allowed', async () => {
  let depth = 0;
  const core = await openWithProviders({
    call: async (json) => {
      depth += 1;
      if (depth === 1) {
        await core.providerCall(
          new TextEncoder().encode(JSON.stringify({ request_id: 'r', method: 'probe', payload: {} })),
        );
      }
      return JSON.stringify({ request_id: JSON.parse(json).request_id, ok: true });
    },
    next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
  });
  await core.providerCall(new TextEncoder().encode(JSON.stringify({ request_id: '5', method: 'probe', payload: {} })));
  await core.close();
});
});
