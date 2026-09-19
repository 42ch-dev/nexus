#!/usr/bin/env node
/**
 * P0-T6 quit-gate tests.
 *
 * No GUI, no Electron, no E2E: the dialog adapter and the P0-T4 controller are
 * stubbed at their frozen contracts, and the detached handoff is exercised
 * against a tiny deterministic fixture service started through a stand-in for
 * the installed Node runtime (a `sh` wrapper that answers `--version` and then
 * `exec`s the real Node on the entry). The fixture child is a real separate
 * process, which is what makes the "survives the parent harness exit" claim
 * observable rather than asserted.
 */
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test, { after } from 'node:test';
import {
  DesktopQuitController,
  KEEP_INTERRUPTED_DETAIL,
  createDetachedServiceHandoff,
  findNodeOnPath,
  isCompatibleNodeVersion,
} from '../dist/quit-controller.js';

const QUIT_CONTROLLER_URL = new URL('../dist/quit-controller.js', import.meta.url).href;

const root = mkdtempSync(join(tmpdir(), 'nexus-quit-'));
const liveChildren = new Set();

after(() => {
  for (const pid of liveChildren) {
    try {
      process.kill(pid, 'SIGKILL');
    } catch {
      // already gone
    }
  }
  rmSync(root, { recursive: true, force: true });
});

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function fixtureDir() {
  return mkdtempSync(join(root, 'case-'));
}

function tempHome() {
  return mkdtempSync(join(root, 'home-'));
}

function writeFixture(path, content, mode = 0o644) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content, { mode });
  chmodSync(path, mode);
  return path;
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

/**
 * Stand-in for the installed standalone Node runtime: it answers the version
 * probe and then replaces itself with the real Node on whatever entry it is
 * handed (`exec` keeps the pid, so the child stays the one detached process).
 */
function fakeNodeScript(version, realNode) {
  const delegate = realNode === null ? '' : `\nexec ${shellQuote(realNode)} "$@"\n`;
  return `#!/bin/sh\nif [ "$1" = "--version" ]; then\n  echo ${shellQuote(version)}\n  exit 0\nfi\n${delegate}`;
}

const TINY_SERVICE_SOURCE = `import { appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const args = process.argv.slice(2);
const flagValue = (flag) => {
  const index = args.indexOf(flag);
  return index === -1 ? null : args[index + 1] ?? null;
};
const home = flagValue('--home');
writeFileSync(
  join(home, 'service-child.json'),
  JSON.stringify({
    pid: process.pid,
    ppid: process.ppid,
    argv: args,
    host: flagValue('--host'),
    port: Number(flagValue('--port')),
  }),
);
const beat = (line) => appendFileSync(join(home, 'heartbeat.log'), line + '\\n');
beat('0 ' + Date.now());
let seq = 0;
const timer = setInterval(() => {
  seq += 1;
  beat(seq + ' ' + Date.now());
}, 25);
const stop = () => {
  clearInterval(timer);
  beat('stopped ' + seq);
  process.exit(0);
};
process.on('SIGTERM', stop);
process.on('SIGINT', stop);
`;

/** Runs the real handoff in its own process, then exits while the child lives. */
function survivalParentSource({ serviceEntry, nodeExecutable, home, host, port }) {
  return `import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createDetachedServiceHandoff } from ${JSON.stringify(QUIT_CONTROLLER_URL)};

const home = ${JSON.stringify(home)};
const handoff = createDetachedServiceHandoff(
  ${JSON.stringify({ serviceEntry, nodeExecutable }, null, 2)},
);
await handoff.prepare();
await handoff.start(${JSON.stringify({ home, host, port })});
const record = join(home, 'service-child.json');
const deadline = Date.now() + 10_000;
while (!existsSync(record) && Date.now() < deadline) {
  await new Promise((resolve) => setTimeout(resolve, 20));
}
if (!existsSync(record)) {
  console.error('the detached child published no record');
  process.exit(1);
}
writeFileSync(join(home, 'parent-exit.json'), JSON.stringify({ parent_pid: process.pid, at: Date.now() }));
process.exit(0);
`;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitFor(predicate, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = predicate();
    if (value) return value;
    if (Date.now() >= deadline) throw new Error('timed out waiting for the fixture');
    await delay(10);
  }
}

