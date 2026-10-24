#!/usr/bin/env node
/**
 * I-000062 — the CLI launcher (dist/main.js) prints the tokenized bootstrap URL
 * at boot, and auto-opens it only when this process serves the SPA.
 *
 * Boot-level: the service is spawned as a real process (never a browser). The
 * `open:` line is a human log on stderr; stdout keeps exactly one machine line.
 * The auto-open edge is observed through a PATH shim for `open`/`xdg-open` that
 * records its argv instead of launching anything.
 *
 * Cases:
 *  - `--web-dist <dir> --no-open` → the URL is printed, nothing is opened, and
 *    the printed token really is the store's (302 + `nexus_session` cookie).
 *  - `--web-dist <dir>` → the same URL is handed to the opener.
 *  - no `--web-dist` → still printed (the print is unconditional for HTTP; only
 *    the auto-open needs the SPA), never opened. This case guards the
 *    `args.webDist` conjunct of the auto-open condition.
 */
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test, { after, before } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const isWindows = process.platform === 'win32';
const OPEN_COMMAND = process.platform === 'darwin' ? 'open' : 'xdg-open';
const OPEN_LINE = /^\[nexus-service\] open: (\S+)$/m;
/** Absence window for the opener marker: the spawn-plus-shim write lands ~400 ms
 * after the `open:` print on this host, so sample for much longer than that. */
const ABSENCE_WINDOW_MS = 3_000;

const tempDirs = [];

after(() => {
  for (const dir of tempDirs) rmSync(dir, { recursive: true, force: true });
});

function tempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  tempDirs.push(dir);
  return dir;
}

before(() => {
  // The suite normally runs after `pnpm -F @42ch/nexus-service build`; build on
  // demand so the file also runs standalone.
  if (existsSync(join(serviceRoot, 'dist', 'main.js'))) return;
  const build = spawnSync('npx', ['tsc', '-p', 'tsconfig.json'], {
    cwd: serviceRoot,
    stdio: 'inherit',
  });
  assert.equal(build.status, 0, 'service build failed');
});

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

async function waitFor(produce, describe, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = produce();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(describe());
    await delay(25);
  }
}

/**
 * Absence proof for the opener marker. Polling the whole window (instead of one
 * fixed-delay sample) keeps a reverted build red even when the shim write lands
 * late under load; a fixed 500 ms sample let it pass ~50% of runs.
 */
async function assertOpenerNeverRan(marker, message) {
  const deadline = Date.now() + ABSENCE_WINDOW_MS;
  for (;;) {
    assert.equal(markerText(marker), '', `${message} (marker ${marker})`);
    if (Date.now() >= deadline) return;
    await delay(25);
  }
}

/** `--port 0` is refused by the CLI, so reserve a concrete free port first. */
function freePort() {
  return new Promise((resolvePort, rejectPort) => {
    const probe = net.createServer();
    probe.once('error', rejectPort);
    probe.listen(0, '127.0.0.1', () => {
      const { port } = probe.address();
      probe.close(() => resolvePort(port));
    });
  });
}

function webDistFixture() {
  // resolveWebDist requires <root>/index.html to exist.
  const root = join(tempDir('nexus-boot-webdist-'), 'dist');
  mkdirSync(root);
  writeFileSync(join(root, 'index.html'), '<!doctype html><html><body>nexus-boot-fixture</body></html>');
  return root;
}

/** Executable `open`/`xdg-open` that records its argv instead of opening. */
function openShim(marker) {
  const dir = tempDir('nexus-boot-shim-');
  const shim = join(dir, OPEN_COMMAND);
  writeFileSync(shim, `#!/bin/sh\nprintf '%s\\n' "$*" >> '${marker}'\n`);
  chmodSync(shim, 0o755);
  return dir;
}

function markerText(marker) {
  return existsSync(marker) ? readFileSync(marker, 'utf8') : '';
}

