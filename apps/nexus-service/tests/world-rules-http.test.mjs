/**
 * World structured-rule HTTP inclusion + lifecycle (V1.198 P0-T3).
 *
 * Drives the real service over the real native binding: the fixture home is
 * seeded through `native-wire-fixture-seed`, the service is built and started
 * from `dist/`, every request crosses the napi boundary and the single Rust
 * core authority decides ownership, visibility and the archive tombstone.
 *
 * Covered here (AC-7..9/11 HTTP portion):
 * - `include_archived` is parsed strictly: absent/false omit archived rows,
 *   exactly `true` reveals them; a malformed value, a duplicate key or an
 *   unknown query key is the retained 400 field-level `invalid_input`
 *   envelope and never an unfiltered fallback.
 * - create still refuses `archived` with `details.field = status`.
 * - the archived row is a terminal tombstone through the native boundary:
 *   a status exit or any other supplied member is refused by name with no
 *   mutation of the retained row.
 * - ownership and id-addressing guards are unchanged (foreign world 403,
 *   missing world 404, unknown/foreign rule id 404 non-disclosure).
 *
 * The inclusion flag is forwarded to the core verbatim — there is no
 * adapter-side status filtering anywhere in this surface.
 */
import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { describe, test, before, after } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');
const OWNED_WORLD = 'wld_owned';
const FOREIGN_WORLD = 'wld_foreign';

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-rules-http-'));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  return home;
}

async function jsonFetch(url, { method = 'GET', headers = {}, body } = {}) {
  const response = await fetch(url, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, headers: response.headers, payload, text };
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

/** `POST .../rules` — asserts 201 and returns the created `rule_id`. */
async function createRule(baseUrl, overrides = {}) {
  const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/rules`, {
    method: 'POST',
    body: {
      canonical_name: 'Wired rule',
      statement: 'Every character entry must carry a summary.',
      constraint: { family: 'module_presence', module_key: 'characters' },
      status: 'active',
      target_entry_types: [],
      ...overrides,
    },
  });
  assert.equal(res.status, 201, res.text);
  return res.payload.rule_id;
}

/** `PATCH .../rules/{id}` with one supplied member. */
function patchRule(baseUrl, ruleId, body, world = OWNED_WORLD) {
  return jsonFetch(`${baseUrl}/v1/daemon/worlds/${world}/rules/${ruleId}`, {
    method: 'PATCH',
    body,
  });
}

/** The listed rule ids of the owned World for the given query suffix. */
async function listRuleIds(baseUrl, query = '') {
  const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/rules${query}`);
  assert.equal(res.status, 200, res.text);
  return res.payload;
}

