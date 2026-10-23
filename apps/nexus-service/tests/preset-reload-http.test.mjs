import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * I-000084 bounded integration target: `POST /v1/daemon/presets/{id}:reload`
 * over the real in-process service and core. The endpoint is read-through: it
 * re-resolves the manifest from disk on every call, so the test proves it by
 * changing the disk between calls (create → resolves, edit → no longer
 * resolves, delete → not_found) rather than by asserting a mock.
 */

const PRESET_ID = 'reload-probe';
const PRESET_YAML = `revision: 1
preset:
  id: ${PRESET_ID}
  version: 1
  kind: creator
  description: "Reload probe"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start"
    next: end
  - id: end
    terminal: true
`;

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-preset-reload-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

/** User preset bundle: `<user home>/.nexus42/presets/<id>`. */
function bundleDir(home) {
  return join(home, '.nexus42', 'presets', PRESET_ID);
}

describe('preset reload (POST /v1/daemon/presets/{id}:reload)', () => {
  let home;
  let service;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    const { startService } = await import(join(serviceRoot, 'dist/index.js'));
    service = await startService({
      home,
      host: '127.0.0.1',
      // Port 0: the OS picks a free port, so parallel suites never collide.
      port: 0,
      allowRemote: false,
      domainOnly: true,
    });
  });

  after(async () => {
    if (service) await service.close();
  });

  test('reload re-reads the manifest from disk on every call', async () => {
    const url = `${service.url}/v1/daemon/presets/${PRESET_ID}:reload`;

    // Unknown before the bundle exists — the route is not a 501 stub.
    const missing = await fetch(url, { method: 'POST' });
    assert.equal(missing.status, 404, await missing.text());

    mkdirSync(bundleDir(home), { recursive: true });
    writeFileSync(join(bundleDir(home), 'preset.yaml'), PRESET_YAML, 'utf8');

    const reloaded = await fetch(url, { method: 'POST' });
    const reloadedBody = await reloaded.text();
    assert.equal(reloaded.status, 200, reloadedBody);
    assert.deepEqual(JSON.parse(reloadedBody), { id: PRESET_ID, reloaded: true });

    // Corrupt the on-disk manifest: the reload re-parses it, so the preset no
    // longer resolves. A cached load would still answer 200 here.
    writeFileSync(join(bundleDir(home), 'preset.yaml'), 'revision: [unclosed\n', 'utf8');
    const broken = await fetch(url, { method: 'POST' });
    assert.equal(broken.status, 404, await broken.text());

    // Restore, then delete: the reload follows disk state.
    writeFileSync(join(bundleDir(home), 'preset.yaml'), PRESET_YAML, 'utf8');
    assert.equal((await fetch(url, { method: 'POST' })).status, 200);
    rmSync(bundleDir(home), { recursive: true, force: true });
    const gone = await fetch(url, { method: 'POST' });
    assert.equal(gone.status, 404, await gone.text());
  });

  test('an embedded preset reloads without a disk bundle', async () => {
    const response = await fetch(`${service.url}/v1/daemon/presets/novel-writing:reload`, {
      method: 'POST',
    });
    const body = await response.text();
    assert.equal(response.status, 200, body);
    assert.deepEqual(JSON.parse(body), { id: 'novel-writing', reloaded: true });
  });
});