function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

/** A detached child is its own process-group leader, so it survives group signals. */
function processGroupId(pid) {
  return Number.parseInt(
    execFileSync('ps', ['-o', 'pgid=', '-p', String(pid)], { encoding: 'utf8' }).trim(),
    10,
  );
}

function track(pid) {
  liveChildren.add(pid);
  return pid;
}

function heartbeat(home) {
  const lines = readFileSync(join(home, 'heartbeat.log'), 'utf8').trim().split('\n');
  const [seq, at] = lines[lines.length - 1].split(' ');
  return { seq: Number(seq), at: Number(at) };
}

async function stopChild(pid) {
  try {
    process.kill(pid, 'SIGTERM');
  } catch {
    // already gone
  }
  await waitFor(() => !processAlive(pid), 5_000);
  liveChildren.delete(pid);
  assert.equal(processAlive(pid), false, `fixture child ${pid} outlived SIGTERM`);
}

/**
 * The frozen quit surface. `stop()` is deliberately absent: an ordinary stop
 * leaves an attached independent service running (row 8), so the gate must use
 * the explicit authenticated stop.
 */
function stubController({ status = { state: 'running', port: 8420 }, onStopExplicit, onKeepForQuit } = {}) {
  const calls = { stopExplicit: 0, keepForQuit: 0 };
  return {
    calls,
    status,
    async stopExplicit() {
      calls.stopExplicit += 1;
      if (onStopExplicit) await onStopExplicit();
    },
    async keepForQuit() {
      calls.keepForQuit += 1;
      if (onKeepForQuit) await onKeepForQuit();
    },
    getStatus: () => ({ ...status }),
  };
}

function quitGate(controller, choose, report) {
  const outcomes = [];
  const dialogStatuses = [];
  const quit = new DesktopQuitController({
    controller,
    chooseQuitChoice: async (status) => {
      dialogStatuses.push(status);
      return choose(status);
    },
    reportQuitOutcome: report ?? ((outcome) => outcomes.push(outcome)),
  });
  return { quit, dialogStatuses, outcomes };
}

function interruptedError(message) {
  return Object.assign(new Error(message), { code: 'interrupted' });
}

function unavailableError(message) {
  return Object.assign(new Error(message), { code: 'unavailable' });
}

// ---------------------------------------------------------------------------
// Quit gate: the three frozen choices
// ---------------------------------------------------------------------------

test('stop choice closes through the explicit stop and allows the quit', async () => {
  const controller = stubController();
  const { quit, dialogStatuses, outcomes } = quitGate(controller, () => 'stop');

  assert.equal(await quit.requestQuit(), true);
  assert.equal(controller.calls.stopExplicit, 1);
  assert.equal(controller.calls.keepForQuit, 0);
  // The dialog sees the frozen status snapshot; a successful stop needs no notice.
  assert.deepEqual(dialogStatuses, [{ state: 'running', port: 8420 }]);
  assert.deepEqual(outcomes, []);
});

test('an unconfirmed close refuses the quit and surfaces the interrupted state', async () => {
  const controller = stubController({
    onStopExplicit: () =>
      Promise.reject(
        interruptedError(
          'service close reported interrupted cleanup (cleanup_confirmed:false); the retained owner blocks restart and quit',
        ),
      ),
  });
  const { quit, outcomes } = quitGate(controller, () => 'stop');

  assert.equal(await quit.requestQuit(), false);
  assert.equal(controller.calls.stopExplicit, 1);
  assert.equal(controller.calls.keepForQuit, 0);
  assert.equal(outcomes.length, 1);
  assert.equal(outcomes[0].allowed, false);
  assert.equal(outcomes[0].choice, 'stop');
  assert.match(outcomes[0].detail, /close was not confirmed, so the app stays open/);
  assert.match(outcomes[0].detail, /cleanup_confirmed:false/);
});

