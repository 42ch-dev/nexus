/**
 * v1.239 P3-T6: `GET /v1/daemon/patterns` over the real core path.
 *
 * The T4 suite drives the handler over a stubbed `ServiceCore.core` seam, so
 * nothing there proves the native reader is wired to the route. This suite
 * seeds a disposable home, copies the committed `nexus-pattern` fixture
 * packages into `<home>/.nexus42/patterns`, starts the service on it and
 * asserts the served listing — pins included. Nothing here reads or writes the
 * developer's own `~/.nexus42`.
 */
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

/** The committed fixture registry root the Rust fixture test also reads. */
const FIXTURE_PATTERNS = join(
  serviceRoot,
  '..',
  '..',
  'crates',
  'nexus-pattern',
  'tests',
  'fixtures',
  'patterns',
);

/** Pins asserted by `crates/nexus-pattern/tests/fixtures.rs`; a silent change fails both. */
const PROMPT_STEP_HASH = 'b89e9707bf2d39e27bf825f74c8bfac37acbdf1f7109614d0942fb8613212a04';
const STEP_WITH_JUDGE_HASH = '1d62b2b1bb02390d704ce10864b327b2120aed59ebe0ba482fe824347404324e';

const PATH = '/v1/daemon/patterns';

describe('patterns end-to-end (v1.239 P3-T6)', () => {
  let home;
  let service;

  before(async () => {
    assert.ok(existsSync(FIXTURE_PATTERNS), `fixture packages missing at ${FIXTURE_PATTERNS}`);
    home = mkdtempSync(join(tmpdir(), 'nexus-service-patterns-e2e-'));
    const seed = spawnSync(
      'cargo',
      ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
      { stdio: 'inherit' },
    );
    assert.equal(seed.status, 0, seed.stderr?.toString());
    cpSync(FIXTURE_PATTERNS, join(home, '.nexus42', 'patterns'), { recursive: true });

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
    if (home) rmSync(home, { recursive: true, force: true });
  });

  /** One listing page, parsed. */
  async function list(search = '') {
    const response = await fetch(`${service.url}${PATH}${search}`);
    const text = await response.text();
    assert.equal(response.status, 200, text);
    return JSON.parse(text);
  }

  test('the native reader reaches the route with its pins and its broken peer', async () => {
    const body = await list();
    assert.deepEqual(
      body.items.map((item) => item.package_dir),
      ['broken-manifest', 'prompt-step', 'step-with-judge'],
    );
    const byDir = new Map(body.items.map((item) => [item.package_dir, item]));

    const promptStep = byDir.get('prompt-step');
    assert.equal(promptStep.available, true);
    assert.equal(promptStep.reason, null);
    assert.equal(promptStep.id, 'prompt-step');
    assert.equal(promptStep.version, '1.0.0');
    assert.equal(promptStep.description, 'Draft an answer to a topic with one prompt step');
    assert.equal(promptStep.effect_class, 'external');
    assert.deepEqual(promptStep.slots, [{ name: 'topic', kind: 'string', required: true }]);
    assert.deepEqual(promptStep.requires_capabilities, ['acp.prompt']);
    assert.equal(promptStep.content_hash, PROMPT_STEP_HASH);

    const stepWithJudge = byDir.get('step-with-judge');
    assert.equal(stepWithJudge.available, true);
    assert.equal(stepWithJudge.reason, null);
    assert.equal(stepWithJudge.effect_class, 'external');
    assert.deepEqual(stepWithJudge.requires_capabilities, ['acp.prompt', 'judge.llm']);
    assert.equal(stepWithJudge.content_hash, STEP_WITH_JUDGE_HASH);

    const broken = byDir.get('broken-manifest');
    assert.equal(broken.available, false);
    assert.equal(broken.id, null);
    assert.equal(broken.version, null);
    assert.equal(broken.content_hash, null);
    assert.equal(broken.effect_class, null);
    assert.deepEqual(broken.slots, []);
    assert.match(broken.reason, /`pattern\.toml` is malformed/);

    assert.equal(body.pagination.has_more, false);
    assert.equal(body.pagination.next_cursor, undefined);
  });

  test('pagination follows the real listing one item at a time', async () => {
    const first = await list('?limit=1');
    assert.deepEqual(
      first.items.map((item) => item.package_dir),
      ['broken-manifest'],
    );
    assert.equal(first.pagination.limit, 1);
    assert.equal(first.pagination.has_more, true);
    assert.equal(first.pagination.next_cursor, 'broken-manifest');

    const second = await list(`?limit=1&cursor=${first.pagination.next_cursor}`);
    assert.deepEqual(
      second.items.map((item) => item.package_dir),
      ['prompt-step'],
    );
    assert.equal(second.items[0].content_hash, PROMPT_STEP_HASH);
    assert.equal(second.pagination.has_more, true);
    assert.equal(second.pagination.next_cursor, 'prompt-step');
  });
});