describe('world-rules-http (V1.198 P0-T3)', () => {
  let home;
  let service;
  let baseUrl;
  let activeId;
  let archivedId;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startDomainService(home, 18_451);
    baseUrl = service.url;

    activeId = await createRule(baseUrl, { canonical_name: 'Kept rule' });
    archivedId = await createRule(baseUrl, { canonical_name: 'Archived rule' });
    const archived = await patchRule(baseUrl, archivedId, { status: 'archived' });
    assert.equal(archived.status, 200, archived.text);
    assert.equal(archived.payload.status, 'archived');
  });

  after(async () => {
    if (service) {
      await service.close();
    }
  });

  test('create refuses archived with the field-level envelope', async () => {
    const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/rules`, {
      method: 'POST',
      body: {
        canonical_name: 'Born archived',
        statement: 'statement',
        constraint: { family: 'module_presence', module_key: 'characters' },
        status: 'archived',
      },
    });
    assert.equal(res.status, 400, res.text);
    assert.equal(res.payload.error.code, 'invalid_input');
    assert.equal(res.payload.error.details.field, 'status');

    const listed = await listRuleIds(baseUrl, '?include_archived=true');
    assert.ok(
      !listed.rules.some((rule) => rule.canonical_name === 'Born archived'),
      'the refused create wrote no row',
    );
  });

  test('default read omits archived; include_archived=true reveals it', async () => {
    const omitted = await listRuleIds(baseUrl);
    assert.ok(
      omitted.rules.some((rule) => rule.rule_id === activeId),
      omitted.text,
    );
    assert.ok(
      !omitted.rules.some((rule) => rule.rule_id === archivedId),
      `the default read hides the archived row: ${omitted.text}`,
    );

    const explicitFalse = await listRuleIds(baseUrl, '?include_archived=false');
    assert.deepEqual(
      explicitFalse.rules.map((rule) => rule.rule_id),
      omitted.rules.map((rule) => rule.rule_id),
      'explicit false is the default read',
    );

    const included = await listRuleIds(baseUrl, '?include_archived=true');
    const archived = included.rules.find((rule) => rule.rule_id === archivedId);
    assert.ok(archived, `explicit inclusion reveals the retained row: ${included.text}`);
    assert.equal(archived.status, 'archived');
    assert.equal(archived.canonical_name, 'Archived rule');
  });

  test('malformed or duplicate include_archived is a 400 field error', async () => {
    for (const query of [
      'include_archived=1',
      'include_archived=0',
      'include_archived=TRUE',
      'include_archived=',
      'include_archived=yes',
      'include_archived=true&include_archived=false',
      'include_archived=false&include_archived=false',
    ]) {
      const res = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/rules?${query}`);
      assert.equal(res.status, 400, `${query} → ${res.text}`);
      assert.equal(res.payload.error.code, 'invalid_input', query);
      assert.equal(res.payload.error.details.field, 'include_archived', query);
    }
  });

  test('unknown query keys are refused instead of silently widened', async () => {
    const unknown = await jsonFetch(
      `${baseUrl}/v1/daemon/worlds/${OWNED_WORLD}/rules?include_archived=true&status=archived`,
    );
    assert.equal(unknown.status, 400, unknown.text);
    assert.equal(unknown.payload.error.code, 'invalid_input');
    assert.equal(unknown.payload.error.details.field, 'status');
  });

  test('archived rows are terminal through the real native boundary', async () => {
    const cases = [
      { body: { status: 'active' }, field: 'status' },
      { body: { status: 'deprecated' }, field: 'status' },
      { body: { status: 'archived', canonical_name: 'Resurrected' }, field: 'canonical_name' },
      { body: { canonical_name: 'Resurrected' }, field: 'canonical_name' },
      // An explicit null is a SUPPLIED member, not an absent one: the native
      // presence capture must survive the typed decode, so this names `status`
      // rather than degrading into the empty-PATCH refusal.
      { body: { status: null }, field: 'status' },
      // A raw empty object supplies nothing and still names `patch`.
      { body: {}, field: 'patch' },
    ];
    for (const { body, field } of cases) {
      const res = await patchRule(baseUrl, archivedId, body);
      assert.equal(res.status, 400, `${JSON.stringify(body)} → ${res.text}`);
      assert.equal(res.payload.error.code, 'invalid_input');
      assert.equal(res.payload.error.details.field, field, JSON.stringify(body));
    }

    const included = await listRuleIds(baseUrl, '?include_archived=true');
    const archived = included.rules.find((rule) => rule.rule_id === archivedId);
    assert.equal(archived.status, 'archived', 'the refused writes changed nothing');
    assert.equal(archived.canonical_name, 'Archived rule');
  });

  test('archive repetition succeeds and keeps the row retained', async () => {
    const transientId = await createRule(baseUrl, { canonical_name: 'Transient rule' });
    const first = await patchRule(baseUrl, transientId, { status: 'archived' });
    assert.equal(first.status, 200, first.text);
    assert.equal(first.payload.status, 'archived');

    const replay = await patchRule(baseUrl, transientId, { status: 'archived' });
    assert.equal(replay.status, 200, replay.text);
    assert.equal(replay.payload.status, 'archived');
    assert.equal(replay.payload.canonical_name, 'Transient rule');

    const included = await listRuleIds(baseUrl, '?include_archived=true');
    assert.ok(
      included.rules.some((rule) => rule.rule_id === transientId),
      'the repeated archive retained the row',
    );
  });

  test('ownership and id-addressing guard parity', async () => {
    const foreignList = await jsonFetch(`${baseUrl}/v1/daemon/worlds/${FOREIGN_WORLD}/rules`);
    assert.equal(foreignList.status, 403, foreignList.text);
    assert.equal(foreignList.payload.error.code, 'forbidden');

    const foreignArchive = await patchRule(baseUrl, activeId, { status: 'archived' }, FOREIGN_WORLD);
    assert.equal(foreignArchive.status, 403, foreignArchive.text);

    const missingList = await jsonFetch(`${baseUrl}/v1/daemon/worlds/wld_missing/rules`);
    assert.equal(missingList.status, 404, missingList.text);
    assert.equal(missingList.payload.error.code, 'not_found');

    const unknownRule = await patchRule(baseUrl, 'rul_missing', { status: 'archived' });
    assert.equal(unknownRule.status, 404, unknownRule.text);
    assert.equal(unknownRule.payload.error.code, 'not_found');

    const listed = await listRuleIds(baseUrl);
    const active = listed.rules.find((rule) => rule.rule_id === activeId);
    assert.equal(active.status, 'active', 'the refused writes left the active rule untouched');
  });
});
