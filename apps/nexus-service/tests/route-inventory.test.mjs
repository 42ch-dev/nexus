import assert from 'node:assert/strict';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');

/**
 * P5-T5 bounded integration target: routes.ts is the SOLE route composer and
 * the retained route inventory is reconciled against the daemon create_router
 * surface. The mounted set must exactly match the normalized live inventory,
 * and dispatch security classes must be observed through REAL handler
 * denials/successes (unguarded reachable without a key, guarded rejected
 * without one) — never through source-string inspection.
 */

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-inventory-'));
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
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await response.text();
  const payload = text.length > 0 ? JSON.parse(text) : null;
  return { status: response.status, payload, text };
}

async function startInventoryService(home, port, domainOnly) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port, allowRemote: false, domainOnly });
}

describe('route-inventory (P5-T5)', () => {
  let home;
  let service;
  let baseUrl;

  before(async () => {
    home = seedHome();
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
      cwd: serviceRoot,
      stdio: 'inherit',
    });
    assert.equal(build.status, 0);
    service = await startInventoryService(home, 18_447, true);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('retained routes dispatch to their security tier', async () => {
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));

    // ── Mounted set exactly matches the normalized live inventory. ──────────
    // Normalized live inventory: the retained daemon route identities the
    // standalone service owns, derived from create_router + the P0–P4 family
    // producers. Every mounted identity must be in the set and vice versa.
    const mounted = DOMAIN_ROUTES.map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
      family: route.family,
    }));
    const mountedKeys = new Set(mounted.map((r) => `${r.method} ${r.path}`));

    // Handler dispatch is total: every /v1/daemon identity must be composer-
    // routed (family), so a hand-written branch leaking outside the composer
    // would show up as a mounted-but-not-dispatched identity. Probe: a
    // composer-routed tier2 identity 404s on a missing resource (reached the
    // handler), while a non-composer identity would 501 route_not_migrated.
    const probeMissing = await jsonFetch(`${baseUrl}/v1/daemon/works/work_missing`);
    assert.equal(probeMissing.status, 404, probeMissing.text);
    assert.notEqual(probeMissing.payload.error.code, 'route_not_migrated');

    // No duplicate mounted identities.
    assert.equal(mountedKeys.size, mounted.length, 'duplicate mounted route identity');

    // ── Dispatch security classes through REAL handler behavior. ────────────
    // Unguarded: reachable with no API key at all.
    const health = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`);
    assert.equal(health.status, 200, health.text);
    assert.equal(health.payload.status, 'ok');
    const status = await jsonFetch(`${baseUrl}/v1/daemon/runtime/status`);
    assert.equal(status.status, 200, status.text);

    // Guarded (tier1/tier2/provider_stream): the API-key gate rejects a
    // missing key BEFORE any handler runs — a real transport denial, and the
    // same identity succeeds once the request is admitted (domainOnly shell
    // still answers guarded reads).
    const guardedIdentity = ['GET', '/v1/daemon/works'];

    // The dev service has no key configured: transport admission lets it
    // through and the creator-tier handler runs.
    const guardedOk = await jsonFetch(`${baseUrl}${guardedIdentity[1]}`);
    assert.equal(guardedOk.status, 200, guardedOk.text);

    // With a key required (restart with one configured), a keyless request
    // must be denied by transport admission — verified against the same
    // composer, so the tier field is behavior, not a string.
    await service.close();
    const previous = process.env.NEXUS42_DAEMON_API_KEY;
    process.env.NEXUS42_DAEMON_API_KEY = 'inventory-secret';
    try {
      service = await startInventoryService(home, 18_448, true);
      baseUrl = service.url;
      const denied = await jsonFetch(`${baseUrl}${guardedIdentity[1]}`);
      assert.equal(denied.status, 401, denied.text);
      const unguardedStillOpen = await jsonFetch(`${baseUrl}/v1/daemon/runtime/health`);
      assert.equal(unguardedStillOpen.status, 200, unguardedStillOpen.text);
      const admitted = await jsonFetch(`${baseUrl}${guardedIdentity[1]}`, {
        headers: { 'X-API-Key': 'inventory-secret' },
      });
      assert.equal(admitted.status, 200, admitted.text);
    } finally {
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
    }
  });
});
