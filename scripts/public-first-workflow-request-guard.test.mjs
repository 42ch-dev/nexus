#!/usr/bin/env node
/**
 * Focused contract tests for the P3-T3 upstream request-budget guard (P3-T3).
 *
 * Scope: the guard's own observable contracts, exercised by real Node child
 * processes that load it exactly the way the driver does
 * (`NODE_OPTIONS=--import=<absolute guard path>`) against owned loopback
 * endpoints and owned temporary attempt directories. Nothing here needs a
 * prepared service/native artifact, a real `dsh`, an upstream model, a
 * credential or the product database — the composed journey belongs to the
 * driver run, not to this file. No case touches an external network.
 *
 *   node --test scripts/public-first-workflow-request-guard.test.mjs
 *
 * The guard is a preload and exports nothing, so every case drives it the only
 * way it is meant to run: as a preload in a child process, observed through
 * (a) what the loopback endpoint actually received, (b) a spy preload that
 * records what the guard forwarded to the original transport, and (c) the
 * evidence files the guard wrote. Each case fails on a plausible regression of
 * the contract it names:
 *   * a second request (same process, concurrent, racing child, or a whole
 *     second launch) reaching the transport — the `wx` slot is the only thing
 *     standing between one prompt and an unbounded call count;
 *   * a 302 being followed instead of refused, which would silently turn one
 *     admitted request into a second one;
 *   * a 429 or a reset being retried, or a failed attempt freeing the slot;
 *   * `/models`-style probes, a wrong method, or the same path on another
 *     origin being admitted before the network;
 *   * a malformed/unusable guard environment being treated as "no guard" —
 *     including a guard that recreates the evidence directory it was denied;
 *   * a non-dsh runtime or a missing `fetch` being treated as an exemption,
 *     including a grandchild of the installed runtime doing the fetch (which
 *     must be visible as a denial, not invisible);
 *   * *evidence* that could not be persisted passing as a clean run: an
 *     unrecordable denial must leave the attempt disqualified rather than
 *     reading as one admitted / zero denied, an unrecordable handshake must
 *     block every dispatch, and when neither an event nor the marker can be
 *     written the child must die instead of returning a catchable rejection;
 *   * the ceiling being installed where it cannot be locked (a pre-existing
 *     non-configurable `fetch`), which would leave a replaceable wrapper that a
 *     later assignment can bypass — the guard must refuse to run at all;
 *   * the ceiling being replaceable by a later `fetch = …` assignment;
 *   * a secret, a URL or a header leaking into the guard's own evidence.
 */
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { after, test } from 'node:test';
import { fileURLToPath } from 'node:url';

/** Absolute path of the guard under test. */
const GUARD_PATH = realpathSync(
  fileURLToPath(new URL('./public-first-workflow-request-guard.mjs', import.meta.url)),
);
/** The one model path the guard admits (contract §6.3 item 2). */
const MODEL_TARGET = '/chat/completions';
/** Evidence contract identifiers this file asserts literally. */
const EVENT_SCHEMA = 'nexus-request-guard-event/1';
const SPENT_SCHEMA = 'nexus-request-guard-spent/1';
const MARKER_SCHEMA = 'nexus-request-guard-evidence-failed/1';
const ADMITTED_CATEGORY = 'model_request_admitted';
const LOADED_CATEGORY = 'guard_loaded';
const DENIED_CATEGORIES = new Set([
  'malformed_env',
  'attempt_dir_missing',
  'attempt_dir_insecure',
  'unsupported_transport',
  'foreign_runtime',
  'package_child_runtime',
  'unclassifiable_request',
  'unexpected_url',
  'unexpected_method',
  'attempt_already_spent',
  'filesystem_error',
  'evidence_write_failed',
]);
/** Per-child bound; every case is a local spawn plus one loopback round trip. */
const CHILD_TIMEOUT_MS = 30_000;

// ---------------------------------------------------------------------------
// Owned fixture sources
// ---------------------------------------------------------------------------

/**
 * The dsh stand-in: performs the guarded call(s) the spec names and reports
 * what happened, including the identity of what the guard forwarded. It never
 * prints anything but its single JSON result line.
 */
const ENTRY_SOURCE = `import { chmodSync } from 'node:fs';
import { join } from 'node:path';

const spec = JSON.parse(process.env.GUARD_TEST_SPEC ?? '{}');
const probeKey = '__nexusGuardProbe';
const records = () => {
  const probe = globalThis[probeKey];
  return probe && Array.isArray(probe.records) ? probe.records : [];
};
const attempts = [];

async function call(label, url, init) {
  const outcome = { label };
  try {
    const response = await fetch(url, init);
    outcome.status = response.status;
    outcome.redirected = response.redirected;
    outcome.text = await response.text();
  } catch (error) {
    outcome.error = {
      name: error?.name ?? null,
      code: error?.code ?? null,
      message: String(error?.message ?? error),
    };
  }
  attempts.push(outcome);
  return outcome;
}

const headers = { ...(spec.headers ?? {}) };
const body = spec.body;
const controller = new AbortController();
const init = { method: spec.method ?? 'POST', headers, body, signal: controller.signal };

if (spec.mode === 'single' || spec.mode === 'sequence') {
  await call('first', spec.url, init);
  if (spec.mode === 'sequence') await call('second', spec.url, init);
} else if (spec.mode === 'concurrent') {
  await Promise.all([call('a', spec.url, init), call('b', spec.url, init)]);
} else if (spec.mode === 'method') {
  await call('method', spec.url, { method: spec.method ?? 'GET', headers });
} else if (spec.mode === 'url') {
  await call('foreign', spec.otherUrl, init);
} else if (spec.mode === 'sequence-chmod-events') {
  await call('first', spec.url, init);
  chmodSync(join(process.env.NEXUS_WORKFLOW_ATTEMPT_DIR, 'events'), 0o500);
  await call('second', spec.url, init);
} else if (spec.mode === 'chmod-events-then-fetch') {
  chmodSync(join(process.env.NEXUS_WORKFLOW_ATTEMPT_DIR, 'events'), 0o500);
  await call('blocked', spec.url, init);
} else if (spec.mode === 'chmod-attempt-then-fetch') {
  const attemptDir = process.env.NEXUS_WORKFLOW_ATTEMPT_DIR;
  chmodSync(join(attemptDir, 'events'), 0o500);
  chmodSync(attemptDir, 0o500);
  await call('locked', spec.url, init);
} else if (spec.mode === 'tamper') {
  let assignThrew = null;
  try {
    globalThis.fetch = () => 'tampered';
  } catch (error) {
    assignThrew = String(error?.message ?? error);
  }
  let defineThrew = null;
  try {
    Object.defineProperty(globalThis, 'fetch', { value: () => 'tampered' });
  } catch (error) {
    defineThrew = String(error?.message ?? error);
  }
  let deleteThrew = null;
  try {
    delete globalThis.fetch;
  } catch (error) {
    deleteThrew = String(error?.message ?? error);
  }
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'fetch');
  await call('after-tamper', spec.otherUrl, init);
  attempts.push({
    label: 'tamper',
    assignThrew,
    defineThrew,
    deleteThrew,
    configurable: descriptor?.configurable ?? null,
    stillFunction: typeof globalThis.fetch === 'function',
  });
} else {
  throw new Error('unknown GUARD_TEST_SPEC mode: ' + spec.mode);
}

const first = records()[0] ?? null;
process.stdout.write(
  JSON.stringify({
    attempts,
    transport: {
      dispatches: records().length,
      first:
        first === null
          ? null
          : {
              urlSameValue: first.input === spec.url,
              initIsForwardedCopy: first.init !== init,
              headersSameRef: first.init?.headers === headers,
              bodySameRef: first.init?.body === body,
              signalSameRef: first.init?.signal === controller.signal,
              redirectValue: first.init?.redirect ?? null,
              methodValue: first.init?.method ?? null,
            },
    },
  }) + '\\n',
);
`;

