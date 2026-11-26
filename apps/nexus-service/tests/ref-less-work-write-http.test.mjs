import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

/**
 * This is a self-contained file per the `apps/nexus-service/tests/` convention
 * (no shared fixture module exists; every sibling file seeds + starts its own
 * bounded service). It mirrors the `domain-http.test.mjs` server fixture but
 * keeps its own home so the refusal assertions do not couple to that suite's
 * shared, order-dependent Work state.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-ref-less-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

async function jsonFetch(url, { method = 'GET', body } = {}) {
  const response = await fetch(url, {
    method,
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, payload, text };
}

async function startDomainService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

const OWNED_WORLD = 'wld_owned';
const CREATE_WORK_BODY = {
  title: 'Ref-less Write Work',
  long_term_goal: 'Prove the typed refusal crosses the wire',
  initial_idea: 'Seeded by the v1.210 P2 T2 wire test',
  world_id: OWNED_WORLD,
};

describe('ref-less work write refusal over HTTP (v1.210 P2 T2)', () => {
  let home;
  let service;
  let baseUrl;
  let creativeRoot;

  before(async () => {
    home = seedHome();
    creativeRoot = mkdtempSync(join(tmpdir(), 'nexus-service-ref-less-creative-'));
    const workspaceDir = join(
      home,
      '.nexus42',
      'creators',
      'ctr_testcreator',
      'workspaces',
      'default',
    );
    writeFileSync(
      join(workspaceDir, 'meta.json'),
      JSON.stringify({ local_root: creativeRoot }),
    );
    service = await startDomainService(home, 18_480);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('each write route refuses a ref-less Work with the typed 400 discriminator', async () => {
    const created = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: CREATE_WORK_BODY,
    });
    assert.equal(created.status, 201, created.text);
    const workId = created.payload.work_id;

    const read = await jsonFetch(`${baseUrl}/v1/daemon/works/${workId}/outline`);
    assert.equal(read.status, 200, read.text);
    assert.equal(read.payload.outline_revision, 0);

    /** @type {Array<[string, string, object]>} */
    const cases = [
      [
        'outline/patch',
        `/v1/daemon/works/${workId}/outline/patch`,
        { base_revision: 0, operation: 'move_chapter', chapter_id: 1, volume_id: 2 },
      ],
      [
        'chapters/{n}/patch',
        `/v1/daemon/works/${workId}/chapters/1/patch`,
        { base_revision: 0, chapter_id: 1, set: { title: 'Refused' } },
      ],
      [
        'timeline/patch',
        `/v1/daemon/works/${workId}/timeline/patch`,
        { base_revision: 0, operation: 'add_event', title: 'Refused' },
      ],
    ];

    for (const [label, path, body] of cases) {
      const response = await jsonFetch(`${baseUrl}${path}`, {
        method: 'POST',
        body: { work_id: workId, ...body },
      });
      assert.equal(response.status, 400, `${label}: ${response.text}`);
      assert.notEqual(response.status, 500, `${label}: must never be a 500`);
      assert.equal(response.payload.error.code, 'invalid_input', label);
      assert.equal(
        response.payload.error.details.field,
        'work_ref_missing',
        `${label}: stable discriminator`,
      );
      assert.match(
        String(response.payload.error.details.reason ?? ''),
        /story_ref/,
        `${label}: the refusal must name the recovery step`,
      );
    }
  });

  test("a ref'd Work control still accepts the same write shape", async () => {
    const workRef = 'http-refd-control';
    const created = await jsonFetch(`${baseUrl}/v1/daemon/works`, {
      method: 'POST',
      body: { ...CREATE_WORK_BODY, story_ref: workRef, title: 'Refd Control Work' },
    });
    assert.equal(created.status, 201, created.text);
    const workId = created.payload.work_id;

    const storiesDir = join(creativeRoot, 'Works', workRef, 'Stories');
    mkdirSync(storiesDir, { recursive: true });
    writeFileSync(
      join(storiesDir, 'ch01-opening.md'),
      '---\nstatus: not_started\nvolume: 1\n---\nChapter one body\n',
    );
    const reconciled = await jsonFetch(
      `${baseUrl}/v1/daemon/works/${workId}/reconcile-chapters`,
      { method: 'POST' },
    );
    assert.equal(reconciled.status, 200, reconciled.text);
    assert.equal(reconciled.payload.created, 1, 'the chapter SSOT row is created');

    const patched = await jsonFetch(`${baseUrl}/v1/daemon/works/${workId}/outline/patch`, {
      method: 'POST',
      body: {
        work_id: workId,
        base_revision: 0,
        operation: 'move_chapter',
        chapter_id: 1,
        volume_id: 2,
      },
    });
    assert.equal(patched.status, 200, patched.text);
    assert.equal(patched.payload.new_revision, 1);
  });
});