test('keep choice transfers the service and reports interrupted in-flight work', async () => {
  const controller = stubController();
  const { quit, outcomes } = quitGate(controller, () => 'keep');

  assert.equal(await quit.requestQuit(), true);
  assert.equal(controller.calls.keepForQuit, 1);
  assert.equal(controller.calls.stopExplicit, 0);
  assert.deepEqual(outcomes, [{ allowed: true, choice: 'keep', detail: KEEP_INTERRUPTED_DETAIL }]);
  assert.match(outcomes[0].detail, /interrupted/);
});

test('keep without an installed Node refuses the quit and leaves the owned service running', async () => {
  const controller = stubController({
    onKeepForQuit: () =>
      Promise.reject(
        unavailableError(
          'keeping the service requires a detached handoff (installed Node runtime and packaged service entry)',
        ),
      ),
  });
  const { quit, outcomes } = quitGate(controller, () => 'keep');

  assert.equal(await quit.requestQuit(), false);
  assert.equal(controller.calls.keepForQuit, 1);
  // Never a silent stop fallback: the utility owner is still alive.
  assert.equal(controller.calls.stopExplicit, 0);
  assert.equal(controller.getStatus().state, 'running');
  assert.equal(outcomes.length, 1);
  assert.equal(outcomes[0].allowed, false);
  assert.equal(outcomes[0].choice, 'keep');
  assert.match(outcomes[0].detail, /was not transferred to an independent process, so the app stays open/);
  assert.match(outcomes[0].detail, /detached handoff/);
});

test('cancel leaves the session intact and reports nothing', async () => {
  const controller = stubController();
  const { quit, outcomes } = quitGate(controller, () => 'cancel');

  assert.equal(await quit.requestQuit(), false);
  assert.deepEqual(controller.calls, { stopExplicit: 0, keepForQuit: 0 });
  assert.deepEqual(outcomes, []);
});

test('a dismissed dialog counts as cancel', async () => {
  const controller = stubController();
  const { quit, outcomes } = quitGate(controller, () => null);

  assert.equal(await quit.requestQuit(), false);
  assert.deepEqual(controller.calls, { stopExplicit: 0, keepForQuit: 0 });
  assert.deepEqual(outcomes, []);
});

test('a failing dialog refuses the quit instead of quitting silently', async () => {
  const controller = stubController();
  const { quit, outcomes } = quitGate(controller, () => {
    throw new Error('no window to attach the sheet to');
  });

  assert.equal(await quit.requestQuit(), false);
  assert.deepEqual(controller.calls, { stopExplicit: 0, keepForQuit: 0 });
  assert.equal(outcomes.length, 1);
  assert.equal(outcomes[0].allowed, false);
  assert.equal(outcomes[0].choice, null);
  assert.match(outcomes[0].detail, /quit dialog failed, so the app stays open/);
});

test('nothing to stop or keep quits without a dialog', async () => {
  const controller = stubController({ status: { state: 'stopped', port: 8420 } });
  const { quit, dialogStatuses, outcomes } = quitGate(controller, () => {
    throw new Error('the dialog must not be asked when nothing is running');
  });

  assert.equal(await quit.requestQuit(), true);
  assert.equal(dialogStatuses.length, 0);
  assert.deepEqual(controller.calls, { stopExplicit: 0, keepForQuit: 0 });
  assert.deepEqual(outcomes, []);
});