/**
 * Spy preload: loaded BEFORE the guard, so it becomes the guard's "original
 * fetch" and records exactly what the guard forwarded — the only way to prove
 * the headers/body/signal objects reached the transport by reference.
 */
const SPY_SOURCE = `const realFetch = globalThis.fetch;
globalThis.__nexusGuardProbe = { records: [] };
globalThis.fetch = (input, init) => {
  globalThis.__nexusGuardProbe.records.push({ input, init, at: Date.now() });
  return realFetch(input, init);
};
`;

/** Preload that removes global fetch before the guard loads (transport drift). */
const STRIP_SOURCE = `delete globalThis.fetch;
globalThis.__nexusFetchStripped = typeof globalThis.fetch;
`;

/** Preload that makes the event channel unwritable before the guard loads. */
const LOCK_EVENTS_SOURCE = `import { chmodSync } from 'node:fs';
import { join } from 'node:path';
chmodSync(join(process.env.NEXUS_WORKFLOW_ATTEMPT_DIR, 'events'), 0o500);
`;

/** Preloads that pre-empt the guard's own install, at both permission levels. */
const LOCK_FETCH_SOURCE = `Object.defineProperty(globalThis, 'fetch', {
  value: globalThis.fetch,
  writable: true,
  configurable: false,
});
`;
const FREEZE_FETCH_SOURCE = `Object.defineProperty(globalThis, 'fetch', {
  value: globalThis.fetch,
  writable: false,
  configurable: false,
});
`;

/** Owned fixture root; holds the stand-in entries, never the repo. */
const FIXTURE_ROOT = realpathSync(mkdtempSync(join(tmpdir(), 'nexus-guard-fixture-')));
const ENTRY_PATH = join(FIXTURE_ROOT, 'entry.mjs');
const DOMAIN_ENTRY_PATH = join(FIXTURE_ROOT, 'foreign-entry.mjs');
const LINKED_ENTRY_PATH = join(FIXTURE_ROOT, 'linked-entry.mjs');
const SPY_PATH = join(FIXTURE_ROOT, 'spy.mjs');
const STRIP_PATH = join(FIXTURE_ROOT, 'strip-fetch.mjs');
/**
 * The launch shape the driver really uses: an executable entry with a node
 * shebang, exec'd directly rather than through `node <entry>`.
 */
const SHEBANG_ENTRY_PATH = join(FIXTURE_ROOT, 'shebang-entry.mjs');
const LOCK_FETCH_PATH = join(FIXTURE_ROOT, 'lock-fetch.mjs');
const FREEZE_FETCH_PATH = join(FIXTURE_ROOT, 'freeze-fetch.mjs');
const LOCK_EVENTS_PATH = join(FIXTURE_ROOT, 'lock-events.mjs');
/**
 * A stand-in installed runtime tree: a package root with the recorded entry and
 * a second entry inside it, so the in-package drift label can be exercised
 * without reading the operator's real installation.
 */
const FAKE_PACKAGE_ROOT = join(FIXTURE_ROOT, 'installed-dsh-package');
const PACKAGE_ENTRY_PATH = join(FAKE_PACKAGE_ROOT, 'lib', 'bin.mjs');
const PACKAGE_CHILD_PATH = join(FAKE_PACKAGE_ROOT, 'lib', 'spawned-child.mjs');
mkdirSync(join(FAKE_PACKAGE_ROOT, 'lib'), { recursive: true });
writeFileSync(
  join(FAKE_PACKAGE_ROOT, 'package.json'),
  JSON.stringify({ name: '@deepseek-ai/dsh', version: '0.1.5-rc.1' }),
);
writeFileSync(ENTRY_PATH, ENTRY_SOURCE);
writeFileSync(DOMAIN_ENTRY_PATH, ENTRY_SOURCE);
writeFileSync(SHEBANG_ENTRY_PATH, `#!/usr/bin/env node\n${ENTRY_SOURCE}`, { mode: 0o755 });
writeFileSync(PACKAGE_ENTRY_PATH, ENTRY_SOURCE);
writeFileSync(PACKAGE_CHILD_PATH, ENTRY_SOURCE);
writeFileSync(SPY_PATH, SPY_SOURCE);
writeFileSync(STRIP_PATH, STRIP_SOURCE);
writeFileSync(LOCK_FETCH_PATH, LOCK_FETCH_SOURCE);
writeFileSync(FREEZE_FETCH_PATH, FREEZE_FETCH_SOURCE);
writeFileSync(LOCK_EVENTS_PATH, LOCK_EVENTS_SOURCE);
symlinkSync(ENTRY_PATH, LINKED_ENTRY_PATH);

