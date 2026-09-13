import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { Worker } from 'node:worker_threads';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const require = createRequire(import.meta.url);
const nodePath = join(__dirname, '..', 'native', 'nexus_core_node.node');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-callback-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

function parseProviderCall(payload) {
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

function providerReply(requestId) {
  return JSON.stringify({
    request_id: requestId,
    ok: true,
    operation_id: null,
    session_id: null,
    health: null,
    error: null,
  });
}

function providerCallBuffer(requestId, extra = {}) {
  return new TextEncoder().encode(
    JSON.stringify({
      request_id: requestId,
      method: 'probe',
      deadline_ms: 30_000,
      payload: {},
      ...extra,
    }),
  );
}

function openWithProviders(providers) {
  const home = seedHome();
  const binding = require(nodePath);
  const core = binding.open(
    JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
    providers,
  );
  return { core, binding, home };
}

describe('callback lifecycle', { concurrency: 1 }, () => {
  test('sync throw is sanitized', async () => {
    const { core } = openWithProviders({
      call: () => {
        throw new Error('sync-boom');
      },
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    await assert.rejects(() =>
      core.providerCall(providerCallBuffer('1')),
    );
    await core.close();
  });

  test('rejected promise is sanitized', async () => {
    const { core } = openWithProviders({
      call: async () => Promise.reject(new Error('reject-boom')),
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    await assert.rejects(() =>
      core.providerCall(providerCallBuffer('2')),
    );
    await core.close();
  });

  test('callback after close is rejected', async () => {
    const { core } = openWithProviders({
      call: async () => JSON.stringify({ request_id: '3', ok: true }),
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    await core.close();
    await assert.rejects(() =>
      core.providerCall(providerCallBuffer('4')),
    );
  });

  test('reentrant provider graph allowed', async () => {
    const { core } = openWithProviders({
      call: async (payload) => {
        const request = parseProviderCall(payload);
        return providerReply(request?.request_id ?? 'unknown');
      },
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    await core.providerCall(providerCallBuffer('5'));
    await core.providerCall(providerCallBuffer('r'));
    await core.close();
  });

  test('same-op concurrent next is busy', async () => {
    let release;
    const gate = new Promise((r) => {
      release = r;
    });
    const { core } = openWithProviders({
      call: async () => JSON.stringify({ request_id: 'x', ok: true }),
      next: async () => {
        await gate;
        return JSON.stringify({ operation_id: 'op-1', events: [], has_more: false });
      },
    });
    const first = core.nextProviderEvents('op-1', 1, 1024);
    const busy = assert.rejects(
      () => core.nextProviderEvents('op-1', 1, 1024),
      /pull already in flight/,
    );
    await new Promise((r) => setTimeout(r, 50));
    release?.();
    await busy;
    await first;
    await core.close();
  });

  test('foreign principal handle rejected', async () => {
    const { core } = openWithProviders({
      call: async () => providerReply('x'),
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    const principal = await core.activePrincipal();
    await core.close();
    const home = seedHome();
    const binding = require(nodePath);
    const core2 = binding.open(
      JSON.stringify({ user_home: home, access: 'engine_owner', allow_uninitialized: false }),
      {
        call: async () => providerReply('x'),
        next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
      },
    );
    await assert.rejects(() => core2.worldKbGraph(principal, 'wld_owned', false));
    await core2.close();
  });

  test('never-settling promise is interrupted on close', async () => {
    const { core } = openWithProviders({
      call: async () => new Promise(() => {}),
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    const pending = core.providerCall(providerCallBuffer('hang'));
    await Promise.race([
      pending.then(() => assert.fail('should not resolve')),
      new Promise((r) => setTimeout(r, 50)),
    ]);
    await core.close();
    await assert.rejects(() => pending);
  });

  test('worker termination does not abort process on require', async () => {
    const script = `
      const { workerData, parentPort } = require('node:worker_threads');
      const binding = require(workerData.nodePath);
      const core = binding.open(JSON.stringify(workerData.options), {
        call: () => new Promise(() => {}),
        next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
      });
      parentPort.postMessage('ready');
      setTimeout(() => core.providerCall(Buffer.from(providerCallBuffer('w'))), 10);
    `;
    const home = seedHome();
    const worker = new Worker(script, {
      eval: true,
      workerData: {
        nodePath,
        options: { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      },
    });
    await new Promise((resolve, reject) => {
      worker.once('message', resolve);
      worker.once('error', reject);
    });
    await worker.terminate();
  });
});
