import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
import { after, before, describe, test } from 'node:test';

const __dirname = dirname(fileURLToPath(import.meta.url));
const serviceRoot = join(__dirname, '..');
const repoRoot = join(__dirname, '..', '..', '..');
const acpFixture = join(repoRoot, 'crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py');

/**
 * P5-T2 bounded integration target: the Actor / memory / context families
 * over the real in-process service and the real native temporary store. No
 * mock forwards anything — the Character/binding fixtures are created through
 * the surface itself, and denial (foreign Character, stale binding) is
 * observed as the retained HTTP statuses with zero storage effects.
 */

function seedHome() {
  const home = mkdtempSync(join(tmpdir(), 'nexus-service-actor-'));
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

async function startActorService(home, port) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({
    home,
    host: '127.0.0.1',
    port,
    allowRemote: false,
    domainOnly: true,
  });
}

const CREATE_CHARACTER_BODY = {
  world_id: 'wld_owned',
  display_name: 'Actor Surface Character',
  persona: { voice: 'measured', wants: 'to be admitted, not simulated' },
};

describe('actor-http (P5-T2)', () => {
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
    service = await startActorService(home, 18_443);
    baseUrl = service.url;
  });

  after(async () => {
    if (service) await service.close();
  });

  test('every assigned Actor/memory/context route identity is mounted at its exact verb and tier', async () => {
    // Enumeration is deliberately separate from the behavioral proof below:
    // this asserts the composer inventory, not handler behavior.
    const { DOMAIN_ROUTES } = await import(join(serviceRoot, 'dist/routes.js'));
    const inventory = DOMAIN_ROUTES.map((route) => ({
      method: route.method,
      path: route.pattern.source.replace(/\\\//g, '/').replace(/^\^|\$$/g, ''),
      tier: route.tier,
      family: route.family,
    }));
    const required = [
      ['GET', '/v1/daemon/characters'],
      ['POST', '/v1/daemon/characters'],
      ['GET', '/v1/daemon/characters/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/archive'],
      ['POST', '/v1/daemon/characters/([^/]+)/restore'],
      ['POST', '/v1/daemon/characters/([^/]+)/bindings'],
      ['GET', '/v1/daemon/characters/([^/]+)/bindings'],
      ['GET', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/bindings/([^/]+)'],
      ['GET', '/v1/daemon/characters/([^/]+)/knowledge'],
      ['GET', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['PATCH', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/knowledge/([^/]+)'],
      ['POST', '/v1/daemon/actor-knowledge/view'],
      ['POST', '/v1/daemon/actor-knowledge/entries'],
      ['GET', '/v1/daemon/creators'],
      ['POST', '/v1/daemon/creators'],
      ['GET', '/v1/daemon/creators/active'],
      ['PUT', '/v1/daemon/creators/active'],
      ['GET', '/v1/daemon/creators/([^/]+)'],
      ['PATCH', '/v1/daemon/creators/([^/]+)'],
      ['POST', '/v1/daemon/creators/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/pending-review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/pending-review/count'],
      ['DELETE', '/v1/daemon/characters/([^/]+)/memory/pending-review/([^/]+)'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/review'],
      ['GET', '/v1/daemon/characters/([^/]+)/memory/fragments'],
      ['POST', '/v1/daemon/characters/([^/]+)/memory/fragments/([^/]+):promote'],
      ['POST', '/v1/daemon/characters/([^/]+)/soul/reflect'],
      ['POST', '/v1/daemon/characters/([^/]+)/tom'],
      ['GET', '/v1/daemon/characters/([^/]+)/tom'],
      ['GET', '/v1/daemon/memory/pending-review'],
      ['GET', '/v1/daemon/memory/pending-review/count'],
      ['DELETE', '/v1/daemon/memory/pending-review/([^/]+)'],
      ['POST', '/v1/daemon/memory/review'],
      ['GET', '/v1/daemon/memory/fragments'],
      ['POST', '/v1/daemon/memory/soul/reflect'],
      ['POST', '/v1/daemon/inspector/moment'],
      ['POST', '/v1/daemon/moment-directive'],
    ];
    const mounted = new Set(inventory.map((route) => `${route.method} ${route.path}`));
    for (const [method, path] of required) {
      assert.ok(
        mounted.has(`${method} ${path}`),
        `missing mounted identity: ${method} ${path}\nmounted:\n${[...mounted].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join('\n')}`,
      );
    }
    // Tier parity (daemon mod.rs authority): the Creator home family is
    // tier1 (API-key only, no active creator); every P5-T2 family route is
    // tier2 (creator-tier). (Runtime liveness reconciled unguarded in P5-T5
    // is out of this task's scope.)
    const OWN_FAMILIES = new Set(['actors', 'memory', 'context']);
    for (const route of inventory) {
      if (!OWN_FAMILIES.has(route.family)) continue;
      if (route.path.startsWith('/v1/daemon/creators')) {
        assert.equal(route.tier, 'tier1', `${route.method} ${route.path} must stay tier1`);
      } else {
        assert.equal(route.tier, 'tier2', `${route.method} ${route.path} must stay tier2`);
      }
    }
  });

  test('foreign actor and stale binding are denied by the real store with zero effect, and a valid Actor gets context', async () => {
    // 1. A legal owned Character + binding is created through the surface.
    const created = await jsonFetch(`${baseUrl}/v1/daemon/characters`, {
      method: 'POST',
      body: CREATE_CHARACTER_BODY,
    });
    assert.equal(created.status, 201, created.text);
    const characterId = created.payload.character?.character_id ?? created.payload.character_id;
    assert.ok(characterId, `character id missing: ${created.text}`);
    // Creation carries the initial active binding for the same World.
    const bindingId = created.payload.binding?.binding_id;
    assert.ok(bindingId, `initial binding id missing: ${created.text}`);

    // 2. A foreign Character ref (never owned by the active creator) is
    //    denied by the real native store — 404, existence hidden — and the
    //    denial produces zero provider/storage effects: the owned Character
    //    list is unchanged afterwards.
    const FOREIGN_CHARACTER = 'chr_' + 'f'.repeat(32);
    const foreign = await jsonFetch(`${baseUrl}/v1/daemon/characters/${FOREIGN_CHARACTER}`);
    assert.equal(foreign.status, 404, foreign.text);
    assert.equal(foreign.payload.error.code, 'not_found');
    const foreignView = await jsonFetch(`${baseUrl}/v1/daemon/actor-knowledge/view`, {
      method: 'POST',
      body: {
        actor_ref: { actor_kind: 'character', character_id: FOREIGN_CHARACTER },
        world_id: 'wld_owned',
        binding_id: bindingId,
      },
    });
    assert.equal(foreignView.status, 404, foreignView.text);

    // 3. A stale binding patch (wrong expected_revision) is the retained 409
    //    conflict, and nothing is written.
    const stale = await jsonFetch(
      `${baseUrl}/v1/daemon/characters/${characterId}/bindings/${bindingId}`,
      { method: 'PATCH', body: { expected_revision: 999, world_sheet_entry_id: 'wse_stale' } },
    );
    assert.equal(stale.status, 409, stale.text);
    const afterStale = await jsonFetch(
      `${baseUrl}/v1/daemon/characters/${characterId}/bindings/${bindingId}`,
    );
    assert.equal(afterStale.status, 200, afterStale.text);
    const liveBinding = afterStale.payload.binding ?? afterStale.payload;
    assert.equal(liveBinding.revision, 0, 'stale patch must not advance the binding revision');
    assert.ok(
      liveBinding.world_sheet_entry_id == null,
      'stale patch must not write the sheet',
    );

    // 4. A valid owned Actor gets real context: the admitted view over the
    //    owned World returns the paginated KnowledgeView (no 501).
    const view = await jsonFetch(`${baseUrl}/v1/daemon/actor-knowledge/view`, {
      method: 'POST',
      body: {
        actor_ref: { actor_kind: 'creator', creator_id: 'ctr_testcreator' },
        world_id: 'wld_owned',
      },
    });
    assert.equal(view.status, 200, view.text);
    assert.ok(Array.isArray(view.payload.items), 'admitted view must return items');
    assert.ok(view.payload.pagination, 'admitted view must return pagination');

    // 5. The retained logout verb: a POST without the `:logout` suffix is
    //    not a routed identity (daemon strips the suffix inside the shared
    //    `{creator_id}` segment and 404s otherwise).
    const bareLogout = await jsonFetch(`${baseUrl}/v1/daemon/creators/ctr_testcreator`, {
      method: 'POST',
    });
    assert.equal(bareLogout.status, 404, bareLogout.text);

    // 6. The moment context surface is real: the directive route answers on
    //    the owned World instead of a migration denial.
    const directive = await jsonFetch(`${baseUrl}/v1/daemon/moment-directive`, {
      method: 'POST',
      body: {
        action: 'show',
        scope: { kind: 'world', id: 'wld_owned' },
      },
    });
    assert.notEqual(directive.status, 501, directive.text);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// P0-T6 — the mounted HTTP Agent-Host Actor journey
//
// The service under test is the real provider-enabled composition from
// `dist/index.js`: one native Host authority, real ACP peers over stdio, no
// model, no network and no paid credential. Every Actor fixture (Worlds,
// Characters, bindings, sessions) is created through the HTTP surface itself,
// so nothing below can pass on a facade echo — the prompt text observed on the
// SSE stream is the peer's own deterministic transformation of the assembled
// prompt, and every terminal outcome comes from what the peer really replied.
//
// Deterministic peers:
//   - `crates/nexus-agent-host/tests/fixtures/mock_acp_workflow.py` (end_turn,
//     plus a never-answering family for cancel/shutdown),
//   - `actor-http-peer.py`, written into the throwaway home by this file, for
//     the exact non-success stop reasons, a mid-prompt EOF, and a terminal
//     emitted *after* an accepted cancel.
//
// Contract notes this file pins deliberately:
//   - The host config's own `max_sessions` (default 4) is raised in the
//     fixture's `config.toml`; the journey legitimately needs more live
//     sessions than the default budget, and the limit itself is not under test.
//   - `R-V1196-ACP-INCOMPLETE-MAPPING`: this adapter reports Refusal /
//     MaxTokens / MaxTurnRequests as `OpFailed(category)`, and the core's §5
//     table reads every `OpFailed` as the `failed` row — so the `incomplete`
//     rows are NOT reachable end-to-end through ACP. The three non-success stop
//     reasons below are therefore asserted as `failed`/null (never success);
//     the `incomplete` rows' own evidence stays the core group
//     (`character_terminal_*`). No test here fabricates an `incomplete`.
//   - `R-V1196-SESSION-CWD-BOUNDARY`: an Actor create must name a `cwd` inside
//     the pinned workspace root (the registered creative root), which is the
//     wire field's documented meaning; the no-cwd fallback is untouched.
// ─────────────────────────────────────────────────────────────────────────────

const JOURNEY_CREATOR = 'ctr_testcreator';
const JOURNEY_WORLD = 'wld_owned';
const FOREIGN_WORLD = 'wld_foreign';
const MAIN_PROVIDER = 'mock-acp-main';
const BLOCK_PROVIDER = 'mock-acp-block';
const LATE_PROVIDER = 'mock-acp-late';
const FAIL_PROVIDER = 'mock-acp-fail';
const STOP_REASON_PROVIDERS = {
  max_tokens: 'mock-acp-max-tokens',
  max_turn_requests: 'mock-acp-max-turn-requests',
  refusal: 'mock-acp-refusal',
};

/** Hermetic ACP peer for the terminal rows this journey must be able to drive. */
const ACTOR_HTTP_PEER = `#!/usr/bin/env python3
"""Deterministic local ACP peer for the P0-T6 Actor HTTP journey.

Speaks the real newline-delimited JSON-RPC ACP wire (initialize, session/new,
session/prompt, session/cancel) over stdio with no model and no network.

Modes (env):
  STOP_REASON=<snake_case>  reply to session/prompt with that stop reason
  BLOCK_PROMPT=1            never reply; on session/cancel return (or, with
                            LATE_END_TURN_AFTER_CANCEL=1, answer the still-open
                            prompt with end_turn after LATE_DELAY_S seconds)
  EXIT_ON_PROMPT=1          exit without replying (a mid-prompt EOF)
"""
import json
import os
import sys
import time

LOG = os.environ.get("ACP_FIXTURE_LOG")
STOP_REASON = os.environ.get("STOP_REASON") or "end_turn"
BLOCK_PROMPT = os.environ.get("BLOCK_PROMPT") == "1"
EXIT_ON_PROMPT = os.environ.get("EXIT_ON_PROMPT") == "1"
LATE_END_TURN_AFTER_CANCEL = os.environ.get("LATE_END_TURN_AFTER_CANCEL") == "1"
LATE_DELAY_S = float(os.environ.get("LATE_DELAY_S") or "1.5")


def log(entry):
    if LOG:
        with open(LOG, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry) + "\\n")


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\\n")
    sys.stdout.flush()


def reply(req, result):
    send({"jsonrpc": "2.0", "id": req.get("id"), "result": result})


def notify(method, params):
    send({"jsonrpc": "2.0", "method": method, "params": params})


def main():
    log({"event": "start", "pid": os.getpid()})
    pending = None
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = req.get("method")
        params = req.get("params") or {}
        log({"event": "request", "method": method})
        if method == "initialize":
            send({"jsonrpc": "2.0", "id": req.get("id"), "result": {
                "protocolVersion": 1,
                "agentCapabilities": {"loadSession": False},
                "agentInfo": {"name": "mock-acp-actor-http", "version": "1.0.0"},
            }})
        elif method == "session/new":
            session_id = "mock-session-%d" % os.getpid()
            log({"event": "session_new", "session_id": session_id})
            reply(req, {"sessionId": session_id})
        elif method == "session/prompt":
            prompt = ""
            for block in params.get("prompt", []):
                if block.get("type") == "text":
                    prompt += block.get("text", "")
            log({"event": "prompt", "prompt": prompt})
            if EXIT_ON_PROMPT:
                log({"event": "exit_on_prompt"})
                return
            if BLOCK_PROMPT:
                pending = req
                log({"event": "prompt_blocked"})
                for raw in sys.stdin:
                    raw = raw.strip()
                    if not raw:
                        continue
                    try:
                        cancel_req = json.loads(raw)
                    except json.JSONDecodeError:
                        continue
                    if cancel_req.get("method") == "session/cancel":
                        log({"event": "cancel"})
                        if LATE_END_TURN_AFTER_CANCEL:
                            time.sleep(LATE_DELAY_S)
                            log({"event": "late_end_turn"})
                            reply(pending, {"stopReason": "end_turn"})
                        return
                return
            notify("session/update", {
                "sessionId": params.get("sessionId"),
                "update": {"sessionUpdate": "agent_message_chunk",
                           "content": {"type": "text", "text": "peer:" + prompt}},
            })
            reply(req, {"stopReason": STOP_REASON})
        elif method == "session/cancel":
            log({"event": "cancel"})
        else:
            send({"jsonrpc": "2.0", "id": req.get("id"),
                  "error": {"code": -32601, "message": "method not found: %s" % method}})
    log({"event": "eof"})


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass
`;

function readLog(path) {
  try {
    return readFileSync(path, 'utf8').trim().split('\n').filter(Boolean).map((line) => JSON.parse(line));
  } catch {
    return [];
  }
}

function realpathOf(path) {
  return execFileSync('python3', ['-c', `import os,sys;print(os.path.realpath(sys.argv[1]))`, path], { encoding: 'utf8' }).trim();
}

/** One ephemeral home: seeded store + the selected-workspace registration the
 * product's create path writes (an engine-owner open pins that root as the
 * Host's workspace boundary) + the ACP families, with `max_sessions` raised so
 * the journey's live-session count is not the thing under test. */
function seedJourneyHome(label) {
  const home = mkdtempSync(join(tmpdir(), `nexus-actor-http-${label}-`));
  const seed = spawnSync(
    'cargo',
    ['run', '-q', '-p', 'nexus-core-node', '--bin', 'native-wire-fixture-seed', '--', home],
    { cwd: repoRoot, stdio: 'inherit' },
  );
  assert.equal(seed.status, 0, seed.stderr?.toString());
  const creativeRoot = join(home, 'creative', JOURNEY_CREATOR, 'default');
  mkdirSync(creativeRoot, { recursive: true });
  const operational = join(home, '.nexus42', 'creators', JOURNEY_CREATOR, 'workspaces', 'default');
  mkdirSync(operational, { recursive: true });
  writeFileSync(
    join(operational, 'meta.json'),
    JSON.stringify({
      schema_version: 1,
      creator_id: JOURNEY_CREATOR,
      workspace_slug: 'default',
      local_root: creativeRoot,
      workspace_id: null,
      created_at: '2020-01-01T00:00:00Z',
    }),
  );
  const peerPath = join(home, 'actor-http-peer.py');
  writeFileSync(peerPath, ACTOR_HTTP_PEER);
  const python = realpathOf(execFileSync('which', ['python3'], { encoding: 'utf8' }).trim());
  const acpLog = join(home, 'acp-fixture.log');
  const peerLog = join(home, 'actor-peer.log');
  const families = [
    [MAIN_PROVIDER, acpFixture, { ACP_FIXTURE_LOG: acpLog }],
    [BLOCK_PROVIDER, acpFixture, { ACP_FIXTURE_LOG: acpLog, BLOCK_PROMPT: '1' }],
    [LATE_PROVIDER, peerPath, { ACP_FIXTURE_LOG: peerLog, BLOCK_PROMPT: '1', LATE_END_TURN_AFTER_CANCEL: '1' }],
    [FAIL_PROVIDER, peerPath, { ACP_FIXTURE_LOG: peerLog, EXIT_ON_PROMPT: '1' }],
    ...Object.entries(STOP_REASON_PROVIDERS).map(([reason, id]) => [id, peerPath, { ACP_FIXTURE_LOG: peerLog, STOP_REASON: reason }]),
  ]
    .map(([id, script, env]) => {
      const envLines = Object.entries(env).map(([key, value]) => `${key} = ${JSON.stringify(value)}`).join('\n');
      return (
        `[[providers]]\nid = ${JSON.stringify(id)}\nprotocol = "acp"\n` +
        `command = ${JSON.stringify(python)}\nargs = [${JSON.stringify(script)}]\n` +
        `enabled = true\n[providers.env]\n${envLines}\n`
      );
    })
    .join('\n');
  const configDir = join(home, '.nexus42', 'agent-host');
  mkdirSync(configDir, { recursive: true });
  writeFileSync(join(configDir, 'config.toml'), `max_sessions = 16\n${families}`);
  return { home, creativeRoot, acpLog, peerLog };
}

async function startJourneyService(home) {
  const { startService } = await import(join(serviceRoot, 'dist/index.js'));
  return startService({ home, host: '127.0.0.1', port: 0, allowRemote: false, domainOnly: false });
}

function parseSseBody(text) {
  const frames = [];
  for (const block of text.split(/\r?\n\r?\n/)) {
    if (!block.trim()) continue;
    let id = '';
    let event = 'message';
    let data = '';
    for (const line of block.split(/\r?\n/)) {
      if (line.startsWith('id:')) id = line.slice(3).trim();
      else if (line.startsWith('event:')) event = line.slice(6).trim();
      else if (line.startsWith('data:')) data += line.slice(5).trim();
    }
    frames.push({ id, event, data: data ? JSON.parse(data) : null });
  }
  return frames;
}

describe('actor-http Agent-Host Actor journey (P0-T6)', { concurrency: 1 }, () => {
  let home;
  let creativeRoot;
  let acpLog;
  let peerLog;
  let service;
  let url;
  let worldB;
  let characterA;
  let bindingA;
  let characterB;
  let bindingB;
  let sessionA;

  const actorBody = (overrides = {}) => ({
    provider_id: MAIN_PROVIDER,
    cwd: creativeRoot,
    actor_ref: { actor_kind: 'character', character_id: characterA },
    viewpoint: { world_id: JOURNEY_WORLD, binding_id: bindingA },
    ...overrides,
  });

  async function jsonFetch(path, { method = 'GET', body, headers = {} } = {}) {
    const response = await fetch(`${url}${path}`, {
      method,
      headers: {
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
        ...headers,
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const text = await response.text();
    let payload = null;
    try {
      payload = text.length > 0 ? JSON.parse(text) : null;
    } catch {
      payload = null;
    }
    return { status: response.status, payload, text, headers: response.headers };
  }

  /** Poll the authority's own Character read until the run leaves `running`. */
  async function waitForCharacterOutcome(operationId) {
    const deadline = Date.now() + 30_000;
    for (;;) {
      const got = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`);
      if (got.payload?.run_status && got.payload.run_status !== 'running') return got.payload;
      assert.ok(Date.now() < deadline, `operation ${operationId} never settled: ${got.text}`);
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }

  /** Poll the authority's session row until its live operation is retired. */
  async function waitForIdleSession(sessionId) {
    const deadline = Date.now() + 30_000;
    for (;;) {
      const got = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}`);
      if (got.status === 200 && !got.payload.active_op_id) return;
      assert.ok(Date.now() < deadline, `session ${sessionId} never went idle: ${got.text}`);
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }

  async function sseBody(sessionId, operationId) {
    const response = await fetch(
      `${url}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`,
      { headers: { Accept: 'text/event-stream' }, signal: AbortSignal.timeout(20_000) },
    );
    return { status: response.status, frames: parseSseBody(await response.text()) };
  }

  before(async () => {
    ({ home, creativeRoot, acpLog, peerLog } = seedJourneyHome('journey'));
    const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], { cwd: serviceRoot, stdio: 'inherit' });
    assert.equal(build.status, 0, 'service TypeScript build must succeed');
    service = await startJourneyService(home);
    url = service.url;
    const status = await jsonFetch('/v1/daemon/runtime/status');
    assert.equal(status.payload.runtime_mode, 'provider_enabled');

    const world = await jsonFetch('/v1/daemon/worlds', { method: 'POST', body: { title: 'Journey World B' } });
    assert.equal(world.status, 201, world.text);
    worldB = world.payload.world_id;
    assert.match(worldB, /^wld_[a-zA-Z0-9]+$/);

    const a = await jsonFetch('/v1/daemon/characters', {
      method: 'POST',
      body: { world_id: JOURNEY_WORLD, display_name: 'Journey Actor A', persona: { voice: 'plain' } },
    });
    assert.equal(a.status, 201, a.text);
    characterA = a.payload.character.character_id;
    bindingA = a.payload.binding.binding_id;

    const b = await jsonFetch('/v1/daemon/characters', {
      method: 'POST',
      body: { world_id: worldB, display_name: 'Journey Actor B', persona: { voice: 'plain' } },
    });
    assert.equal(b.status, 201, b.text);
    characterB = b.payload.character.character_id;
    bindingB = b.payload.binding.binding_id;
    assert.match(characterA, /^chr_[0-9a-f]{32}$/);
    assert.match(bindingA, /^awb_[0-9a-f]{32}$/);
    assert.notEqual(characterA, characterB);
    assert.notEqual(bindingA, bindingB);

    sessionA = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(sessionA.status, 200, sessionA.text);
  });

  after(async () => {
    if (service) await service.close();
  });

  test('an Actor session is created from the stored pair, echoes it, and reuses its exact key', async () => {
    const created = sessionA.payload;
    assert.match(created.session_id, /^[0-9a-f-]{36}$/);
    assert.equal(created.provider_id, MAIN_PROVIDER);
    assert.equal(created.state, 'Ready');
    assert.deepEqual(created.actor_ref, { actor_kind: 'character', character_id: characterA });
    assert.deepEqual(created.viewpoint, { world_id: JOURNEY_WORLD, binding_id: bindingA });
    assert.equal(created.active_op_id, undefined, 'a fresh session is not busy');

    // GET and list keep the Actor echo: an Actor session is never downgraded to
    // a provider-only session by a later read.
    const got = await jsonFetch(`/v1/daemon/agent-host/sessions/${created.session_id}`);
    assert.equal(got.status, 200, got.text);
    assert.deepEqual(got.payload.actor_ref, created.actor_ref);
    assert.deepEqual(got.payload.viewpoint, created.viewpoint);
    const listed = await jsonFetch('/v1/daemon/agent-host/sessions');
    assert.equal(listed.status, 200, listed.text);
    const row = listed.payload.items.find((item) => item.session_id === created.session_id);
    assert.ok(row, 'the Actor session must appear in the merged list');
    assert.deepEqual(row.actor_ref, created.actor_ref);

    // Exact reuse key (provider, canonical root, model/mode, Actor, World/binding
    // and holder/knowledge fingerprint) returns the SAME ready session.
    const reused = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(reused.status, 200, reused.text);
    assert.equal(reused.payload.session_id, created.session_id);

    // A different Character on a different World is a different session: the two
    // pairs never collapse into one journal or one cache row.
    const other = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: {
        provider_id: MAIN_PROVIDER,
        cwd: creativeRoot,
        actor_ref: { actor_kind: 'character', character_id: characterB },
        viewpoint: { world_id: worldB, binding_id: bindingB },
      },
    });
    assert.equal(other.status, 200, other.text);
    assert.notEqual(other.payload.session_id, created.session_id);
    assert.deepEqual(other.payload.viewpoint, { world_id: worldB, binding_id: bindingB });
  });

  test('a foreign, missing, mismatched or inactive Actor/World/binding is refused before any launch', async () => {
    const launchesBefore = readLog(acpLog).length;
    /** @type {Array<[Record<string, unknown>, number, string]>} */
    const rejections = [
      // Malformed, null and partial pairs (both-or-neither is enforced by
      // own-property presence, so an explicit `null` is present-and-invalid).
      [actorBody({ actor_ref: null, viewpoint: null }), 400, 'invalid_input'],
      [{ provider_id: MAIN_PROVIDER, cwd: creativeRoot, actor_ref: { actor_kind: 'character', character_id: characterA } }, 400, 'invalid_input'],
      [actorBody({ actor_ref: { actor_kind: 'character', character_id: 'chr_bad' } }), 400, 'invalid_input'],
      [actorBody({ actor_ref: { actor_kind: 'nope', character_id: characterA } }), 400, 'invalid_input'],
      [actorBody({ viewpoint: { world_id: 'wld_bad-1', binding_id: bindingA } }), 400, 'invalid_input'],
      [actorBody({ viewpoint: { world_id: JOURNEY_WORLD, binding_id: null } }), 400, 'invalid_input'],
      [actorBody({ viewpoint: { world_id: JOURNEY_WORLD, binding_id: bindingA, extra: 1 } }), 400, 'invalid_input'],
      [actorBody({ nope: 1 }), 400, 'invalid_input'],
      // A foreign Creator ref (valid shape, never this principal's) and a
      // Creator ref that carries a Character binding are store-level refusals.
      [actorBody({ actor_ref: { actor_kind: 'creator', creator_id: 'ctr_othercreator' }, viewpoint: { world_id: JOURNEY_WORLD } }), 404, 'not_found'],
      [actorBody({ actor_ref: { actor_kind: 'creator', creator_id: JOURNEY_CREATOR }, viewpoint: { world_id: JOURNEY_WORLD, binding_id: bindingA } }), 400, 'invalid_input'],
      // Foreign World, missing World, missing Character, mismatched binding.
      [actorBody({ viewpoint: { world_id: FOREIGN_WORLD, binding_id: bindingA } }), 404, 'not_found'],
      [actorBody({ viewpoint: { world_id: 'wld_missingworld', binding_id: bindingA } }), 404, 'not_found'],
      [actorBody({ actor_ref: { actor_kind: 'character', character_id: `chr_${'a'.repeat(32)}` } }), 404, 'not_found'],
      [actorBody({ viewpoint: { world_id: JOURNEY_WORLD, binding_id: bindingB } }), 404, 'not_found'],
      [actorBody({ viewpoint: { world_id: JOURNEY_WORLD } }), 400, 'invalid_input'],
    ];
    for (const [body, status, code] of rejections) {
      const res = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body });
      assert.equal(res.status, status, `expected ${status} for ${JSON.stringify(body)}: ${res.text}`);
      assert.equal(res.payload.error.code, code, res.text);
    }
    assert.equal(readLog(acpLog).length, launchesBefore, 'a refused Actor must never launch a provider');

    // Inactive: an archived Character is a typed conflict for both a new create
    // and a prompt on the session that admitted it. The restore is in a
    // `finally` so a failed assertion cannot leave the fixture archived for the
    // tests that follow.
    const detail = await jsonFetch(`/v1/daemon/characters/${characterA}`);
    assert.equal(detail.status, 200, detail.text);
    const archived = await jsonFetch(`/v1/daemon/characters/${characterA}/archive`, {
      method: 'POST',
      body: { expected_revision: detail.payload.character.revision },
    });
    assert.equal(archived.status, 200, archived.text);
    const archivedRevision = archived.payload.character.revision;
    try {
      const inactiveCreate = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
      assert.equal(inactiveCreate.status, 409, inactiveCreate.text);
      assert.equal(inactiveCreate.payload.error.code, 'owner_busy');
      assert.equal(inactiveCreate.payload.error.details.conflict_code, 'character_inactive');
      const inactivePrompt = await jsonFetch(
        `/v1/daemon/agent-host/sessions/${sessionA.payload.session_id}/operations`,
        { method: 'POST', body: { kind: 'prompt', content: 'archived?' } },
      );
      assert.equal(inactivePrompt.status, 409, inactivePrompt.text);
      assert.equal(inactivePrompt.payload.error.details.conflict_code, 'character_inactive');
      assert.equal(readLog(acpLog).length, launchesBefore, 'an inactive Actor must never launch a provider');
    } finally {
      const restored = await jsonFetch(`/v1/daemon/characters/${characterA}/restore`, {
        method: 'POST',
        body: { expected_revision: archivedRevision },
      });
      assert.equal(restored.status, 200, restored.text);
      assert.equal(restored.payload.character.status, 'active');
    }
  });

  test('lifecycle and knowledge invalidation retire the session and the next create admits a fresh one', async () => {
    // Restore above changed the Character lifecycle epoch: the reuse key no
    // longer matches, so the next create admits a fresh session instead of
    // reusing the pre-archive one.
    const fresh = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(fresh.status, 200, fresh.text);
    assert.notEqual(fresh.payload.session_id, sessionA.payload.session_id);
    const freshSession = fresh.payload.session_id;

    const first = await jsonFetch(`/v1/daemon/agent-host/sessions/${freshSession}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'before governance' },
    });
    assert.equal(first.status, 200, first.text);
    assert.equal((await waitForCharacterOutcome(first.payload.operation_id)).run_status, 'succeeded');

    // A material governance change moves the World's stored knowledge revision;
    // the next prompt re-reads it, retires the session in place and refuses.
    const patch = await jsonFetch(`/v1/daemon/worlds/${JOURNEY_WORLD}/kb/patch-entity`, {
      method: 'POST',
      body: { entity_id: 'kb_mod', expected_version: 0, patch: { title: 'Journey Updated Mod' } },
    });
    assert.equal(patch.status, 200, patch.text);
    const stale = await jsonFetch(`/v1/daemon/agent-host/sessions/${freshSession}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'after governance' },
    });
    assert.equal(stale.status, 409, stale.text);
    assert.equal(stale.payload.error.code, 'owner_busy');
    assert.equal(stale.payload.error.details.conflict_code, 'actor_session_stale');

    // The retired id is never reused: the next create admits a new session with
    // the fresh knowledge fingerprint, and it runs normally.
    const after = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(after.status, 200, after.text);
    assert.notEqual(after.payload.session_id, freshSession);
    const run = await jsonFetch(`/v1/daemon/agent-host/sessions/${after.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'after governance fresh' },
    });
    assert.equal(run.status, 200, run.text);
    const outcome = await waitForCharacterOutcome(run.payload.operation_id);
    assert.equal(outcome.run_status, 'succeeded');
    assert.equal(outcome.finish_reason, 'end_turn');
    assert.deepEqual(outcome.capture, { status: 'disabled', pending_id: null, code: null });
  });

  test('a Character prompt reports the authority outcome, and a non-success stop is never success', async () => {
    // end_turn: the real success row, with the peer's own transformed text on
    // the stream and `capture` disabled (this host has no capture writer).
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const executed = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'journey-echo' },
    });
    assert.equal(executed.status, 200, executed.text);
    assert.deepEqual(Object.keys(executed.payload).sort(), ['operation_id', 'session_id', 'status']);
    assert.equal(executed.payload.status, 'started');
    assert.equal(executed.payload.session_id, session.payload.session_id);
    const success = await waitForCharacterOutcome(executed.payload.operation_id);
    assert.equal(success.run_status, 'succeeded');
    assert.equal(success.finish_reason, 'end_turn');
    assert.deepEqual(success.capture, { status: 'disabled', pending_id: null, code: null });
    const stream = await sseBody(session.payload.session_id, executed.payload.operation_id);
    assert.equal(stream.status, 200);
    const terminal = stream.frames.filter((frame) => frame.event === 'provider_event' && frame.data?.OpFinished);
    assert.equal(terminal.length, 1, 'exactly one terminal frame');
    assert.equal(terminal[0].data.OpFinished.reason, 'end_turn');
    const delta = stream.frames.find((frame) => frame.data?.MessageDelta);
    assert.ok(delta, 'the peer message delta must be delivered');
    assert.match(delta.data.MessageDelta.text, /^transformed:journey-echo/);

    // The ACP adapter reports Refusal / MaxTokens / MaxTurnRequests as
    // `OpFailed(category)`, and §5 reads every `OpFailed` as the failure row, so
    // each of the three must settle `failed`/null — never `succeeded` and never
    // an `incomplete` this adapter cannot produce (R-V1196-ACP-INCOMPLETE-MAPPING).
    for (const providerId of Object.values(STOP_REASON_PROVIDERS)) {
      const row = await jsonFetch('/v1/daemon/agent-host/sessions', {
        method: 'POST',
        body: actorBody({ provider_id: providerId }),
      });
      assert.equal(row.status, 200, `${providerId}: ${row.text}`);
      const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${row.payload.session_id}/operations`, {
        method: 'POST',
        body: { kind: 'prompt', content: `row-${providerId}` },
      });
      assert.equal(prompt.status, 200, `${providerId}: ${prompt.text}`);
      const outcome = await waitForCharacterOutcome(prompt.payload.operation_id);
      assert.equal(outcome.run_status, 'failed', `${providerId} must not be reported as success`);
      assert.equal(outcome.finish_reason, null);
    }

    // A mid-prompt EOF is the fault row too.
    const failing = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: actorBody({ provider_id: FAIL_PROVIDER }),
    });
    assert.equal(failing.status, 200, failing.text);
    const brokenPrompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${failing.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'eof' },
    });
    assert.equal(brokenPrompt.status, 200, brokenPrompt.text);
    const broken = await waitForCharacterOutcome(brokenPrompt.payload.operation_id);
    assert.equal(broken.run_status, 'failed');
    assert.equal(broken.finish_reason, null);
  });

  test('an accepted cancel is the operation truth and a late provider terminal cannot rewrite it', async () => {
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: actorBody({ provider_id: LATE_PROVIDER }),
    });
    assert.equal(session.status, 200, session.text);
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'cancel-me' },
    });
    assert.equal(prompt.status, 200, prompt.text);

    // Exact-key reuse is ready-only: while this session's run is live, the same
    // key is a Busy conflict, never a second session and never a second launch.
    const launchesBeforeBusy = readLog(acpLog).length + readLog(peerLog).length;
    const busy = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: actorBody({ provider_id: LATE_PROVIDER }),
    });
    assert.equal(busy.status, 409, busy.text);
    assert.equal(busy.payload.error.code, 'owner_busy');
    assert.equal(busy.payload.error.details.conflict_code, 'actor_session_busy');
    assert.equal(readLog(acpLog).length + readLog(peerLog).length, launchesBeforeBusy, 'a Busy reuse must not launch');

    const cancel = await jsonFetch(`/v1/daemon/agent-host/operations/${prompt.payload.operation_id}`, {
      method: 'POST',
      body: {},
    });
    assert.equal(cancel.status, 200, cancel.text);
    assert.deepEqual(cancel.payload, { operation_id: prompt.payload.operation_id, status: 'cancelled' });
    const cancelled = await waitForCharacterOutcome(prompt.payload.operation_id);
    assert.equal(cancelled.run_status, 'cancelled');
    assert.equal(cancelled.finish_reason, 'cancelled');

    // The peer really does answer the already-cancelled prompt with end_turn
    // (its own log is the evidence); canonical truth must not move.
    const late = await (async () => {
      const deadline = Date.now() + 15_000;
      for (;;) {
        const entry = readLog(peerLog).find((line) => line.event === 'late_end_turn');
        if (entry) return entry;
        assert.ok(Date.now() < deadline, 'the peer never produced its late terminal');
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
    })();
    assert.equal(late.event, 'late_end_turn');
    const afterLate = await jsonFetch(`/v1/daemon/agent-host/operations/${prompt.payload.operation_id}`);
    assert.equal(afterLate.payload.run_status, 'cancelled', 'a late terminal must not rewrite an accepted cancel');

    // A second cancel is the finished conflict, not another provider effect.
    const again = await jsonFetch(`/v1/daemon/agent-host/operations/${prompt.payload.operation_id}`, {
      method: 'POST',
      body: {},
    });
    assert.equal(again.status, 409, again.text);
    assert.equal(again.payload.error.code, 'owner_busy');
    assert.equal(again.payload.error.details.conflict_code, 'actor_operation_finished');
  });

  test('a Character run settles with no SSE subscriber, and the retained observation still replays', async () => {
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'unwatched' },
    });
    assert.equal(prompt.status, 200, prompt.text);
    // Nothing is subscribed: the authority's own drain still settles the run.
    const outcome = await waitForCharacterOutcome(prompt.payload.operation_id);
    assert.equal(outcome.run_status, 'succeeded');
    assert.equal(outcome.finish_reason, 'end_turn');
    // The bounded observation was retained, so a later subscriber still gets the
    // real frames (and no fabricated terminal).
    const stream = await sseBody(session.payload.session_id, prompt.payload.operation_id);
    assert.equal(stream.status, 200);
    assert.ok(
      stream.frames.some((frame) => frame.data?.MessageDelta?.text?.includes('transformed:unwatched')),
      'the retained peer frame must replay',
    );
    assert.equal(
      stream.frames.filter((frame) => frame.data?.OpFinished).length,
      1,
      'the retained terminal must replay exactly once',
    );
  });

  test('SSE keeps the Actor lane on a cold mirror and enforces per-session association', async () => {
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'cold-mirror' },
    });
    const operationId = prompt.payload.operation_id;
    const outcome = await waitForCharacterOutcome(operationId);
    assert.equal(outcome.run_status, 'succeeded');

    // Drop every process-local mirror row for the session and operation: the
    // authority's own truth must still authorize GET/SSE/cancel/delete as Actor.
    service.service.providerRegistry.removeSession(session.payload.session_id);
    assert.equal(service.service.providerRegistry.sessionRecord(session.payload.session_id), undefined);
    assert.equal(service.service.providerRegistry.operationRecord(operationId), undefined);

    const got = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}`);
    assert.equal(got.status, 200, got.text);
    assert.deepEqual(got.payload.actor_ref, { actor_kind: 'character', character_id: characterA });
    assert.deepEqual(got.payload.viewpoint, { world_id: JOURNEY_WORLD, binding_id: bindingA });

    const inspector = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(inspector.status, 200, inspector.text);
    assert.equal(inspector.payload.run_status, 'succeeded');
    assert.equal(inspector.payload.session_id, session.payload.session_id);

    const stream = await sseBody(session.payload.session_id, operationId);
    assert.equal(stream.status, 200);
    assert.ok(stream.frames.some((frame) => frame.data?.MessageDelta?.text?.includes('transformed:cold-mirror')));

    // A cross-session operation is refused before headers, even in Actor mode.
    // A different provider id is what makes this a different session: the same
    // pair would legitimately reuse the same ready session.
    const other = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: actorBody({ provider_id: BLOCK_PROVIDER }),
    });
    assert.equal(other.status, 200, other.text);
    assert.notEqual(other.payload.session_id, session.payload.session_id);
    const cross = await fetch(
      `${url}/v1/daemon/agent-host/sessions/${other.payload.session_id}/events?operation_id=${operationId}`,
      { headers: { Accept: 'text/event-stream' } },
    );
    assert.equal(cross.status, 403);
    await cross.body?.cancel();

    // A finished Character operation is refused a cancel through the authority.
    const finishedCancel = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`, { method: 'POST', body: {} });
    assert.equal(finishedCancel.status, 409, finishedCancel.text);
    assert.equal(finishedCancel.payload.error.details.conflict_code, 'actor_operation_finished');

    const removed = await jsonFetch(`/v1/daemon/agent-host/sessions/${session.payload.session_id}`, { method: 'DELETE' });
    assert.equal(removed.status, 200, removed.text);
    assert.equal(removed.payload.status, 'shutdown');
  });

  test('remember absent or false captures nothing, a Character remember is refused before effects, and the legacy refusal stays', async () => {
    const memoryBefore = await jsonFetch(`/v1/daemon/characters/${characterA}/memory/pending-review/count`);
    assert.equal(memoryBefore.payload.count, 0);

    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const sessionId = session.payload.session_id;

    // remember:true on an admitted Character session is a pre-effect refusal:
    // no reservation, no provider work, no pending/captured result.
    const launchesBefore = readLog(acpLog).length;
    const refused = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'capture?', remember: true },
    });
    assert.equal(refused.status, 501, refused.text);
    assert.equal(refused.payload.error.code, 'route_not_migrated');
    assert.equal(readLog(acpLog).length, launchesBefore, 'a refused remember must not reach the provider');

    // remember:false and absent both execute and capture nothing.
    for (const body of [
      { kind: 'prompt', content: 'no capture (false)', remember: false },
      { kind: 'prompt', content: 'no capture (absent)' },
    ]) {
      const run = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, { method: 'POST', body });
      assert.equal(run.status, 200, run.text);
      const outcome = await waitForCharacterOutcome(run.payload.operation_id);
      assert.equal(outcome.run_status, 'succeeded');
      assert.deepEqual(outcome.capture, { status: 'disabled', pending_id: null, code: null });
    }

    // Legacy provider-only sessions keep the retained `422 invalid_input`.
    const legacy = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: { provider_id: MAIN_PROVIDER, cwd: creativeRoot },
    });
    assert.equal(legacy.status, 200, legacy.text);
    const legacyRemember = await jsonFetch(`/v1/daemon/agent-host/sessions/${legacy.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'legacy capture', remember: true },
    });
    assert.equal(legacyRemember.status, 422, legacyRemember.text);
    assert.equal(legacyRemember.payload.error.code, 'invalid_input');

    // Zero memory effects, observed through the real Character memory surface.
    const memoryAfter = await jsonFetch(`/v1/daemon/characters/${characterA}/memory/pending-review/count`);
    assert.equal(memoryAfter.payload.count, 0, 'no Character prompt may write memory');
    const fragments = await jsonFetch(`/v1/daemon/characters/${characterA}/memory/fragments`);
    assert.equal(fragments.payload.fragments.length, 0, 'no Character prompt may create memory fragments');
  });

  test('session shutdown in flight settles the run and retires the session', async () => {
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: actorBody({ provider_id: BLOCK_PROVIDER }),
    });
    assert.equal(session.status, 200, session.text);
    const sessionId = session.payload.session_id;
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'shut me down' },
    });
    assert.equal(prompt.status, 200, prompt.text);
    const shutdown = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}`, { method: 'DELETE' });
    assert.equal(shutdown.status, 200, shutdown.text);
    assert.deepEqual(shutdown.payload, { session_id: sessionId, status: 'shutdown' });

    const outcome = await waitForCharacterOutcome(prompt.payload.operation_id);
    assert.equal(outcome.run_status, 'cancelled');
    assert.equal(outcome.finish_reason, 'cancelled');

    const gone = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}`);
    assert.equal(gone.status, 404, gone.text);
    const afterShutdown = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'again' },
    });
    assert.equal(afterShutdown.status, 404, afterShutdown.text);
    assert.equal(afterShutdown.payload.error.code, 'not_found');
  });

  test('the provider-only lane and the unsupported model/mode refusals are unchanged', async () => {
    // A legacy create omits both actor_ref and viewpoint: unchanged lane.
    const created = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: { provider_id: MAIN_PROVIDER, cwd: creativeRoot },
    });
    assert.equal(created.status, 200, created.text);
    assert.deepEqual(Object.keys(created.payload).sort(), ['provider_id', 'session_id', 'state']);
    const executed = await jsonFetch(`/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'legacy prompt' },
    });
    assert.equal(executed.status, 200, executed.text);
    // The generic OperationResponse — not a Character result — is preserved for
    // a provider-only operation, including its inspect shape.
    const inspect = await jsonFetch(`/v1/daemon/agent-host/operations/${executed.payload.operation_id}`);
    assert.equal(inspect.status, 200, inspect.text);
    assert.deepEqual(Object.keys(inspect.payload).sort(), ['operation_id', 'session_id', 'status']);
    assert.ok(['started', 'running', 'finished'].includes(inspect.payload.status), inspect.payload.status);
    const stream = await sseBody(created.payload.session_id, executed.payload.operation_id);
    assert.equal(stream.status, 200);
    assert.ok(stream.frames.some((frame) => frame.data?.MessageDelta?.text?.includes('transformed:legacy prompt')));

    for (const body of [{ kind: 'set_model', model: 'm' }, { kind: 'set_mode', mode: 'x' }]) {
      const refused = await jsonFetch(`/v1/daemon/agent-host/sessions/${created.payload.session_id}/operations`, {
        method: 'POST',
        body,
      });
      assert.equal(refused.status, 501, refused.text);
      assert.equal(refused.payload.error.code, 'route_not_migrated');
    }
  });

  test('a valid Creator Actor session takes the core arm for prompt, cancel and observation', async () => {
    const creatorBody = (providerId) => ({
      provider_id: providerId,
      cwd: creativeRoot,
      actor_ref: { actor_kind: 'creator', creator_id: JOURNEY_CREATOR },
      viewpoint: { world_id: JOURNEY_WORLD },
    });
    const cancelsBefore = readLog(acpLog).filter((entry) => entry.event === 'cancel').length;

    // 1. Both Actor kinds are admitted by the ONE core Host authority: a stored
    //    Creator ref forbids a binding and echoes the pair (contract §4).
    const blocked = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: creatorBody(BLOCK_PROVIDER),
    });
    assert.equal(blocked.status, 200, blocked.text);
    assert.deepEqual(blocked.payload.actor_ref, { actor_kind: 'creator', creator_id: JOURNEY_CREATOR });
    assert.deepEqual(blocked.payload.viewpoint, { world_id: JOURNEY_WORLD });
    const blockedSessionId = blocked.payload.session_id;

    // 2. The prompt is a core Actor effect, and while it is the session's live
    //    work a Creator Actor operation keeps the GENERIC observation
    //    (contract §5): the authority's own operation row — never a Character
    //    result and never the provider-only mirror.
    const blockedPrompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${blockedSessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'creator-cancel' },
    });
    assert.equal(blockedPrompt.status, 200, blockedPrompt.text);
    const blockedOperationId = blockedPrompt.payload.operation_id;
    const live = await jsonFetch(`/v1/daemon/agent-host/operations/${blockedOperationId}`);
    assert.equal(live.status, 200, live.text);
    assert.deepEqual(Object.keys(live.payload).sort(), ['operation_id', 'session_id', 'status']);
    assert.equal(live.payload.session_id, blockedSessionId);
    assert.equal(live.payload.status, 'running');

    // 3. Cancel reaches the AUTHORITY. This host's core retains no cancellable
    //    Creator row, so its own `not_found` is the answer — not the raw provider
    //    cancel lane (which answered 500/internal before this fix) and not a
    //    provider cancel the peer would have observed.
    const cancelLive = await jsonFetch(`/v1/daemon/agent-host/operations/${blockedOperationId}`, {
      method: 'POST',
      body: {},
    });
    assert.equal(cancelLive.status, 404, cancelLive.text);
    assert.equal(cancelLive.payload.error.code, 'not_found');
    assert.equal(
      readLog(acpLog).filter((entry) => entry.event === 'cancel').length,
      cancelsBefore,
      'an Actor operation id must never reach the provider-only cancel lane',
    );

    // 4. Observation: the authority retains no event stream for a Creator Actor
    //    prompt, so the Actor arm answers with its bounded resync gap to
    //    canonical truth — not the 404 the Character-only narrowing produced,
    //    and without ever pulling the provider-only stream.
    const { sseTestHooks } = await import(join(serviceRoot, 'dist/sse.js'));
    const pullsBefore = sseTestHooks.providerPullCount;
    const stream = await sseBody(blockedSessionId, blockedOperationId);
    assert.equal(stream.status, 200);
    const gap = stream.frames.find((frame) => frame.event === 'gap');
    assert.ok(gap, `the Creator Actor arm must report a bounded resync gap: ${JSON.stringify(stream.frames)}`);
    assert.equal(gap.data.reason, 'history_unavailable');
    assert.equal(gap.data.resync_required, true);
    assert.equal(gap.data.inspect_url, `/v1/daemon/agent-host/operations/${blockedOperationId}`);
    assert.equal(sseTestHooks.providerPullCount, pullsBefore, 'the Actor lane must not pull the provider stream');

    // 5. Once the authority stops serving its row the operation is absent (404),
    //    never the mirror's stale `started` admission row.
    const settledSession = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: creatorBody(MAIN_PROVIDER),
    });
    assert.equal(settledSession.status, 200, settledSession.text);
    const settledPrompt = await jsonFetch(
      `/v1/daemon/agent-host/sessions/${settledSession.payload.session_id}/operations`,
      { method: 'POST', body: { kind: 'prompt', content: 'creator-echo' } },
    );
    assert.equal(settledPrompt.status, 200, settledPrompt.text);
    await waitForIdleSession(settledSession.payload.session_id);
    const settled = await jsonFetch(`/v1/daemon/agent-host/operations/${settledPrompt.payload.operation_id}`);
    assert.equal(settled.status, 404, settled.text);
    assert.equal(settled.payload.error.code, 'not_found');
    // The observe path is the same absence: no retained stream exists for an
    // operation the authority no longer serves, so the stream is refused before
    // headers instead of hanging or fabricating frames.
    const settledStream = await fetch(
      `${url}/v1/daemon/agent-host/sessions/${settledSession.payload.session_id}/events?operation_id=${settledPrompt.payload.operation_id}`,
      { headers: { Accept: 'text/event-stream' } },
    );
    assert.equal(settledStream.status, 404);
    await settledStream.body?.cancel();

    await jsonFetch(`/v1/daemon/agent-host/sessions/${blockedSessionId}`, { method: 'DELETE' });
    await jsonFetch(`/v1/daemon/agent-host/sessions/${settledSession.payload.session_id}`, { method: 'DELETE' });
  });

  test('an Actor operation the authority no longer retains is refused, never downgraded to the provider-only lane', async () => {
    const cancelsBefore = readLog(acpLog).filter((entry) => entry.event === 'cancel').length;
    const { hostQuery } = await import(join(serviceRoot, 'dist/world-kb.js'));

    // 1. A real Creator Actor operation the authority answers for only while it
    //    is the session's live work: once it settles, the authority retains no
    //    row of any kind for it (no Character outcome, no operation row) while
    //    the mirror keeps its Actor mark. That is the aged state under test.
    const created = await jsonFetch('/v1/daemon/agent-host/sessions', {
      method: 'POST',
      body: {
        provider_id: MAIN_PROVIDER,
        cwd: creativeRoot,
        actor_ref: { actor_kind: 'creator', creator_id: JOURNEY_CREATOR },
        viewpoint: { world_id: JOURNEY_WORLD },
      },
    });
    assert.equal(created.status, 200, created.text);
    const sessionId = created.payload.session_id;
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'aged-out-actor' },
    });
    assert.equal(prompt.status, 200, prompt.text);
    const operationId = prompt.payload.operation_id;
    await waitForIdleSession(sessionId);

    // Precondition, observed through the authority itself: nothing is retained.
    const nativeRow = await hostQuery(service.service, { query: 'get_operation', operation_id: operationId })
      .then((response) => response.operation ?? null)
      .catch((error) => (error?.code === 'not_found' ? null : Promise.reject(error)));
    assert.equal(nativeRow, null, 'precondition: the authority retains no operation row');
    assert.equal(
      service.service.providerRegistry.operationRecord(operationId)?.actorBacked,
      true,
      'precondition: the mirror still carries the Actor mark',
    );

    // 2. Fresh native absence must NOT read as provider-only ownership: the
    //    stale Actor record is refused on both the inspect and cancel paths.
    const got = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(got.status, 404, got.text);
    assert.equal(got.payload.error.code, 'not_found');
    const cancel = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`, { method: 'POST', body: {} });
    assert.equal(cancel.status, 404, cancel.text);
    assert.equal(cancel.payload.error.code, 'not_found');

    // 3. The same rule for a Character Actor record core's bounded terminal index
    //    has already evicted: the eviction is simulated with an id the authority
    //    never saw (driving 1024 terminal operations is not a fixture), and the
    //    endpoint behaviour under test does not depend on how the record aged.
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const evictedOperationId = randomUUID();
    service.service.providerRegistry.registerOperation({
      operationId: evictedOperationId,
      sessionId: session.payload.session_id,
      providerId: MAIN_PROVIDER,
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
    const evictedGot = await jsonFetch(`/v1/daemon/agent-host/operations/${evictedOperationId}`);
    assert.equal(evictedGot.status, 404, evictedGot.text);
    const evictedCancel = await jsonFetch(`/v1/daemon/agent-host/operations/${evictedOperationId}`, {
      method: 'POST',
      body: {},
    });
    assert.equal(evictedCancel.status, 404, evictedCancel.text);
    assert.equal(
      readLog(acpLog).filter((entry) => entry.event === 'cancel').length,
      cancelsBefore,
      'an expired Actor operation must never reach the provider-only cancel lane',
    );
  });

  test('settled-but-unobserved Actor operations are aged out of the transport mirror', async () => {
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const sessionId = session.payload.session_id;
    const tracked = [];
    const retained = () => tracked.filter((id) => service.service.providerRegistry.operationRecord(id) !== undefined).length;

    for (let index = 0; index < 6; index += 1) {
      const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
        method: 'POST',
        body: { kind: 'prompt', content: `unobserved-${index}` },
      });
      assert.equal(prompt.status, 200, prompt.text);
      tracked.push(prompt.payload.operation_id);
      // No SSE subscriber anywhere: the authority's own drain still settles the
      // run, and nothing else can tell the mirror it is over.
      assert.equal((await waitForCharacterOutcome(prompt.payload.operation_id)).run_status, 'succeeded');
      for (const earlier of tracked.slice(0, -1)) {
        assert.equal(
          service.service.providerRegistry.operationRecord(earlier),
          undefined,
          `settled unobserved Actor operation ${earlier} must not outlive the core's retention`,
        );
      }
      assert.ok(retained() <= 2, `the Actor arm must stay bounded: ${retained()} records after ${index + 1} prompts`);
    }
    for (const earlier of tracked.slice(0, -1)) {
      assert.equal(
        service.service.providerRegistry.hubForOperation(earlier),
        undefined,
        `a retired Actor operation must release its transport hub: ${earlier}`,
      );
    }
  });

  test('a record retired while a pull is in flight still ends the stream with its outcome', async () => {
    // The mirror's Actor arm is aged out from the authority's own truth on every
    // admission (`ageSettledActorOperations` -> `retireActorOperation` ->
    // `disposeOperation`). Retiring a record WHILE a stream's pull is in flight
    // used to remove the record and close its hub under that reader: the fetched
    // terminal was dropped (`ingestEvents` finds no record), the loop then read a
    // closed hub, and the stream ended with neither a terminal frame nor a resync
    // gap — the client could not tell a finished run from a truncated one.
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const sessionId = session.payload.session_id;
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'retired-mid-pull' },
    });
    assert.equal(prompt.status, 200, prompt.text);
    const operationId = prompt.payload.operation_id;
    const outcome = await waitForCharacterOutcome(operationId);
    assert.equal(outcome.run_status, 'succeeded', 'the authority settled the run and retains its observation');

    // Deterministic interleave: hold this stream's first Actor pull open, retire
    // the mirror record exactly while that pull is in flight, then let the pull
    // return the retained terminal. No timing luck: the retirement provably
    // happens between the pull request and the batch the loop ingests.
    const core = service.service.core;
    const realNextHostEvents = core.nextHostEvents.bind(core);
    let pullStarted;
    const started = new Promise((resolve) => { pullStarted = resolve; });
    let releasePull;
    const gate = new Promise((resolve) => { releasePull = resolve; });
    let gated = false;
    core.nextHostEvents = async (...args) => {
      if (!gated) {
        gated = true;
        pullStarted();
        await gate;
      }
      return realNextHostEvents(...args);
    };
    try {
      const streaming = sseBody(sessionId, operationId);
      await started;
      assert.equal(
        service.service.providerRegistry.operationRecord(operationId)?.actorBacked,
        true,
        'precondition: the stream is reading the Actor record the retirement targets',
      );
      service.service.providerRegistry.retireActorOperation(operationId);
      releasePull();
      const stream = await streaming;
      assert.equal(stream.status, 200);
      const endings = stream.frames.filter(
        (frame) =>
          frame.event === 'gap' ||
          (frame.event === 'provider_event' &&
            (frame.data?.OpFinished !== undefined ||
              frame.data?.OpFailed !== undefined ||
              frame.data?.SessionStopped !== undefined)),
      );
      assert.equal(
        endings.length,
        1,
        `a retirement that raced an in-flight pull must end the stream with the terminal or a resync gap, never a bare close: ${JSON.stringify(stream.frames)}`,
      );
      assert.equal(endings[0].event, 'provider_event', 'the fetched outcome is delivered, not replaced by a gap');
      assert.equal(endings[0].data.OpFinished.reason, 'end_turn');
      assert.equal(stream.frames.at(-1), endings[0], 'nothing may follow the stream ending');

      // The deferral is a handshake, not a leak: the record outlives only its
      // reader, and the retirement lands as soon as that reader is gone.
      const deadline = Date.now() + 5_000;
      while (service.service.providerRegistry.operationRecord(operationId) !== undefined) {
        assert.ok(Date.now() < deadline, 'the deferred retirement must be applied when the stream detaches');
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      assert.equal(
        service.service.providerRegistry.hubForOperation(operationId),
        undefined,
        'the retired operation must release its hub once its reader is gone',
      );
    } finally {
      core.nextHostEvents = realNextHostEvents;
    }
  });

  test('a retirement deferred by a stream lands when its last reader leaves', async () => {
    const { ProviderRegistry } = await import(join(serviceRoot, 'dist/provider-registry.js'));
    const { OperationEventHub } = await import(join(serviceRoot, 'dist/sse.js'));
    const registry = new ProviderRegistry();
    const sessionId = randomUUID();
    const operationId = randomUUID();
    registry.registerOperation({
      operationId,
      sessionId,
      providerId: MAIN_PROVIDER,
      status: 'started',
      terminalEvent: null,
      terminalTranscript: null,
      actorBacked: true,
    });
    registry.ensureHub(operationId, () => new OperationEventHub(operationId, sessionId));

    // Two readers, one retirement: neither the retirement nor the first detach
    // may take the record away from a stream that is still reading it.
    registry.attachOperationStream(operationId);
    registry.attachOperationStream(operationId);
    registry.retireActorOperation(operationId);
    assert.ok(registry.operationRecord(operationId), 'a retirement must never land under a live reader');
    registry.detachOperationStream(operationId);
    assert.ok(registry.operationRecord(operationId), 'the first detach must not drop what the second reader reads');
    registry.detachOperationStream(operationId);
    assert.equal(registry.operationRecord(operationId), undefined, 'the last detach applies the deferred retirement');
    assert.equal(registry.hubForOperation(operationId), undefined, 'and releases the hub with it');
  });

  test('session create stays API-key and Origin guarded, with zero launch', async () => {
    // One process holds one core Host authority, so the journey service must be
    // down before the keyed service opens; it is restored either way.
    await service.close();
    service = undefined;
    const denied = seedJourneyHome('denied');
    const previous = process.env.NEXUS42_DAEMON_API_KEY;
    process.env.NEXUS42_DAEMON_API_KEY = 'actor-journey-secret';
    let keyed;
    try {
      keyed = await startJourneyService(denied.home);
      const launched = () => readLog(denied.acpLog).filter((entry) => entry.event === 'session_new').length;
      const launchesBefore = launched();
      const actorDenied = {
        provider_id: MAIN_PROVIDER,
        cwd: denied.creativeRoot,
        actor_ref: { actor_kind: 'character', character_id: `chr_${'0'.repeat(32)}` },
        viewpoint: { world_id: JOURNEY_WORLD, binding_id: `awb_${'0'.repeat(32)}` },
      };
      const noKey = await fetch(`${keyed.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(actorDenied),
      });
      assert.equal(noKey.status, 401);
      const wrongKey = await fetch(`${keyed.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Key': 'wrong' },
        body: JSON.stringify(actorDenied),
      });
      assert.equal(wrongKey.status, 401);
      const wrongOrigin = await fetch(`${keyed.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Key': 'actor-journey-secret', Origin: 'http://evil.example.com:9999' },
        body: JSON.stringify(actorDenied),
      });
      assert.equal(wrongOrigin.status, 403);
      assert.equal(launched(), launchesBefore, 'a denied request must launch nothing');

      // The service is functional under the key: this launch proves the
      // measurement above is live, so the unchanged count was a real zero.
      const allowed = await fetch(`${keyed.url}/v1/daemon/agent-host/sessions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-API-Key': 'actor-journey-secret' },
        body: JSON.stringify({ provider_id: MAIN_PROVIDER, cwd: denied.creativeRoot }),
      });
      assert.equal(allowed.status, 200);
      assert.ok(launched() > launchesBefore, 'the authorized launch must reach the provider');
    } finally {
      if (keyed) await keyed.close();
      if (previous === undefined) delete process.env.NEXUS42_DAEMON_API_KEY;
      else process.env.NEXUS42_DAEMON_API_KEY = previous;
      service = await startJourneyService(home);
      url = service.url;
    }
  });

  test('a confirmed close and reopen replays no historical Actor session', async () => {
    // Establish one settled run on this home, then tear the service down.
    const session = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(session.status, 200, session.text);
    const sessionId = session.payload.session_id;
    const prompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'pre-restart' },
    });
    const operationId = prompt.payload.operation_id;
    assert.equal((await waitForCharacterOutcome(operationId)).run_status, 'succeeded');

    await service.close();
    service = await startJourneyService(home);
    url = service.url;
    const status = await jsonFetch('/v1/daemon/runtime/status');
    assert.equal(status.payload.runtime_mode, 'provider_enabled');

    // Process-lifetime Actor truth is gone: a missing detailed outcome is 404,
    // never a synthesized success and never a re-created session.
    const oldSession = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}`);
    assert.equal(oldSession.status, 404, oldSession.text);
    const oldOperation = await jsonFetch(`/v1/daemon/agent-host/operations/${operationId}`);
    assert.equal(oldOperation.status, 404, oldOperation.text);
    const oldPrompt = await jsonFetch(`/v1/daemon/agent-host/sessions/${sessionId}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'replay?' },
    });
    assert.equal(oldPrompt.status, 404, oldPrompt.text);
    const oldEvents = await fetch(`${url}/v1/daemon/agent-host/sessions/${sessionId}/events?operation_id=${operationId}`, {
      headers: { Accept: 'text/event-stream' },
    });
    assert.equal(oldEvents.status, 404);
    await oldEvents.body?.cancel();

    // A new create with the same pair is a fresh session, not the old id.
    const fresh = await jsonFetch('/v1/daemon/agent-host/sessions', { method: 'POST', body: actorBody() });
    assert.equal(fresh.status, 200, fresh.text);
    assert.notEqual(fresh.payload.session_id, sessionId);
    const run = await jsonFetch(`/v1/daemon/agent-host/sessions/${fresh.payload.session_id}/operations`, {
      method: 'POST',
      body: { kind: 'prompt', content: 'post-restart' },
    });
    assert.equal(run.status, 200, run.text);
    assert.equal((await waitForCharacterOutcome(run.payload.operation_id)).run_status, 'succeeded');
  });
});