/** Attempt directories and the fixture root created by this file, cleaned once. */
const ownedAttemptDirs = [];
after(() => {
  for (const dir of ownedAttemptDirs) {
    try {
      const eventsDir = join(dir, 'events');
      if (existsSync(eventsDir)) chmodSync(eventsDir, 0o700);
      chmodSync(dir, 0o700);
    } catch {
      // already removable
    }
    rmSync(dir, { recursive: true, force: true });
  }
  rmSync(FIXTURE_ROOT, { recursive: true, force: true });
});

// ---------------------------------------------------------------------------
// Owned loopback endpoint
// ---------------------------------------------------------------------------

/**
 * One owned loopback endpoint. `mode` selects the transport behaviour (not a
 * data fixture): `ok` answers 200 JSON, `redirect302` answers a same-origin
 * redirect, `status429` answers a rate-limit response, `reset` destroys the
 * socket once the request arrived. Every request that reaches it is recorded,
 * so a denied dispatch is observable as "the endpoint saw nothing".
 */
function startEndpoint(mode) {
  const requests = [];
  const server = createServer((request, response) => {
    const chunks = [];
    request.on('data', (chunk) => chunks.push(chunk));
    request.on('end', () => {
      requests.push({
        method: request.method,
        target: request.url,
        headers: request.headers,
        body: Buffer.concat(chunks).toString('utf8'),
      });
      if (mode === 'ok') {
        response.writeHead(200, { 'content-type': 'application/json' });
        response.end(
          JSON.stringify({
            id: 'cmpl-guard-test-1',
            choices: [{ index: 0, message: { role: 'assistant', content: 'READY' } }],
          }),
        );
        return;
      }
      if (mode === 'redirect302') {
        response.writeHead(302, { location: '/redirected-target' });
        response.end();
        return;
      }
      if (mode === 'status429') {
        response.writeHead(429, { 'content-type': 'application/json', 'retry-after': '1' });
        response.end(JSON.stringify({ error: { message: 'rate limited', type: 'rate_limit_error' } }));
        return;
      }
      if (mode === 'reset') {
        request.socket.destroy();
        return;
      }
      response.writeHead(500);
      response.end();
    });
  });
  return new Promise((resolvePromise) => {
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      resolvePromise({
        port,
        origin: `http://127.0.0.1:${port}`,
        url: `http://127.0.0.1:${port}${MODEL_TARGET}`,
        requests,
        close: () => new Promise((done) => server.close(done)),
      });
    });
  });
}

/** Run `body` against one owned endpoint and always release it. */
async function withEndpoint(mode, body) {
  const endpoint = await startEndpoint(mode);
  try {
    return await body(endpoint);
  } finally {
    await endpoint.close();
  }
}

// ---------------------------------------------------------------------------
// Child processes, attempt directories, evidence reads
// ---------------------------------------------------------------------------

/** One fresh owner-only attempt directory (mkdtemp creates mode 0700). */
function makeAttemptDir() {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'nexus-guard-attempt-')));
  ownedAttemptDirs.push(dir);
  return dir;
}

/**
 * Spawn one guarded child. The child environment is built from scratch, so the
 * test process's own environment can never leak into the case. Fields left
 * undefined are omitted, which is how the malformed-environment cases are made.
 */
function runGuardChild({
  attemptDir,
  allowedUrl,
  dshRealpath,
  spec,
  entry = ENTRY_PATH,
  direct = false,
  preloads = [SPY_PATH, GUARD_PATH],
  timeoutMs = CHILD_TIMEOUT_MS,
}) {
  const env = { PATH: [dirname(process.execPath), process.env.PATH ?? ''].join(':') };
  if (attemptDir !== undefined) env.NEXUS_WORKFLOW_ATTEMPT_DIR = attemptDir;
  if (allowedUrl !== undefined) env.NEXUS_WORKFLOW_ALLOWED_URL = allowedUrl;
  if (dshRealpath !== undefined) env.NEXUS_WORKFLOW_DSH_REALPATH = dshRealpath;
  if (spec !== undefined) env.GUARD_TEST_SPEC = JSON.stringify(spec);
  if (preloads.length > 0) env.NODE_OPTIONS = preloads.map((path) => `--import=${path}`).join(' ');
  const [command, args] = direct ? [entry, []] : [process.execPath, [entry]];
  return new Promise((resolvePromise) => {
    const child = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    const timer = setTimeout(() => child.kill('SIGKILL'), timeoutMs);
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      resolvePromise({ code, signal, stdout, stderr, result: parseHarnessOutput(stdout) });
    });
  });
}

/** Last non-empty stdout line of the stand-in, parsed. */
function parseHarnessOutput(stdout) {
  const lines = stdout
    .trim()
    .split('\n')
    .filter((line) => line.trim() !== '');
  if (lines.length === 0) return null;
  try {
    return JSON.parse(lines[lines.length - 1]);
  } catch {
    return null;
  }
}

/** A child that ran the stand-in to its end. */
function assertChildRan(child, label) {
  assert.equal(child.code, 0, `${label} exited ${child.code} (signal ${child.signal}): ${child.stderr}`);
  assert.notEqual(child.result, null, `${label} produced no harness result: ${child.stderr}`);
}

/** The single call outcome of a one-call child. */
function firstAttempt(child) {
  assert.ok(Array.isArray(child.result?.attempts) && child.result.attempts.length >= 1);
  return child.result.attempts[0];
}

/** Every event the guard wrote, parsed. Names are ordered for stable messages. */
function readEvents(attemptDir) {
  const eventsDir = join(attemptDir, 'events');
  if (!existsSync(eventsDir)) return [];
  return readdirSync(eventsDir)
    .sort()
    .map((name) => JSON.parse(readFileSync(join(eventsDir, name), 'utf8')));
}