function bootService({ port, home, webDist, noOpen = false, shimDir }) {
  const args = [
    join(serviceRoot, 'dist', 'main.js'),
    '--home',
    home,
    '--host',
    '127.0.0.1',
    '--port',
    String(port),
  ];
  if (webDist) args.push('--web-dist', webDist);
  if (noOpen) args.push('--no-open');

  const child = spawn(process.execPath, args, {
    env: { ...process.env, PATH: `${shimDir}${delimiter}${process.env.PATH ?? ''}` },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
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

  return {
    child,
    get stdout() {
      return stdout;
    },
    get stderr() {
      return stderr;
    },
    listening: () =>
      waitFor(
        () => stderr.includes('[nexus-service] listening on'),
        () => `service never reported listening; stderr:\n${stderr}`,
      ),
    openUrl: () =>
      waitFor(
        () => OPEN_LINE.exec(stderr)?.[1] ?? null,
        () => `no "open:" line; stderr:\n${stderr}`,
      ),
    stop: () =>
      new Promise((resolveStop) => {
        if (child.exitCode !== null || child.signalCode !== null) {
          resolveStop();
          return;
        }
        child.once('exit', resolveStop);
        child.kill('SIGTERM');
      }),
  };
}

function get(url) {
  return new Promise((resolveRequest, rejectRequest) => {
    const req = http.request(url, { method: 'GET', agent: false }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () =>
        resolveRequest({
          status: res.statusCode,
          headers: res.headers,
          text: Buffer.concat(chunks).toString('utf8'),
        }),
      );
    });
    req.on('error', rejectRequest);
    req.end();
  });
}

function assertBootstrapUrl(url, port) {
  assert.match(url, new RegExp(`^http://127\\.0\\.0\\.1:${port}/__bootstrap\\?token=[A-Za-z0-9_-]{43}$`));
}

test('--no-open prints the bootstrap URL without opening it, and its token mints a session', async () => {
  const marker = join(tempDir('nexus-boot-marker-'), 'opened');
  const shimDir = openShim(marker);
  const port = await freePort();
  const service = bootService({
    port,
    home: tempDir('nexus-boot-home-'),
    webDist: webDistFixture(),
    noOpen: true,
    shimDir,
  });
  try {
    await service.listening();
    const url = await service.openUrl();
    assertBootstrapUrl(url, port);
    const stdoutLines = service.stdout.split('\n').filter(Boolean);
    assert.equal(stdoutLines.length, 1, `stdout must be the single ready line, got: ${JSON.stringify(stdoutLines)}`);
    assert.match(stdoutLines[0], /^NEXUS_SERVICE_READY \{/);
    assert.ok(!service.stdout.includes('[nexus-service]'), 'human logs must stay on stderr');

    await delay(500);
    assert.equal(markerText(marker), '', '--no-open must not launch the opener');

    const response = await get(url);
    assert.equal(response.status, 302);
    const cookie = response.headers['set-cookie']?.[0] ?? '';
    assert.match(cookie, /(?:^|;)\s*nexus_session=[A-Za-z0-9_-]+/, `bootstrap must set a session cookie, got: ${cookie}`);
    assert.match(cookie, /HttpOnly/i);
  } finally {
    await service.stop();
  }
});

test('--web-dist hands the printed bootstrap URL to the opener', { skip: isWindows }, async () => {
  const marker = join(tempDir('nexus-boot-marker-'), 'opened');
  const shimDir = openShim(marker);
  const port = await freePort();
  const service = bootService({
    port,
    home: tempDir('nexus-boot-home-'),
    webDist: webDistFixture(),
    shimDir,
  });
  try {
    await service.listening();
    const url = await service.openUrl();
    assertBootstrapUrl(url, port);
    await waitFor(
      () => markerText(marker).trim() === url,
      () => `opener argv ${JSON.stringify(markerText(marker))} never became the printed URL ${url}`,
    );
  } finally {
    await service.stop();
  }
});

test('without --web-dist the URL is still printed but never opened', { skip: isWindows }, async () => {
  const marker = join(tempDir('nexus-boot-marker-'), 'opened');
  const shimDir = openShim(marker);
  const port = await freePort();
  const service = bootService({ port, home: tempDir('nexus-boot-home-'), shimDir });
  try {
    await service.listening();
    const url = await service.openUrl();
    assertBootstrapUrl(url, port);
  } finally {
    await service.stop();
  }
  // Stop first, then observe: a build missing the `args.webDist` conjunct calls
  // the opener synchronously right after the `open:` print, so the shim must
  // have written by now — but its shell write can trail under load. Poll the
  // full absence window rather than trusting one sample.
  await assertOpenerNeverRan(marker, 'auto-open without --web-dist would land on route_not_migrated');
});
