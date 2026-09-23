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
    // Independent authority: a hardcoded normalized snapshot of the daemon
    // create_router + P0–P4 family identities (NOT derived from the composer
    // under test). Divergence is a real reconciliation bug, in either
    // direction.
    const LIVE_INVENTORY = [
  ['DELETE', '/v1/daemon/agent-host/sessions/([^/]+)', 'provider_stream', 'host'],
  ['DELETE', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)', 'tier2', 'actors'],
  ['DELETE', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)', 'tier2', 'actors'],
  ['DELETE', '/v1/daemon/characters/([^/]+)/memory/pending-review/([^/]+)', 'tier2', 'memory'],
  ['DELETE', '/v1/daemon/kb/entries/([^/]+)', 'tier2', 'knowledge'],
  ['DELETE', '/v1/daemon/memory/pending-review/([^/]+)', 'tier2', 'memory'],
  ['DELETE', '/v1/daemon/presets/([^/]+)', 'tier2', 'presets'],
  ['DELETE', '/v1/daemon/reading/annotations/([^/]+)', 'tier2', 'knowledge'],
  ['DELETE', '/v1/daemon/reading/progress', 'tier2', 'knowledge'],
  ['DELETE', '/v1/daemon/works/([^/]+)', 'tier2', 'works'],
  ['DELETE', '/v1/daemon/works/([^/]+)/findings/([^/]+)', 'tier2', 'knowledge'],
  ['DELETE', '/v1/daemon/worlds/([^/]+)', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/agent-host/health', 'tier1', 'host'],
  ['GET', '/v1/daemon/agent-host/operations/([^/]+)', 'tier2', 'host'],
  ['GET', '/v1/daemon/agent-host/providers', 'tier1', 'host'],
  ['GET', '/v1/daemon/agent-host/sessions', 'tier2', 'host'],
  ['GET', '/v1/daemon/agent-host/sessions/([^/]+)', 'tier2', 'host'],
  ['GET', '/v1/daemon/agent-host/sessions/([^/]+)/events', 'provider_stream', 'host'],
  ['GET', '/v1/daemon/characters', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)/bindings', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)/knowledge', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)', 'tier2', 'actors'],
  ['GET', '/v1/daemon/characters/([^/]+)/memory/fragments', 'tier2', 'memory'],
  ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review', 'tier2', 'memory'],
  ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review/count', 'tier2', 'memory'],
  ['GET', '/v1/daemon/characters/([^/]+)/tom', 'tier2', 'memory'],
  ['GET', '/v1/daemon/core/changes', 'tier2', 'world_kb'],
  ['GET', '/v1/daemon/creators', 'tier1', 'actors'],
  ['GET', '/v1/daemon/creators/([^/]+)', 'tier1', 'actors'],
  ['GET', '/v1/daemon/creators/active', 'tier1', 'actors'],
  ['GET', '/v1/daemon/daemon/status', 'unguarded', 'runtime'],
  ['GET', '/v1/daemon/findings/([^/]+)', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/findings/stale', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/kb/entries', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/kb/entries/([^/]+)', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/memory/fragments', 'tier2', 'memory'],
  ['GET', '/v1/daemon/memory/pending-review', 'tier2', 'memory'],
  ['GET', '/v1/daemon/memory/pending-review/count', 'tier2', 'memory'],
  ['GET', '/v1/daemon/narrative/worlds', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/narrative/worlds/([^/]+)', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/orchestration/presets', 'tier2', 'presets'],
  ['GET', '/v1/daemon/orchestration/presets/([^/]+)/profile', 'tier2', 'presets'],
  ['GET', '/v1/daemon/orchestration/schedules', 'tier2', 'execution'],
  ['GET', '/v1/daemon/orchestration/schedules/([^/]+)', 'tier2', 'execution'],
  ['GET', '/v1/daemon/orchestration/sessions', 'tier2', 'execution'],
  ['GET', '/v1/daemon/orchestration/sessions/([^/]+)', 'tier2', 'execution'],
  ['GET', '/v1/daemon/presets', 'tier2', 'presets'],
  ['GET', '/v1/daemon/presets/([^/]+)', 'tier2', 'presets'],
  ['GET', '/v1/daemon/reading/annotations', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/reading/progress', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/references', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/references/([^/]+)', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/runtime/cert-fingerprint', 'unguarded', 'runtime'],
  ['GET', '/v1/daemon/runtime/health', 'unguarded', 'runtime'],
  ['GET', '/v1/daemon/runtime/status', 'unguarded', 'runtime'],
  ['GET', '/v1/daemon/timeline/overview', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/works', 'tier2', 'works'],
  ['GET', '/v1/daemon/works/([^/]+)', 'tier2', 'works'],
  ['GET', '/v1/daemon/works/([^/]+)/chapters', 'tier2', 'content'],
  ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)', 'tier2', 'content'],
  ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/body', 'tier2', 'content'],
  ['GET', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/outline', 'tier2', 'content'],
  ['GET', '/v1/daemon/works/([^/]+)/findings', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/works/([^/]+)/findings/([^/]+)', 'tier2', 'knowledge'],
  ['GET', '/v1/daemon/works/([^/]+)/outline', 'tier2', 'content'],
  ['GET', '/v1/daemon/works/pool', 'tier2', 'works'],
  ['GET', '/v1/daemon/works/pool/inspiration', 'tier2', 'works'],
  ['GET', '/v1/daemon/worlds/([^/]+)/findings', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/worlds/([^/]+)/kb/candidates', 'tier2', 'world_kb'],
  ['GET', '/v1/daemon/worlds/([^/]+)/kb/graph', 'tier2', 'world_kb'],
  ['GET', '/v1/daemon/worlds/([^/]+)/kb/key-blocks/([^/]+)/state', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/worlds/([^/]+)/rules', 'tier2', 'worlds'],
  ['GET', '/v1/daemon/worlds/([^/]+)/timeline/events', 'tier2', 'worlds'],
  ['PATCH', '/v1/daemon/characters/([^/]+)', 'tier2', 'actors'],
  ['PATCH', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)', 'tier2', 'actors'],
  ['PATCH', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)', 'tier2', 'actors'],
  ['PATCH', '/v1/daemon/creators/([^/]+)', 'tier1', 'actors'],
  ['PATCH', '/v1/daemon/findings/batch', 'tier2', 'knowledge'],
  ['PATCH', '/v1/daemon/orchestration/schedules/([^/]+)/core-context', 'tier2', 'execution'],
  ['PATCH', '/v1/daemon/presets/([^/]+)', 'tier2', 'presets'],
  ['PATCH', '/v1/daemon/reading/annotations/([^/]+)', 'tier2', 'knowledge'],
  ['PATCH', '/v1/daemon/works/([^/]+)', 'tier2', 'works'],
  ['PATCH', '/v1/daemon/works/([^/]+)/chapters/([^/]+)', 'tier2', 'content'],
  ['PATCH', '/v1/daemon/works/([^/]+)/findings/([^/]+)', 'tier2', 'knowledge'],
  ['PATCH', '/v1/daemon/worlds/([^/]+)/rules/([^/]+)', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/actor-knowledge/entries', 'tier2', 'actors'],
  ['POST', '/v1/daemon/actor-knowledge/view', 'tier2', 'actors'],
  ['POST', '/v1/daemon/agent-host/operations/([^/]+)', 'provider_stream', 'host'],
  ['POST', '/v1/daemon/agent-host/scan', 'tier1', 'host'],
  ['POST', '/v1/daemon/agent-host/sessions', 'provider_stream', 'host'],
  ['POST', '/v1/daemon/agent-host/sessions/([^/]+)/operations', 'provider_stream', 'host'],
  ['POST', '/v1/daemon/characters', 'tier2', 'actors'],
  ['POST', '/v1/daemon/characters/([^/]+)/archive', 'tier2', 'actors'],
  ['POST', '/v1/daemon/characters/([^/]+)/bindings', 'tier2', 'actors'],
  ['POST', '/v1/daemon/characters/([^/]+)/memory/fragments/([^/]+):promote', 'tier2', 'memory'],
  ['POST', '/v1/daemon/characters/([^/]+)/memory/pending-review', 'tier2', 'memory'],
  ['POST', '/v1/daemon/characters/([^/]+)/memory/review', 'tier2', 'memory'],
  ['POST', '/v1/daemon/characters/([^/]+)/restore', 'tier2', 'actors'],
  ['POST', '/v1/daemon/characters/([^/]+)/soul/reflect', 'tier2', 'memory'],
  ['POST', '/v1/daemon/characters/([^/]+)/tom', 'tier2', 'memory'],
  ['POST', '/v1/daemon/creators', 'tier1', 'actors'],
  ['POST', '/v1/daemon/creators/([^/]+)', 'tier1', 'actors'],
  ['POST', '/v1/daemon/findings/prune', 'tier2', 'knowledge'],
  ['POST', '/v1/daemon/inspector/moment', 'tier2', 'context'],
  ['POST', '/v1/daemon/kb/entries', 'tier2', 'knowledge'],
  ['POST', '/v1/daemon/memory/review', 'tier2', 'memory'],
  ['POST', '/v1/daemon/memory/soul/reflect', 'tier2', 'memory'],
  ['POST', '/v1/daemon/moment-directive', 'tier2', 'context'],
  ['POST', '/v1/daemon/orchestration/schedules', 'tier2', 'execution'],
  ['POST', '/v1/daemon/orchestration/schedules/([^/]+)/signal', 'tier2', 'execution'],
  ['POST', '/v1/daemon/presets', 'tier2', 'presets'],
  ['POST', '/v1/daemon/presets:validate', 'tier2', 'presets'],
  ['POST', '/v1/daemon/reading/annotations', 'tier2', 'knowledge'],
  ['POST', '/v1/daemon/strategies/([^/]+)/states/([^/]+)/patch', 'tier2', 'presets'],
  ['POST', '/v1/daemon/strategies/([^/]+)/states/([^/]+)/prompt/patch', 'tier2', 'presets'],
  ['POST', '/v1/daemon/strategies/([^/]+)/transitions/patch', 'tier2', 'presets'],
  ['POST', '/v1/daemon/works', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/([^/]+)/chapters/([^/]+)/patch', 'tier2', 'content'],
  ['POST', '/v1/daemon/works/([^/]+)/completion-lock/release', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/([^/]+)/findings', 'tier2', 'knowledge'],
  ['POST', '/v1/daemon/works/([^/]+)/findings/from-review', 'tier2', 'knowledge'],
  ['POST', '/v1/daemon/works/([^/]+)/inspiration', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/([^/]+)/outline/patch', 'tier2', 'content'],
  ['POST', '/v1/daemon/works/([^/]+)/reconcile-chapters', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/([^/]+)/timeline/patch', 'tier2', 'content'],
  ['POST', '/v1/daemon/works/pool', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/pool/archive', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/pool/inspiration', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/pool/inspiration/archive', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/pool/inspiration/promote', 'tier2', 'works'],
  ['POST', '/v1/daemon/works/pool/promote', 'tier2', 'works'],
  ['POST', '/v1/daemon/worlds', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/forks', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/kb/pack/export', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/kb/pack/import', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/kb/patch-entity', 'tier2', 'world_kb'],
  ['POST', '/v1/daemon/worlds/([^/]+)/kb/patch-relationship', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/kb/promote-candidate', 'tier2', 'worlds'],
  ['POST', '/v1/daemon/worlds/([^/]+)/rules', 'tier2', 'worlds'],
  ['PUT', '/v1/daemon/creators/active', 'tier1', 'actors'],
  ['PUT', '/v1/daemon/reading/progress', 'tier2', 'knowledge'],
    ];
    const liveKeys = new Set(
      LIVE_INVENTORY.map(([m, p, tier, family]) => `${m} ${p} ${tier} ${family}`),
    );

    // Normalized live inventory: the retained daemon route identities the
    // standalone service owns, derived from create_router + the P0–P4 family
    // producers. Every mounted identity must be in the set and vice versa.
    const mounted = DOMAIN_ROUTES.map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
      family: route.family,
    }));
    const mountedKeys = new Set(
      mounted.map((r) => `${r.method} ${r.path} ${r.tier} ${r.family}`),
    );

    // Handler dispatch is total: every /v1/daemon identity must be composer-
    // routed (family), so a hand-written branch leaking outside the composer
    // would show up as a mounted-but-not-dispatched identity. Probe: a
    // composer-routed tier2 identity 404s on a missing resource (reached the
    // handler), while a non-composer identity would 501 route_not_migrated.
    const probeMissing = await jsonFetch(`${baseUrl}/v1/daemon/works/work_missing`);
    assert.equal(probeMissing.status, 404, probeMissing.text);
    assert.notEqual(probeMissing.payload.error.code, 'route_not_migrated');

    // No duplicate mounted identities, and mounted == authoritative inventory.
    assert.equal(mountedKeys.size, mounted.length, 'duplicate mounted route identity');
    const missingFromMounted = [...liveKeys].filter((k) => !mountedKeys.has(k));
    const extraMounted = [...mountedKeys].filter((k) => !liveKeys.has(k));
    assert.deepEqual(
      { missingFromMounted, extraMounted },
      { missingFromMounted: [], extraMounted: [] },
      `inventory drift\nmissing:\n${missingFromMounted.join('\n')}\nextra:\n${extraMounted.join('\n')}`,
    );

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