/** Events grouped by kind. */
function summarize(events) {
  return {
    loaded: events.filter((event) => event.kind === 'loaded'),
    admitted: events.filter((event) => event.kind === 'admitted'),
    denied: events.filter((event) => event.kind === 'denied'),
  };
}

/** The terminal integrity marker, or null when the guard never wrote one. */
function readMarker(attemptDir) {
  const markerPath = join(attemptDir, 'evidence-failed');
  if (!existsSync(markerPath)) return null;
  return JSON.parse(readFileSync(markerPath, 'utf8'));
}

/**
 * Restore the permissions the evidence-fault cases removed, so cleanup can
 * remove the attempt directory and later reads work normally.
 */
function restoreAttemptDir(attemptDir) {
  const eventsDir = join(attemptDir, 'events');
  if (existsSync(eventsDir)) chmodSync(eventsDir, 0o700);
  chmodSync(attemptDir, 0o700);
}

/** Every file under one attempt directory (the slot plus the events). */
function listAttemptFiles(attemptDir) {
  const files = readdirSync(attemptDir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => join(attemptDir, entry.name));
  const eventsDir = join(attemptDir, 'events');
  if (existsSync(eventsDir)) {
    for (const name of readdirSync(eventsDir)) files.push(join(eventsDir, name));
  }
  return files;
}

/** A one-call spec against `url`. */
function singleSpec(url, overrides = {}) {
  return { mode: 'single', url, method: 'POST', body: '{"model":"deepseek-chat"}', ...overrides };
}

// ---------------------------------------------------------------------------
// The single admitted request
// ---------------------------------------------------------------------------

test('one dsh-runtime POST is admitted, forwarded opaque, and recorded', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const secretHeader = 'Bearer guard-test-secret-9f3a';
    const secretBody =
      '{"model":"deepseek-chat","messages":[{"role":"user","content":"guard-test-body-7c1d"}]}';
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: {
        mode: 'single',
        url: endpoint.url,
        method: 'POST',
        headers: {
          authorization: secretHeader,
          'x-test-marker': 'marker-1',
          'content-type': 'application/json',
        },
        body: secretBody,
      },
    });
    assertChildRan(child, 'single');

    // The guard wrote nothing to the child's own channels.
    assert.equal(child.stdout.trim().split('\n').length, 1, child.stdout);
    assert.equal(/nexus/i.test(child.stderr), false, child.stderr);

    const attempt = firstAttempt(child);
    assert.equal(attempt.status, 200);
    assert.equal(attempt.redirected, false);
    assert.match(attempt.text, /READY/);

    // What the endpoint actually received: exactly one request, unchanged.
    assert.equal(endpoint.requests.length, 1);
    assert.equal(endpoint.requests[0].method, 'POST');
    assert.equal(endpoint.requests[0].target, MODEL_TARGET);
    assert.equal(endpoint.requests[0].headers.authorization, secretHeader);
    assert.equal(endpoint.requests[0].headers['x-test-marker'], 'marker-1');
    assert.equal(endpoint.requests[0].body, secretBody);

    // What the guard forwarded to the original transport: the caller's own
    // headers/body/signal objects, with only `redirect` forced.
    const transport = child.result.transport;
    assert.equal(transport.dispatches, 1);
    assert.equal(transport.first.urlSameValue, true);
    assert.equal(transport.first.initIsForwardedCopy, true);
    assert.equal(transport.first.headersSameRef, true);
    assert.equal(transport.first.bodySameRef, true);
    assert.equal(transport.first.signalSameRef, true);
    assert.equal(transport.first.redirectValue, 'error');
    assert.equal(transport.first.methodValue, 'POST');

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(denied.length, 0);
    assert.equal(admitted.length, 1);
    assert.equal(loaded.length, 1);
    assert.equal(loaded[0].schema, EVENT_SCHEMA);
    assert.equal(loaded[0].runtime, 'dsh');
    assert.equal(loaded[0].category, LOADED_CATEGORY);
    assert.deepEqual(loaded[0].problems, []);
    assert.equal(admitted[0].runtime, 'dsh');
    assert.equal(admitted[0].category, ADMITTED_CATEGORY);
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
  });
});

// ---------------------------------------------------------------------------
// One slot per attempt: same process, concurrent, racing child, second launch
// ---------------------------------------------------------------------------

test('a second request in the same process is denied and never dispatched', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { ...singleSpec(endpoint.url), mode: 'sequence' },
    });
    assertChildRan(child, 'sequence');
    assert.equal(firstAttempt(child).status, 200);
    assert.equal(child.result.attempts[1].error?.code, 'attempt_already_spent');
    assert.equal(child.result.transport.dispatches, 1);
    assert.equal(endpoint.requests.length, 1);

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 1);
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].category, 'attempt_already_spent');
  });
});

test('two concurrent calls in one process admit exactly one', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { ...singleSpec(endpoint.url), mode: 'concurrent' },
    });
    assertChildRan(child, 'concurrent');
    const statuses = child.result.attempts.map((attempt) => attempt.status ?? null);
    assert.equal(statuses.filter((status) => status === 200).length, 1);
    assert.equal(
      child.result.attempts.filter((attempt) => attempt.error?.code === 'attempt_already_spent').length,
      1,
    );
    assert.equal(child.result.transport.dispatches, 1);
    assert.equal(endpoint.requests.length, 1);
    assert.equal(summarize(readEvents(attemptDir)).admitted.length, 1);
  });
});

test('two child processes racing on one attempt admit exactly one', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const options = {
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    };
    const [a, b] = await Promise.all([runGuardChild(options), runGuardChild(options)]);
    assertChildRan(a, 'racer-a');
    assertChildRan(b, 'racer-b');

    const outcomes = [a, b].map(firstAttempt);
    assert.equal(outcomes.filter((outcome) => outcome.status === 200).length, 1);
    assert.equal(
      outcomes.filter((outcome) => outcome.error?.code === 'attempt_already_spent').length,
      1,
    );
    assert.equal(endpoint.requests.length, 1);
    // The loser never dispatched at all.
    assert.deepEqual(
      [a.result.transport.dispatches, b.result.transport.dispatches].sort(),
      [0, 1],
    );

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 2);
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].category, 'attempt_already_spent');
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
  });
});

