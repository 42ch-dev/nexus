import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { openCore } from '../dist/index.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..', '..', '..');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-tsfn-callback-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: root },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

describe('openCore provider TSFN callback unpack', { concurrency: 1 }, () => {
  test('providerCall and nextProviderEvents round-trip through exported openCore', async () => {
    const home = seedHome();
    let receivedCallId;
    let receivedNextOp;

    const core = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      {
        call: async (request) => {
          receivedCallId = request.request_id;
          return {
            request_id: request.request_id,
            ok: true,
            operation_id: '00000000-0000-4000-8000-000000010001',
            session_id: '00000000-0000-4000-8000-000000010002',
            health: null,
            error: null,
          };
        },
        next: async (operationId, maxEvents, maxBytes) => {
          receivedNextOp = { operationId, maxEvents, maxBytes };
          return {
            operation_id: operationId,
            events: [],
            has_more: false,
          };
        },
      },
    );

    const reply = await core.providerCall({
      request_id: 'unpack-proof-call',
      method: 'cancel',
      deadline_ms: 30_000,
      payload: { operation_id: '00000000-0000-4000-8000-000000010001' },
    });
    assert.equal(receivedCallId, 'unpack-proof-call');
    assert.equal(reply.ok, true);

    const batch = await core.nextProviderEvents('00000000-0000-4000-8000-000000010001', 4, 8192);
    assert.equal(receivedNextOp.operationId, '00000000-0000-4000-8000-000000010001');
    assert.equal(receivedNextOp.maxEvents, 4);
    assert.equal(receivedNextOp.maxBytes, 8192);
    assert.equal(batch.operation_id, '00000000-0000-4000-8000-000000010001');

    await core.close();
  });

  test('createAcpProvider works through exported openCore without manual unpack', async () => {
    const build = spawnSync('pnpm', ['--filter', '@42ch/nexus-provider-acp', 'build'], {
      cwd: root,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0, build.stderr?.toString());
    const { createAcpProvider } = await import('../../nexus-provider-acp/dist/index.js');
    const home = seedHome();
    const core = await openCore(
      { user_home: home, access: 'engine_owner', allow_uninitialized: false },
      createAcpProvider(),
    );

    await assert.rejects(
      () => core.nextProviderEvents('00000000-0000-4000-8000-000000000099', 16, 65536),
      (err) => /not.?found|operation_not_found/i.test(String(err)),
    );
    await core.close();
  });
});
