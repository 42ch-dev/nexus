import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, dirname } from 'node:path';
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

  test('host start failure after a successful core open leaves no owner', async () => {
    const binding = require(nodePath);
    // A `..` component passes the core's home resolution but is rejected by the
    // host's config/workspace path policy — the core opens, then host.start
    // fails, which is the branch under test.
    const home = seedHome();
    // Built by concatenation: `path.join` would normalize the `..` away.
    const traversal = `${home}/../${basename(home)}`;
    let failure = null;
    try {
      binding.open(
        JSON.stringify({
          user_home: traversal,
          access: 'engine_owner',
          allow_uninitialized: false,
        }),
      );
    } catch (error) {
      failure = String(error);
    }
    assert.ok(failure, 'host start must fail for a traversal config path');
    assert.match(failure, /must not contain/i);
    assert.ok(
      !failure.includes('interrupted'),
      `confirmed cleanup must not report Interrupted, got: ${failure}`,
    );

    // The environment survived with no retained owner: a fresh open succeeds and
    // reports the truth of the newly started host.
    const validHome = seedHome();
    const core = binding.open(
      JSON.stringify({ user_home: validHome, access: 'engine_owner', allow_uninitialized: false }),
      {
        call: async () => providerReply('x'),
        next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
      },
    );
    const health = JSON.parse(
      new TextDecoder().decode(
        await core.hostQuery(new TextEncoder().encode(JSON.stringify({ query: 'health' }))),
      ),
    );
    assert.equal(health.health.running, true);
    assert.equal(health.health.active_sessions, 0);
    await core.close();
  });

  test('unconfirmed rollback fences opens until a confirmed settlement', async () => {
    const binding = require(nodePath);
    const home = seedHome();
    // `path.join` would normalize the `..` away; the host policy must see it.
    const traversal = `${home}/../${basename(home)}`;
    binding.forceUnconfirmedCleanup(true);
    try {
      // 1) core opens, host.start fails, rollback cleanup is unconfirmed.
      let failure = null;
      try {
        binding.open(
          JSON.stringify({
            user_home: traversal,
            access: 'engine_owner',
            allow_uninitialized: false,
          }),
        );
      } catch (error) {
        failure = String(error);
      }
      assert.ok(failure, 'host start must fail for a traversal config path');
      assert.match(failure, /interrupted/);
      assert.match(failure, /cleanup unconfirmed/);

      // 2) while cleanup stays unconfirmed, further opens are denied and the
      //    retained owners survive (never replaced or dropped).
      const validHome = seedHome();
      for (let attempt = 0; attempt < 2; attempt += 1) {
        let denied = null;
        try {
          binding.open(
            JSON.stringify({
              user_home: validHome,
              access: 'engine_owner',
              allow_uninitialized: false,
            }),
          );
        } catch (error) {
          denied = String(error);
        }
        assert.ok(denied, 'open must be denied while a retained owner is unconfirmed');
        assert.match(denied, /interrupted/);
      }

      // 3) a confirmed cleanup settles the retained owners and reopens.
      binding.forceUnconfirmedCleanup(false);
      const core = binding.open(
        JSON.stringify({ user_home: validHome, access: 'engine_owner', allow_uninitialized: false }),
        {
          call: async () => providerReply('x'),
          next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
        },
      );
      const principal = await core.activePrincipal();
      assert.ok(principal.startsWith('p:'), `expected a principal handle, got ${principal}`);
      const health = JSON.parse(
        new TextDecoder().decode(
          await core.hostQuery(new TextEncoder().encode(JSON.stringify({ query: 'health' }))),
        ),
      );
      assert.equal(health.health.running, true);
      await core.close();
    } finally {
      binding.forceUnconfirmedCleanup(false);
    }
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
      setTimeout(
        () =>
          core.providerCall(
            Buffer.from(
              JSON.stringify({ request_id: 'w', method: 'probe', deadline_ms: 30000, payload: {} }),
            ),
          ),
        10,
      );
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