test('a second launch cannot reset a spent attempt', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const options = {
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    };
    const first = await runGuardChild(options);
    assertChildRan(first, 'launch-1');
    assert.equal(firstAttempt(first).status, 200);

    const spentPath = join(attemptDir, 'spent');
    const before = { content: readFileSync(spentPath, 'utf8'), ino: statSync(spentPath).ino };

    const second = await runGuardChild(options);
    assertChildRan(second, 'launch-2');
    assert.equal(firstAttempt(second).error?.code, 'attempt_already_spent');
    assert.equal(second.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 1);

    const after = { content: readFileSync(spentPath, 'utf8'), ino: statSync(spentPath).ino };
    assert.deepEqual(after, before, 'the slot was rewritten or replaced by the second launch');

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 2);
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 1);
  });
});

test('a fresh attempt directory is a fresh budget', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const options = (attemptDir) => ({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    const first = await runGuardChild(options(makeAttemptDir()));
    const second = await runGuardChild(options(makeAttemptDir()));
    assertChildRan(first, 'attempt-1');
    assertChildRan(second, 'attempt-2');
    assert.equal(firstAttempt(first).status, 200);
    assert.equal(firstAttempt(second).status, 200);
    assert.equal(first.result.transport.dispatches, 1);
    assert.equal(second.result.transport.dispatches, 1);
    assert.equal(endpoint.requests.length, 2);
  });
});

// ---------------------------------------------------------------------------
// Transport answers that must not become a second request
// ---------------------------------------------------------------------------

test('a 302 answer is not followed, so it cannot become a second request', async () => {
  await withEndpoint('redirect302', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(child, 'redirect');
    const attempt = firstAttempt(child);
    assert.equal(attempt.status, undefined);
    assert.equal(typeof attempt.error?.message, 'string');

    // One request, to the selected endpoint only; the redirect target was never
    // contacted, and the transport was dispatched exactly once.
    assert.deepEqual(
      endpoint.requests.map((request) => request.target),
      [MODEL_TARGET],
    );
    assert.equal(child.result.transport.dispatches, 1);
    assert.equal(child.result.transport.first.redirectValue, 'error');

    const { admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
  });
});

test('a 429 answer is reported as itself, is not retried, and keeps the slot spent', async () => {
  await withEndpoint('status429', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const options = {
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    };
    const child = await runGuardChild(options);
    assertChildRan(child, 'rate-limited');
    const attempt = firstAttempt(child);
    assert.equal(attempt.status, 429);
    assert.match(attempt.text, /rate limited/);
    assert.equal(child.result.transport.dispatches, 1);
    assert.equal(endpoint.requests.length, 1);

    const { admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 0);

    const second = await runGuardChild(options);
    assertChildRan(second, 'rate-limited-second');
    assert.equal(firstAttempt(second).error?.code, 'attempt_already_spent');
    assert.equal(endpoint.requests.length, 1);
  });
});

test('a connection reset spends the authorization and is never retried', async () => {
  await withEndpoint('reset', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const options = {
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    };
    const child = await runGuardChild(options);
    assertChildRan(child, 'reset');
    const attempt = firstAttempt(child);
    assert.equal(attempt.status, undefined);
    assert.equal(typeof attempt.error?.message, 'string');
    assert.equal(endpoint.requests.length, 1);
    assert.equal(child.result.transport.dispatches, 1);

    const { admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), true);

    const second = await runGuardChild(options);
    assertChildRan(second, 'reset-second');
    assert.equal(firstAttempt(second).error?.code, 'attempt_already_spent');
    assert.equal(endpoint.requests.length, 1);
  });
});

// ---------------------------------------------------------------------------
// Endpoint, method and runtime gates (all denied before the network)
// ---------------------------------------------------------------------------

test('an unexpected endpoint is denied before any network and spends nothing', async () => {
  await withEndpoint('ok', async (endpoint) => {
    await withEndpoint('ok', async (otherOrigin) => {
      // Same path, another loopback origin: the exact selected URL is the rule,
      // not the path or the host suffix.
      const attemptDir = makeAttemptDir();
      const child = await runGuardChild({
        attemptDir,
        allowedUrl: endpoint.url,
        dshRealpath: ENTRY_PATH,
        spec: singleSpec(otherOrigin.url),
      });
      assertChildRan(child, 'other-origin');
      assert.equal(firstAttempt(child).error?.code, 'unexpected_url');
      assert.equal(child.result.transport.dispatches, 0);
      assert.equal(endpoint.requests.length, 0);
      assert.equal(otherOrigin.requests.length, 0);
      const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
      assert.equal(loaded.length, 1);
      assert.equal(admitted.length, 0);
      assert.equal(denied.length, 1);
      assert.equal(denied[0].category, 'unexpected_url');
      assert.equal(denied[0].runtime, 'dsh');
      assert.equal(existsSync(join(attemptDir, 'spent')), false, 'a denial must not spend the slot');

      // A different path on the selected origin (the `/models`-style probe).
      const probeDir = makeAttemptDir();
      const probe = await runGuardChild({
        attemptDir: probeDir,
        allowedUrl: endpoint.url,
        dshRealpath: ENTRY_PATH,
        spec: { ...singleSpec(endpoint.url), mode: 'url', otherUrl: `${endpoint.origin}/models` },
      });
      assertChildRan(probe, 'models-probe');
      assert.equal(firstAttempt(probe).error?.code, 'unexpected_url');
      assert.equal(probe.result.transport.dispatches, 0);
      assert.equal(endpoint.requests.length, 0);
      assert.equal(existsSync(join(probeDir, 'spent')), false);
    });
  });
});

