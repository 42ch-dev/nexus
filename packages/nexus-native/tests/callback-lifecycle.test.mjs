import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { Worker } from 'node:worker_threads';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');
const require = createRequire(import.meta.url);
const { loadNodePath } = await import('../dist/loader.js');
const nodePath = loadNodePath();

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

/**
 * A valid seeded home whose `.nexus42/agent-host/config.toml` is a symlink to a
 * valid TOML holding default-safe values.
 *
 * `escape: false` points the link inside the expected config directory (the
 * config path stays where the host expects it). `escape: true` points it outside
 * that directory: `validate_host_admission` still reads it through the link and
 * admits the open, and only `HostManager::start`'s post-open canonicalization
 * rejects the escaped path — so an `escape: true` failure is the rollback branch
 * (core already open, host start failed), never a pre-open admission rejection.
 */
function seedHostConfigHome(escape) {
  const home = seedHome();
  const configDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(configDir, { recursive: true });
  const realConfig = escape
    ? join(home, 'host-config-outside-agent-host.toml')
    : join(configDir, 'host-config.toml');
  writeFileSync(realConfig, 'max_sessions = 8\n');
  symlinkSync(realConfig, join(configDir, 'config.toml'));
  return home;
}

/** Parse the wire envelope an `open` rejection carries; null when it has none. */
function parseWireFailure(failure) {
  try {
    return JSON.parse(String(failure).replace(/^Error:\s*/, ''));
  } catch {
    return null;
  }
}

function parseProviderCall(payload) {
  return typeof payload === 'string' ? JSON.parse(payload) : payload;
}

function unpackCallbackPayload(...args) {
  const payload = args.length > 1 ? args[1] : args[0];
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

function effectCallBuffer(requestId, method = 'cancel', payload = { operation_id: '00000000-0000-4000-8000-000000000099' }) {
  return new TextEncoder().encode(
    JSON.stringify({
      request_id: requestId,
      method,
      deadline_ms: 30_000,
      payload,
    }),
  );
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
    await core.providerCall(effectCallBuffer('5'));
    await core.providerCall(effectCallBuffer('r'));
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
    const closeReport = JSON.parse(new TextDecoder().decode(await core.close()));
    assert.equal(closeReport.cleanup_confirmed, true);
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
    const pending = core.providerCall(effectCallBuffer('hang'));
    await Promise.race([
      pending.then(() => assert.fail('should not resolve')),
      new Promise((r) => setTimeout(r, 50)),
    ]);
    await core.close();
    await assert.rejects(() => pending);
  });

  test('host start failure after a successful core open leaves no owner', async () => {
    const binding = require(nodePath);
    // Control: the same linked-config fixture with its target INSIDE the
    // expected config directory opens, so the failure below is attributable to
    // the escaped config path and nothing else about the fixture.
    const control = binding.open(
      JSON.stringify({
        user_home: seedHostConfigHome(false),
        access: 'engine_owner',
        allow_uninitialized: false,
      }),
      {
        call: async () => providerReply('control'),
        next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
      },
    );
    await control.close();

    // A config link escaping `.nexus42/agent-host/` is read and admitted by
    // `validate_host_admission` (before the core opens) and rejected only by
    // `HostManager::start`'s config-path canonicalization — after the core has
    // already opened, which is the rollback branch under test.
    let failure = null;
    try {
      binding.open(
        JSON.stringify({
          user_home: seedHostConfigHome(true),
          access: 'engine_owner',
          allow_uninitialized: false,
        }),
      );
    } catch (error) {
      failure = String(error);
    }
    assert.ok(failure, 'host start must fail for an escaping config path');
    const wire = parseWireFailure(failure);
    assert.ok(wire, `open rejection must carry the wire envelope, got: ${failure}`);
    assert.equal(wire.code, 'forbidden');
    assert.equal(wire.details?.category, 'policy_denied');
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
    binding.forceUnconfirmedCleanup(true);
    try {
      // 1) core opens, host.start rejects the escaped config path, and the
      //    rollback's cleanup reports unconfirmed, so its owners are retained.
      let failure = null;
      try {
        binding.open(
          JSON.stringify({
            user_home: seedHostConfigHome(true),
            access: 'engine_owner',
            allow_uninitialized: false,
          }),
        );
      } catch (error) {
        failure = String(error);
      }
      assert.ok(failure, 'host start must fail for an escaping config path');
      const wire = parseWireFailure(failure);
      assert.ok(wire, `open rejection must carry the wire envelope, got: ${failure}`);
      assert.equal(wire.code, 'forbidden');
      assert.equal(wire.details?.category, 'policy_denied');

      // 2) while cleanup stays unconfirmed, further opens are denied and the
      //    retained owners survive (never replaced or dropped). Distinct fresh
      //    homes are used, so a denial can only come from the environment-wide
      //    retained-owner fence rather than from one home's own resources.
      const validHome = seedHome();
      for (const candidate of [validHome, seedHome()]) {
        let denied = null;
        try {
          binding.open(
            JSON.stringify({
              user_home: candidate,
              access: 'engine_owner',
              allow_uninitialized: false,
            }),
          );
        } catch (error) {
          denied = String(error);
        }
        assert.ok(denied, 'open must be denied while a retained owner is unconfirmed');
        const deniedWire = parseWireFailure(denied);
        assert.ok(deniedWire, `fence denial must carry the wire envelope, got: ${denied}`);
        assert.notEqual(
          deniedWire.details?.category,
          'policy_denied',
          `the fence denial is not a repeat of the config-path rejection, got: ${denied}`,
        );
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

  test('unknown provider next maps to not_found through real bridge', async () => {
    const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0, build.stderr?.toString());
    const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
    const providers = createAcpProvider();
    const { core } = openWithProviders({
      call: async (...args) =>
        JSON.stringify(await providers.call(unpackCallbackPayload(...args))),
      next: async (...args) => {
        const req = unpackCallbackPayload(...args);
        return JSON.stringify(
          await providers.next(req.operation_id, req.max_events, req.max_bytes),
        );
      },
    });
    await assert.rejects(
      () => core.nextProviderEvents('00000000-0000-4000-8000-000000000099', 16, 65536),
      (err) => /not.?found|operation_not_found/i.test(String(err)),
    );
    await core.close();
  });


  test('concurrent close settles once', async () => {
    const { core } = openWithProviders({
      call: async () => providerReply('cc'),
      next: async () => JSON.stringify({ operation_id: 'x', events: [], has_more: false }),
    });
    const [a, b] = await Promise.all([core.close(), core.close()]);
    const ra = JSON.parse(new TextDecoder().decode(a));
    const rb = JSON.parse(new TextDecoder().decode(b));
    assert.equal(ra.state, rb.state);
    assert.equal(ra.cleanup_confirmed, rb.cleanup_confirmed);
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