test('concurrent quit requests join one decision', async () => {
  const controller = stubController();
  let release;
  const gate = new Promise((resolve) => {
    release = resolve;
  });
  let asked = 0;
  const { quit } = quitGate(controller, async () => {
    asked += 1;
    await gate;
    return 'stop';
  });

  const first = quit.requestQuit();
  const second = quit.requestQuit();
  release();
  assert.deepEqual(await Promise.all([first, second]), [true, true]);
  assert.equal(asked, 1);
  assert.equal(controller.calls.stopExplicit, 1);
});

test('a cancelled quit can be asked again later', async () => {
  const controller = stubController();
  const choices = ['cancel', 'keep'];
  const { quit } = quitGate(controller, () => choices.shift());

  assert.equal(await quit.requestQuit(), false);
  assert.equal(await quit.requestQuit(), true);
  assert.equal(controller.calls.keepForQuit, 1);
});

test('a failing outcome reporter does not change the decision', async () => {
  const controller = stubController({
    onKeepForQuit: () => Promise.reject(unavailableError('no detached handoff')),
  });
  const { quit } = quitGate(controller, () => 'keep', () => {
    throw new Error('the notification surface is gone');
  });

  assert.equal(await quit.requestQuit(), false);
});

// ---------------------------------------------------------------------------
// Detached handoff (D-21 Keep transfer target)
// ---------------------------------------------------------------------------

test('isCompatibleNodeVersion accepts the frozen minimum and newer release lines', () => {
  assert.equal(isCompatibleNodeVersion('v22.22.0'), true);
  assert.equal(isCompatibleNodeVersion('22.22.0'), true);
  assert.equal(isCompatibleNodeVersion('v22.23.1'), true);
  assert.equal(isCompatibleNodeVersion('v23.0.0'), true);
  assert.equal(isCompatibleNodeVersion('v22.21.9'), false);
  assert.equal(isCompatibleNodeVersion('v20.11.0'), false);
  assert.equal(isCompatibleNodeVersion('v21.99.99'), false);
  assert.equal(isCompatibleNodeVersion('not-a-version'), false);
});

test('findNodeOnPath takes the first executable node and nothing else', () => {
  const dir = fixtureDir();
  const empty = fixtureDir();
  const good = writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', null), 0o755);
  const notExecutable = fixtureDir();
  writeFixture(join(notExecutable, 'node'), 'console.log("not executable")', 0o644);

  assert.equal(findNodeOnPath({ PATH: dir }), good);
  assert.equal(findNodeOnPath({ PATH: notExecutable }), null);
  assert.equal(findNodeOnPath({ PATH: empty }), null);
  assert.equal(findNodeOnPath({}), null);
});

test('prepare accepts a compatible standalone Node with a packaged entry', async () => {
  const dir = fixtureDir();
  const entry = writeFixture(join(dir, 'main.js'), '// packaged service entry');
  const node = writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', process.execPath), 0o755);

  await createDetachedServiceHandoff({ serviceEntry: entry, nodeExecutable: node }).prepare();
});

test('prepare refuses a Node runtime that is missing, unusable or too old', async () => {
  const dir = fixtureDir();
  const entry = writeFixture(join(dir, 'main.js'), '// packaged service entry');

  await assert.rejects(
    createDetachedServiceHandoff({ serviceEntry: entry, env: { PATH: fixtureDir() } }).prepare(),
    /no standalone Node runtime was found on PATH/,
  );
  await assert.rejects(
    createDetachedServiceHandoff({
      serviceEntry: entry,
      nodeExecutable: writeFixture(join(dir, 'noisy-node'), fakeNodeScript('not-a-version', null), 0o755),
    }).prepare(),
    /did not report a usable version/,
  );
  await assert.rejects(
    createDetachedServiceHandoff({
      serviceEntry: entry,
      nodeExecutable: writeFixture(join(dir, 'old-node'), fakeNodeScript('v20.11.0', null), 0o755),
    }).prepare(),
    /is older than the required 22\.22\.0/,
  );
  await assert.rejects(
    createDetachedServiceHandoff({
      serviceEntry: join(dir, 'missing-main.js'),
      nodeExecutable: writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', process.execPath), 0o755),
    }).prepare(),
    /packaged service entry is not a readable file/,
  );
});