test('a non-POST method on the exact endpoint is denied', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { mode: 'method', url: endpoint.url, method: 'GET' },
    });
    assertChildRan(child, 'wrong-method');
    assert.equal(firstAttempt(child).error?.code, 'unexpected_method');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);
    const { admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(admitted.length, 0);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].category, 'unexpected_method');
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
  });
});

test('a foreign runtime is denied and never dispatched', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: DOMAIN_ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(child, 'foreign-runtime');
    assert.equal(firstAttempt(child).error?.code, 'foreign_runtime');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 1);
    assert.equal(loaded[0].runtime, 'other');
    assert.equal(admitted.length, 0);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].runtime, 'other');
    assert.equal(denied[0].category, 'foreign_runtime');
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
  });
});

test('a symlinked launch of the recorded entry is still the dsh runtime', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      entry: LINKED_ENTRY_PATH,
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(child, 'symlinked-entry');
    assert.equal(firstAttempt(child).status, 200);
    const { loaded, admitted } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 1);
    assert.equal(loaded[0].runtime, 'dsh');
    assert.equal(admitted.length, 1);
  });
});

test('the real launch shape (executable entry, node shebang) is guarded the same way', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      entry: SHEBANG_ENTRY_PATH,
      direct: true,
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: SHEBANG_ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(child, 'shebang-entry');
    assert.equal(firstAttempt(child).status, 200);
    assert.equal(endpoint.requests.length, 1);
    assert.equal(child.result.transport.dispatches, 1);
    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 1);
    assert.equal(loaded[0].runtime, 'dsh');
    assert.equal(admitted.length, 1);
    assert.equal(denied.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
  });
});

test('a child process inside the installed package tree is denied as package_child_runtime', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      entry: PACKAGE_CHILD_PATH,
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: PACKAGE_ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(child, 'package-child');
    assert.equal(firstAttempt(child).error?.code, 'package_child_runtime');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(loaded.length, 1);
    assert.equal(loaded[0].runtime, 'other');
    assert.equal(admitted.length, 0);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].runtime, 'other');
    assert.equal(denied[0].category, 'package_child_runtime');
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
  });
});

test('the guard refuses to run when the ceiling cannot be locked', async () => {
  await withEndpoint('ok', async (endpoint) => {
    // A pre-existing non-configurable `fetch` (writable or not) cannot be turned
    // into the locked accessor. A replaceable wrapper would be a silent bypass —
    // a later assignment would drop the guard and the evidence produced before
    // the drop could still authorize the run — so the child fails closed.
    for (const [label, lockPreload] of [
      ['nonconfigurable-writable', LOCK_FETCH_PATH],
      ['frozen', FREEZE_FETCH_PATH],
    ]) {
      const attemptDir = makeAttemptDir();
      const child = await runGuardChild({
        attemptDir,
        allowedUrl: endpoint.url,
        dshRealpath: ENTRY_PATH,
        spec: singleSpec(endpoint.url),
        preloads: [SPY_PATH, lockPreload, GUARD_PATH],
      });
      assert.equal(child.result, null, `${label}: the guard let the child run`);
      assert.equal(child.code, 78, `${label}: exit ${child.code} (signal ${child.signal})`);
      assert.equal(endpoint.requests.length, 0, `${label}: something was dispatched`);

      const marker = readMarker(attemptDir);
      assert.notEqual(marker, null, `${label}: no integrity marker`);
      assert.equal(marker.schema, MARKER_SCHEMA, label);
      assert.equal(marker.reason, 'fetch_not_replaceable', label);
      assert.equal(marker.kind, null, label);

      const events = summarize(readEvents(attemptDir));
      assert.equal(events.admitted.length, 0, label);
      assert.equal(existsSync(join(attemptDir, 'spent')), false, label);
    }
  });
});

test('an unrecordable denial is terminal for the attempt, not a silent zero-denial pass', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { ...singleSpec(endpoint.url), mode: 'sequence-chmod-events' },
    });
    restoreAttemptDir(attemptDir);
    assertChildRan(child, 'unrecordable-denial');

    // The first request is admitted normally and the slot is consumed.
    assert.equal(firstAttempt(child).status, 200);
    assert.equal(endpoint.requests.length, 1);
    assert.equal(child.result.transport.dispatches, 1);
    // The second is denied before dispatch, exactly as it should be...
    assert.equal(child.result.attempts[1].error?.code, 'attempt_already_spent');

    // ...but that denial could not be persisted, so the counts alone would read
    // as one admitted / zero denied with a spent slot — a clean-looking receipt
    // for an attempt in which an unexpected call was in fact refused. The marker
    // is what disqualifies it.
    const events = summarize(readEvents(attemptDir));
    assert.equal(events.admitted.length, 1);
    assert.equal(events.denied.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
    const marker = readMarker(attemptDir);
    assert.notEqual(marker, null, 'a refused call left no trace at all');
    assert.equal(marker.schema, MARKER_SCHEMA);
    assert.equal(marker.reason, 'event_write_failed');
    assert.equal(marker.kind, 'denied');
    assert.equal(typeof marker.pid, 'number');
    assert.equal(typeof marker.at, 'string');
    // The marker is evidence readers parse: safe keys only, nothing derived
    // from the request.
    assert.deepEqual(Object.keys(marker).sort(), ['at', 'kind', 'pid', 'reason', 'schema']);
    const markerText = readFileSync(join(attemptDir, 'evidence-failed'), 'utf8');
    for (const needle of [endpoint.url, endpoint.origin, '127.0.0.1', MODEL_TARGET, 'authorization']) {
      assert.equal(markerText.includes(needle), false, `marker contains ${needle}`);
    }
  });
});

test('when neither an event nor the marker can be written the child is terminated', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { ...singleSpec(endpoint.url), mode: 'chmod-attempt-then-fetch' },
    });
    const marker = readMarker(attemptDir);
    restoreAttemptDir(attemptDir);

    // No marker channel either: the guard must not return a rejection the
    // runtime could catch and continue from, so the child dies.
    assert.equal(marker, null, 'a marker should have been impossible here');
    assert.equal(child.result, null, 'the guard let the child continue');
    assert.equal(child.code, 78, `exit ${child.code} (signal ${child.signal})`);
    assert.equal(endpoint.requests.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
  });
});

test('an unrecordable admission is never dispatched', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: { ...singleSpec(endpoint.url), mode: 'chmod-events-then-fetch' },
    });
    restoreAttemptDir(attemptDir);
    assertChildRan(child, 'unrecordable-admission');

    // The request was admissible, but its admission could not be persisted, so
    // it must not reach the transport at all.
    assert.equal(firstAttempt(child).error?.code, 'evidence_write_failed');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);

    const events = summarize(readEvents(attemptDir));
    assert.equal(events.loaded.length, 1);
    assert.equal(events.admitted.length, 0);
    // The slot was taken before the record attempt, and the attempt is
    // disqualified by the marker rather than by a missing event.
    assert.equal(existsSync(join(attemptDir, 'spent')), true);
    const marker = readMarker(attemptDir);
    assert.notEqual(marker, null, 'the unrecordable admission left no trace');
    assert.equal(marker.reason, 'event_write_failed');
    assert.equal(marker.kind, 'admitted');
  });
});

test('a loaded record that cannot be persisted taints the attempt and blocks dispatch', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    mkdirSync(join(attemptDir, 'events'), { mode: 0o700 });
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
      preloads: [SPY_PATH, LOCK_EVENTS_PATH, GUARD_PATH],
    });
    restoreAttemptDir(attemptDir);
    assertChildRan(child, 'locked-events');

    // The handshake could not be recorded, so nothing may be dispatched and the
    // attempt is disqualified by the marker rather than by a missing event.
    assert.equal(firstAttempt(child).error?.code, 'evidence_write_failed');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
    const events = summarize(readEvents(attemptDir));
    assert.equal(events.loaded.length, 0);
    assert.equal(events.admitted.length, 0);
    const marker = readMarker(attemptDir);
    assert.notEqual(marker, null, 'the unrecordable handshake left no trace');
    assert.equal(marker.reason, 'event_write_failed');
    assert.equal(marker.kind, 'loaded');
  });
});

test('a missing global fetch is an unsupported transport, not a bypass', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
      preloads: [STRIP_PATH, GUARD_PATH],
    });
    assertChildRan(child, 'no-fetch');
    assert.equal(firstAttempt(child).error?.code, 'unsupported_transport');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);

    const { loaded, admitted, denied } = summarize(readEvents(attemptDir));
    assert.equal(admitted.length, 0);
    assert.equal(denied.length, 1);
    assert.equal(denied[0].category, 'unsupported_transport');
    assert.equal(loaded.length, 1);
    assert.ok(loaded[0].problems.includes('unsupported_transport'), JSON.stringify(loaded[0].problems));
    assert.equal(existsSync(join(attemptDir, 'spent')), false);
  });
});

// ---------------------------------------------------------------------------
// Malformed environment and unusable evidence directory (fail closed)
// ---------------------------------------------------------------------------