test('start runs the packaged entry with the same home and port as a detached child', async () => {
  const dir = fixtureDir();
  const home = tempHome();
  const entry = writeFixture(join(dir, 'main.js'), TINY_SERVICE_SOURCE);
  const node = writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', process.execPath), 0o755);
  const handoff = createDetachedServiceHandoff({ serviceEntry: entry, nodeExecutable: node });

  await handoff.prepare();
  await handoff.start({ home, host: '127.0.0.1', port: 8420 });

  const recordFile = join(home, 'service-child.json');
  await waitFor(() => existsSync(recordFile));
  const record = JSON.parse(readFileSync(recordFile, 'utf8'));
  track(record.pid);
  assert.deepEqual(record.argv, ['--home', home, '--host', '127.0.0.1', '--port', '8420']);
  assert.equal(record.host, '127.0.0.1');
  assert.equal(record.port, 8420);
  assert.equal(processAlive(record.pid), true);

  await stopChild(record.pid);
});

test('start refuses when the handoff target disappeared after prepare', async () => {
  const dir = fixtureDir();
  const home = tempHome();
  const entry = writeFixture(join(dir, 'main.js'), TINY_SERVICE_SOURCE);
  const node = writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', process.execPath), 0o755);

  const missingEntry = createDetachedServiceHandoff({ serviceEntry: entry, nodeExecutable: node });
  await missingEntry.prepare();
  rmSync(entry);
  await assert.rejects(
    missingEntry.start({ home, host: '127.0.0.1', port: 8420 }),
    /packaged service entry is not a readable file/,
  );

  const missingNode = createDetachedServiceHandoff({ serviceEntry: entry, nodeExecutable: node });
  writeFixture(entry, TINY_SERVICE_SOURCE);
  await missingNode.prepare();
  rmSync(node);
  await assert.rejects(
    missingNode.start({ home, host: '127.0.0.1', port: 8420 }),
    /detached service child failed to start/,
  );
});

test('a ready detached child survives the parent harness exit', async () => {
  const dir = fixtureDir();
  const home = tempHome();
  const entry = writeFixture(join(dir, 'main.js'), TINY_SERVICE_SOURCE);
  const node = writeFixture(join(dir, 'node'), fakeNodeScript('v22.22.0', process.execPath), 0o755);
  const parent = writeFixture(
    join(dir, 'survival-parent.mjs'),
    survivalParentSource({ serviceEntry: entry, nodeExecutable: node, home, host: '127.0.0.1', port: 8420 }),
  );

  const run = spawnSync(process.execPath, [parent], { encoding: 'utf8', timeout: 30_000 });
  assert.equal(run.status, 0, `the parent harness failed: ${run.stderr}`);
  assert.equal(existsSync(join(home, 'parent-exit.json')), true);

  const exit = JSON.parse(readFileSync(join(home, 'parent-exit.json'), 'utf8'));
  const child = JSON.parse(readFileSync(join(home, 'service-child.json'), 'utf8'));
  track(child.pid);
  assert.deepEqual(child.argv, ['--home', home, '--host', '127.0.0.1', '--port', '8420']);
  // The spawning parent is already gone when the child is sampled.
  assert.equal(processAlive(exit.parent_pid), false);

  const sampled = heartbeat(home);
  await delay(300);
  const later = heartbeat(home);
  assert.ok(later.seq > sampled.seq, `the detached child stopped beating (${sampled.seq} → ${later.seq})`);
  assert.ok(later.at > exit.at, 'the detached child kept working after the parent harness exited');
  assert.equal(processAlive(child.pid), true);
  // `detached`, not merely "the parent exited politely": the child leads its own
  // process group, so the exiting session cannot take it down with a group signal.
  assert.equal(processGroupId(child.pid), child.pid);

  await stopChild(child.pid);
});