test('an unusable guard environment fails closed without creating anything', async () => {
  await withEndpoint('ok', async (endpoint) => {
    // (a) no selected URL at all.
    const missingUrlDir = makeAttemptDir();
    const missingUrl = await runGuardChild({
      attemptDir: missingUrlDir,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(missingUrl, 'missing-allowed-url');
    assert.equal(firstAttempt(missingUrl).error?.code, 'malformed_env');
    const missingUrlEvents = summarize(readEvents(missingUrlDir));
    assert.equal(missingUrlEvents.admitted.length, 0);
    assert.equal(missingUrlEvents.denied.length, 1);
    assert.equal(missingUrlEvents.denied[0].category, 'malformed_env');
    assert.ok(
      missingUrlEvents.loaded[0].problems.includes('env_missing:allowedUrl'),
      JSON.stringify(missingUrlEvents.loaded[0].problems),
    );
    assert.equal(existsSync(join(missingUrlDir, 'spent')), false);

    // (b) the selected URL is not the model endpoint: the configuration gate
    // precedes the request gate, so the child cannot post its way in.
    const wrongPathDir = makeAttemptDir();
    const wrongPathUrl = `${endpoint.origin}/models`;
    const wrongPath = await runGuardChild({
      attemptDir: wrongPathDir,
      allowedUrl: wrongPathUrl,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(wrongPathUrl),
    });
    assertChildRan(wrongPath, 'allowed-url-not-model-endpoint');
    assert.equal(firstAttempt(wrongPath).error?.code, 'malformed_env');
    assert.ok(
      summarize(readEvents(wrongPathDir)).loaded[0].problems.includes('allowed_url_not_model_endpoint'),
    );

    // (c) a query string is not the exact selected URL.
    const queryDir = makeAttemptDir();
    const query = await runGuardChild({
      attemptDir: queryDir,
      allowedUrl: `${endpoint.url}?stream=true`,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(query, 'allowed-url-with-query');
    assert.equal(firstAttempt(query).error?.code, 'malformed_env');
    assert.ok(
      summarize(readEvents(queryDir)).loaded[0].problems.includes('allowed_url_has_query_or_fragment'),
    );

    // (d) a cleartext non-loopback origin is not the deterministic loopback
    // origin and is not HTTPS either: a configuration refusal, so a mis-set
    // selected URL can never become cleartext egress.
    const insecureTransportDir = makeAttemptDir();
    // A cleartext origin that is not the loopback origin. `0.0.0.0` keeps the
    // case hermetic: even a guard that wrongly admitted it could only ever dial
    // the local host, and the connection is refused instantly.
    const cleartextUrl = 'http://0.0.0.0:1/chat/completions';
    const insecureTransport = await runGuardChild({
      attemptDir: insecureTransportDir,
      allowedUrl: cleartextUrl,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(cleartextUrl),
    });
    assertChildRan(insecureTransport, 'cleartext-selected-url');
    assert.equal(firstAttempt(insecureTransport).error?.code, 'malformed_env');
    assert.ok(
      summarize(readEvents(insecureTransportDir)).loaded[0].problems.includes(
        'allowed_url_insecure_transport',
      ),
    );
    assert.equal(existsSync(join(insecureTransportDir, 'spent')), false);

    // (e) the attempt directory does not exist: denied, and NOT created — the
    // guard must never be able to recreate the evidence directory.
    const absentDir = join(makeAttemptDir(), 'never-created');
    const absent = await runGuardChild({
      attemptDir: absentDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(absent, 'missing-attempt-dir');
    assert.equal(firstAttempt(absent).error?.code, 'attempt_dir_missing');
    assert.equal(existsSync(absentDir), false, 'the guard created the attempt directory');

    // (f) group/other permission bits on an existing attempt directory.
    const insecureDir = makeAttemptDir();
    chmodSync(insecureDir, 0o755);
    const insecure = await runGuardChild({
      attemptDir: insecureDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(insecure, 'insecure-attempt-dir');
    assert.equal(firstAttempt(insecure).error?.code, 'attempt_dir_insecure');
    assert.equal(existsSync(join(insecureDir, 'spent')), false);

    // (g) a relative attempt directory is not a directory this guard can trust.
    const relative = await runGuardChild({
      attemptDir: 'nexus-relative-attempt',
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: singleSpec(endpoint.url),
    });
    assertChildRan(relative, 'relative-attempt-dir');
    assert.equal(firstAttempt(relative).error?.code, 'attempt_dir_missing');

    // Nothing reached the endpoint in any of the above.
    assert.equal(endpoint.requests.length, 0);
  });
});

// ---------------------------------------------------------------------------
// The ceiling itself
// ---------------------------------------------------------------------------

test('the ceiling cannot be replaced and keeps denying after a tamper attempt', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: {
        mode: 'tamper',
        url: endpoint.url,
        otherUrl: `${endpoint.origin}/models`,
        method: 'POST',
        body: '{"model":"deepseek-chat"}',
      },
    });
    assertChildRan(child, 'tamper');
    const tamper = child.result.attempts.find((attempt) => attempt.label === 'tamper');
    assert.notEqual(tamper, undefined);
    assert.equal(tamper.assignThrew, null, 'a polyfill-style assignment must not crash the process');
    assert.equal(typeof tamper.defineThrew, 'string');
    assert.equal(typeof tamper.deleteThrew, 'string');
    assert.equal(tamper.configurable, false);
    assert.equal(tamper.stillFunction, true);

    // Behaviourally: the fetch that follows the tamper attempt is still the
    // guard's, and it still denies.
    const after = child.result.attempts.find((attempt) => attempt.label === 'after-tamper');
    assert.equal(after.error?.code, 'unexpected_url');
    assert.equal(child.result.transport.dispatches, 0);
    assert.equal(endpoint.requests.length, 0);
  });
});

// ---------------------------------------------------------------------------
// Evidence safety
// ---------------------------------------------------------------------------

test('guard evidence carries no secret, no URL and only the contract fields', async () => {
  await withEndpoint('ok', async (endpoint) => {
    const attemptDir = makeAttemptDir();
    const secretHeader = 'Bearer guard-test-secret-2b7f';
    const secretBody =
      '{"model":"deepseek-chat","messages":[{"role":"user","content":"guard-test-body-5e91"}]}';
    const child = await runGuardChild({
      attemptDir,
      allowedUrl: endpoint.url,
      dshRealpath: ENTRY_PATH,
      spec: {
        ...singleSpec(endpoint.url),
        mode: 'sequence',
        headers: { authorization: secretHeader, 'content-type': 'application/json' },
        body: secretBody,
      },
    });
    assertChildRan(child, 'evidence');

    // Positive control: the request really was dispatched with the secret —
    // the negative assertions below are then about the evidence, not about a
    // request that never happened.
    assert.equal(endpoint.requests.length, 1);
    assert.equal(endpoint.requests[0].headers.authorization, secretHeader);
    assert.equal(endpoint.requests[0].body, secretBody);

    const forbidden = [
      secretHeader,
      'guard-test-secret-2b7f',
      'guard-test-body-5e91',
      endpoint.url,
      endpoint.origin,
      '127.0.0.1',
      MODEL_TARGET,
      'authorization',
      'Bearer',
      'messages',
      'deepseek-chat',
    ];
    const files = listAttemptFiles(attemptDir);
    assert.ok(files.length >= 4, `expected slot + events, found ${files.join(', ')}`);
    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      for (const needle of forbidden) {
        assert.equal(text.includes(needle), false, `${file} contains ${needle}: ${text}`);
      }
    }

    // Event files: the declared name shape, the declared key set per kind, the
    // declared enums — nothing extra.
    const eventNames = readdirSync(join(attemptDir, 'events'));
    assert.ok(eventNames.length >= 3);
    for (const name of eventNames) {
      assert.match(name, /^(loaded|admitted|denied)-\d+-\d+-[0-9a-f]{8}\.json$/, name);
    }
    const events = readEvents(attemptDir);
    assert.equal(events.length, eventNames.length);
    for (const event of events) {
      assert.equal(event.schema, EVENT_SCHEMA);
      assert.ok(['loaded', 'admitted', 'denied'].includes(event.kind));
      assert.ok(['dsh', 'other'].includes(event.runtime));
      assert.equal(typeof event.at, 'string');
      const keys = Object.keys(event).sort();
      if (event.kind === 'loaded') {
        assert.deepEqual(keys, ['at', 'category', 'kind', 'problems', 'runtime', 'schema']);
      } else {
        assert.deepEqual(keys, ['at', 'category', 'kind', 'runtime', 'schema']);
      }
      if (event.kind === 'loaded') assert.equal(event.category, LOADED_CATEGORY);
      if (event.kind === 'admitted') assert.equal(event.category, ADMITTED_CATEGORY);
      if (event.kind === 'denied') assert.ok(DENIED_CATEGORIES.has(event.category));
    }

    // The consumed slot is secret-free as well.
    const spent = JSON.parse(readFileSync(join(attemptDir, 'spent'), 'utf8'));
    assert.deepEqual(Object.keys(spent).sort(), ['at', 'category', 'pid', 'schema']);
    assert.equal(spent.schema, SPENT_SCHEMA);
    assert.equal(spent.category, ADMITTED_CATEGORY);
  });
});
