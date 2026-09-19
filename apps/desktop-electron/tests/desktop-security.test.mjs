#!/usr/bin/env node
/**
 * Desktop trust-boundary tests (v1.192 P0-T1).
 *
 * Defends the frozen IPC and scheme/navigation contracts without launching
 * Electron: parser bounds + closed shapes, sender identity (top frame, live
 * window, exact app origin, generation), admission caps, asset path
 * traversal/symlink escape, HTML-only SPA fallback, exact-origin CSP, and the
 * single external-URL predicate. Runs against the compiled output:
 *   pnpm --dir apps/desktop-electron run build && node --test apps/desktop-electron/tests/desktop-security.test.mjs
 */
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  DESKTOP_OPERATIONS,
  MAX_PATH_BYTES,
  MAX_REQUEST_BYTES,
  MAX_URL_BYTES,
  desktopErr,
  desktopError,
  desktopOk,
  errorCode,
  isDesktopAppOrigin,
  isDesktopResponse,
  parseDesktopRequest,
  parseDesktopRuntimeMetadata,
} from '../dist/desktop-contract.js';
import { DesktopAdmission, assertDesktopSender } from '../dist/desktop-ipc.js';
import {
  allowDesktopNavigation,
  assertDesktopServiceOrigin,
  buildDesktopCsp,
  desktopPathHasTraversal,
  isAllowedDesktopExternalUrl,
  isDesktopHtmlNavigation,
  resolveDesktopAssetPath,
} from '../dist/protocol.js';

const VALID_SENDER = {
  windowAlive: true,
  senderIsSelectedWebContents: true,
  senderFramePresent: true,
  senderFrameIsMainFrame: true,
  frameUrl: 'nexus://app/index.html',
  generation: 7,
};

function expectCode(fn, code) {
  assert.throws(fn, (err) => errorCode(err) === code);
}

// ---------------------------------------------------------------------------
// Request parser: envelope, bounds, closed shapes
// ---------------------------------------------------------------------------

test('valid top-frame request parses with normalized payload', () => {
  const req = parseDesktopRequest({
    request_id: 'req-1.2_3-4',
    operation: 'open_with',
    payload: { path: '/workspace/chapter.md' },
  });
  assert.equal(req.request_id, 'req-1.2_3-4');
  assert.equal(req.operation, 'open_with');
  assert.deepEqual(req.payload, { path: '/workspace/chapter.md' });
});

test('request_id must match the frozen ASCII identifier grammar', () => {
  expectCode(() => parseDesktopRequest({ request_id: 'bad id!', operation: 'get_daemon_status' }), 'invalid_input');
  expectCode(
    () => parseDesktopRequest({ request_id: 'x'.repeat(129), operation: 'get_daemon_status' }),
    'invalid_input',
  );
  expectCode(() => parseDesktopRequest({ request_id: 42, operation: 'get_daemon_status' }), 'invalid_input');
});

test('unknown operation is rejected before effects', () => {
  expectCode(
    () => parseDesktopRequest({ request_id: 'r1', operation: 'exec_arbitrary', payload: {} }),
    'invalid_input',
  );
  assert.equal(DESKTOP_OPERATIONS.includes('exec_arbitrary'), false);
});

test('oversized frame is rejected (1 MiB request bound)', () => {
  const huge = 'a'.repeat(MAX_REQUEST_BYTES);
  expectCode(
    () => parseDesktopRequest({ request_id: 'r1', operation: 'set_workspace_path', payload: { path: huge } }),
    'input_too_large',
  );
});

test('unknown payload fields are rejected (closed shapes)', () => {
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'open_with',
        payload: { path: '/ok', extra: 'nope' },
      }),
    'invalid_input',
  );
  expectCode(
    () => parseDesktopRequest({ request_id: 'r1', operation: 'open_with', payload: ['/ok'] }),
    'invalid_input',
  );
});

test('null-payload operations reject any payload', () => {
  expectCode(
    () => parseDesktopRequest({ request_id: 'r1', operation: 'get_workspace_root', payload: { path: '/x' } }),
    'invalid_input',
  );
  // explicit null is tolerated
  const req = parseDesktopRequest({ request_id: 'r1', operation: 'get_workspace_root', payload: null });
  assert.equal(req.payload, undefined);
});

test('path fields: control characters and oversized paths rejected', () => {
  expectCode(
    () => parseDesktopRequest({ request_id: 'r1', operation: 'open_with', payload: { path: '/ok\x00.sh' } }),
    'invalid_input',
  );
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'open_with',
        payload: { path: 'a'.repeat(MAX_PATH_BYTES + 1) },
      }),
    'input_too_large',
  );
});

test('url and id fields enforce frozen byte bounds', () => {
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'open_external_url',
        payload: { url: `https://example.com/${'u'.repeat(MAX_URL_BYTES)}` },
      }),
    'input_too_large',
  );
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'switch_active_creator',
        payload: { creatorId: 'c'.repeat(257) },
      }),
    'input_too_large',
  );
});

test('connection config is public-only: unknown fields and key readback rejected', () => {
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'set_connection_config',
        payload: {
          config: { endpointUrl: 'http://127.0.0.1:8420', hasApiKey: true, apiKey: 'secret' },
          credential: { action: 'keep' },
        },
      }),
    'invalid_input',
  );
  const req = parseDesktopRequest({
    request_id: 'r1',
    operation: 'set_connection_config',
    payload: {
      config: { endpointUrl: 'http://127.0.0.1:8420', label: 'local', hasApiKey: true },
      credential: { action: 'replace', value: 'fresh-key' },
    },
  });
  assert.deepEqual(req.payload.credential, { action: 'replace', value: 'fresh-key' });
  expectCode(
    () =>
      parseDesktopRequest({
        request_id: 'r1',
        operation: 'set_connection_config',
        payload: {
          config: { endpointUrl: 'http://127.0.0.1:8420', hasApiKey: true },
          credential: { action: 'maybe' },
        },
      }),
    'invalid_input',
  );
});

test('response envelope helpers honor the 1 MiB response bound', () => {
  const ok = desktopOk('r1', { value: 1 });
  assert.equal(ok.ok, true);
  assert.equal(isDesktopResponse(ok), true);
  assert.equal(isDesktopResponse({ nope: 1 }), false);
  const failure = desktopErr('r1', 'busy', 'cap');
  assert.deepEqual(failure, { request_id: 'r1', ok: false, error: { code: 'busy', message: 'cap' } });
  assert.throws(() => desktopOk('r1', { blob: 'x'.repeat(MAX_REQUEST_BYTES) }), (err) => errorCode(err) === 'internal');
});

test('runtime metadata is nonsecret and http(s)-only', () => {
  assert.deepEqual(parseDesktopRuntimeMetadata({ localEndpoint: 'http://127.0.0.1:8420' }), {
    localEndpoint: 'http://127.0.0.1:8420',
  });
  expectCode(() => parseDesktopRuntimeMetadata({ localEndpoint: 'file:///etc/passwd' }), 'invalid_input');
  expectCode(() => parseDesktopRuntimeMetadata({ localEndpoint: 'http://x', apiKey: 'k' }), 'invalid_input');
});

// ---------------------------------------------------------------------------
// Sender identity: subframes, stale senders, exact app origin
// ---------------------------------------------------------------------------

test('valid top-frame app-origin sender is allowed', () => {
  assert.doesNotThrow(() => assertDesktopSender(VALID_SENDER, 7));
});

test('subframe and frameless senders are rejected (no getURL fallback)', () => {
  expectCode(
    () => assertDesktopSender({ ...VALID_SENDER, senderFrameIsMainFrame: false }, 7),
    'invalid_sender',
  );
  expectCode(() => assertDesktopSender({ ...VALID_SENDER, senderFramePresent: false, frameUrl: null }, 7), 'invalid_sender');
});

test('stale sender from a previous window generation is rejected', () => {
  expectCode(() => assertDesktopSender({ ...VALID_SENDER, generation: 6 }, 7), 'stale_sender');
});

test('dead window or foreign webContents is rejected', () => {
  expectCode(() => assertDesktopSender({ ...VALID_SENDER, windowAlive: false }, 7), 'invalid_sender');
  expectCode(
    () => assertDesktopSender({ ...VALID_SENDER, senderIsSelectedWebContents: false }, 7),
    'invalid_sender',
  );
});

test('sender frame URL must be the exact app origin', () => {
  for (const frameUrl of [
    'https://app/index.html',
    'file:///app/index.html',
    'nexus://other/index.html',
    'nexus://app@evil/index.html',
    'nexus://user:pass@app/index.html',
    'javascript:alert(1)',
  ]) {
    expectCode(() => assertDesktopSender({ ...VALID_SENDER, frameUrl }, 7), 'invalid_origin');
  }
});

test('dev HMR origin is allowed only under the explicit dev option', () => {
  expectCode(
    () => assertDesktopSender({ ...VALID_SENDER, frameUrl: 'http://localhost:5173/' }, 7),
    'invalid_origin',
  );
  assert.doesNotThrow(() => assertDesktopSender({ ...VALID_SENDER, frameUrl: 'http://localhost:5173/' }, 7, { dev: true }));
  // even in dev, only the exact Vite origin — not arbitrary http
  expectCode(
    () => assertDesktopSender({ ...VALID_SENDER, frameUrl: 'http://localhost:5174/' }, 7, { dev: true }),
    'invalid_origin',
  );
});

test('isDesktopAppOrigin rejects credentials and non-app hosts', () => {
  assert.equal(isDesktopAppOrigin('nexus://app/index.html'), true);
  assert.equal(isDesktopAppOrigin('nexus://app:0/index.html'), true);
  assert.equal(isDesktopAppOrigin('nexus://app:8080/index.html'), false);
  assert.equal(isDesktopAppOrigin('nexus://App/index.html'), false);
  assert.equal(isDesktopAppOrigin('nexus://app.evil/index.html'), false);
});

// ---------------------------------------------------------------------------
// Admission caps: 32 active + 16 queued, queued aggregate ≤ 1 MiB, overflow busy
// ---------------------------------------------------------------------------

test('admission allows 32 active, queues the next 16, rejects the 49th with busy', async () => {
  const admission = new DesktopAdmission();
  const releases = [];
  for (let i = 0; i < 32; i += 1) {
    const release = await admission.admit(1);
    assert.ok(release);
    releases.push(release);
  }
  const queued = [];
  for (let i = 0; i < 16; i += 1) {
    const pending = admission.admit(1);
    queued.push(pending);
  }
  // 49th concurrent call exceeds 32 active + 16 queued
  assert.equal(await admission.admit(1), null);
  const firstQueued = queued[0];
  releases[0]();
  assert.ok(await firstQueued);
  releases.slice(1).forEach((release) => release());
  for (const pending of queued.slice(1)) assert.ok(await pending);
});

test('queued aggregate byte cap rejects overflow with busy', async () => {
  const admission = new DesktopAdmission();
  const releases = [];
  for (let i = 0; i < 32; i += 1) {
    releases.push(await admission.admit(1));
  }
  // A 1 MiB frame alone fits the empty queue (aggregate ≤ 1 MiB is admitted);
  // overflow needs prior queued bytes.
  const smallQueued = admission.admit(1);
  assert.equal(await admission.admit(MAX_REQUEST_BYTES), null); // 1 byte + 1 MiB > cap
  releases.forEach((release) => release());
  assert.ok(await smallQueued);
});

// ---------------------------------------------------------------------------
// Scheme / asset path policy: traversal, symlink escape, regular files only
// ---------------------------------------------------------------------------

function makeDist() {
  const root = mkdtempSync(join(tmpdir(), 'nexus-dist-'));
  mkdirSync(join(root, 'assets'), { recursive: true });
  writeFileSync(join(root, 'index.html'), '<html></html>');
  writeFileSync(join(root, 'assets', 'app.js'), 'console.log(1)');
  return root;
}

test('serves real files under the dist root', () => {
  const root = makeDist();
  try {
    // macOS realpaths /tmp under /private — compare against the canonical root.
    const canonicalRoot = realpathSync(root);
    assert.equal(resolveDesktopAssetPath(root, '/index.html'), join(canonicalRoot, 'index.html'));
    assert.equal(resolveDesktopAssetPath(root, '/assets/app.js'), join(canonicalRoot, 'assets', 'app.js'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('path traversal is rejected, encoded or plain', () => {
  const root = makeDist();
  try {
    assert.equal(resolveDesktopAssetPath(root, '/../secret.txt'), null);
    assert.equal(resolveDesktopAssetPath(root, '/assets/../../secret.txt'), null);
    assert.equal(resolveDesktopAssetPath(root, '/%2e%2e/%2e%2e/secret.txt'), null);
    assert.equal(resolveDesktopAssetPath(root, '/..%2f..%2fsecret.txt'), null);
    assert.equal(desktopPathHasTraversal('/%2e%2e/x'), true);
    assert.equal(desktopPathHasTraversal('/assets/app.js'), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('symlink escape outside the dist root is rejected', () => {
  const root = makeDist();
  const outside = mkdtempSync(join(tmpdir(), 'nexus-outside-'));
  try {
    writeFileSync(join(outside, 'secret.txt'), 'top secret');
    symlinkSync(join(outside, 'secret.txt'), join(root, 'assets', 'linked.js'));
    // Symlinks are not regular files: rejected even before containment.
    assert.equal(resolveDesktopAssetPath(root, '/assets/linked.js'), null);
    // Directory symlink pointing outside is likewise rejected.
    symlinkSync(outside, join(root, 'assets', 'escape'), 'dir');
    assert.equal(resolveDesktopAssetPath(root, '/assets/escape/secret.txt'), null);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(outside, { recursive: true, force: true });
  }
});

test('unknown asset resolves to null (handler maps it to 404)', () => {
  const root = makeDist();
  try {
    assert.equal(resolveDesktopAssetPath(root, '/missing.js'), null);
    assert.equal(resolveDesktopAssetPath(root, '/assets'), null); // directory, not a regular file
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('SPA fallback applies to HTML navigation only, never API/assets/traversal', () => {
  assert.equal(isDesktopHtmlNavigation('text/html,application/xhtml+xml'), true);
  assert.equal(isDesktopHtmlNavigation('application/json'), false);
  assert.equal(isDesktopHtmlNavigation(undefined), false);
  // fallback decision chain mirrors the handler: traversal never falls back
  assert.equal(desktopPathHasTraversal('/v1/daemon/health/../../secret'), true);
});

// ---------------------------------------------------------------------------
// Navigation and external URL policy
// ---------------------------------------------------------------------------

test('navigation is denied outside the app origin', () => {
  assert.equal(allowDesktopNavigation('nexus://app/workspace'), true);
  assert.equal(allowDesktopNavigation('https://evil.example/'), false);
  assert.equal(allowDesktopNavigation('file:///etc/passwd'), false);
  assert.equal(allowDesktopNavigation('javascript:alert(1)'), false);
  assert.equal(allowDesktopNavigation(' data:text/html,x'), false);
  assert.equal(allowDesktopNavigation('nexus://app:12345/'), false);
  assert.equal(allowDesktopNavigation('http://localhost:5173/'), false); // dev not default
  assert.equal(allowDesktopNavigation('http://localhost:5173/', { dev: true }), true);
});

test('external URL predicate: http/https with host only, no userinfo/controls', () => {
  assert.equal(isAllowedDesktopExternalUrl('https://github.com/42ch-dev/nexus'), true);
  assert.equal(isAllowedDesktopExternalUrl('http://example.com/page'), true);
  assert.equal(isAllowedDesktopExternalUrl('https://user:pass@example.com/'), false);
  assert.equal(isAllowedDesktopExternalUrl('javascript:alert(1)'), false);
  assert.equal(isAllowedDesktopExternalUrl('data:text/html,x'), false);
  assert.equal(isAllowedDesktopExternalUrl('file:///etc/passwd'), false);
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/\x1f'), false);
  assert.equal(isAllowedDesktopExternalUrl(' https://example.com/'), false);
  assert.equal(isAllowedDesktopExternalUrl('https://'), false);
});

// ---------------------------------------------------------------------------
// CSP: exact origins, no wildcards, no renderer strings
// ---------------------------------------------------------------------------

test('CSP inserts exact validated origins and keeps the frozen posture', () => {
  const csp = buildDesktopCsp({
    serviceOrigin: 'http://127.0.0.1:8420',
    fingerprintProbeOrigin: 'https://probe.example.com',
  });
  assert.match(csp, /default-src 'self'/);
  assert.match(csp, /script-src 'self'/);
  assert.match(csp, /style-src 'self' 'unsafe-inline'/);
  assert.match(csp, /img-src 'self' data: blob: https:/);
  assert.match(csp, /connect-src 'self' http:\/\/127\.0\.0\.1:8420 https:\/\/probe\.example\.com/);
  assert.match(csp, /object-src 'none'/);
  assert.match(csp, /base-uri 'none'/);
  assert.match(csp, /frame-src 'none'/);
  assert.match(csp, /frame-ancestors 'none'/);
  assert.match(csp, /form-action 'none'/);
  assert.doesNotMatch(csp, /unsafe-eval/);
  assert.doesNotMatch(csp, /\*/);
});

test('CSP dev mode adds only the exact Vite origins and HMR websocket origin', () => {
  const csp = buildDesktopCsp({
    serviceOrigin: 'http://127.0.0.1:8420',
    fingerprintProbeOrigin: 'https://probe.example.com',
    dev: true,
  });
  assert.match(csp, /connect-src [^;]*http:\/\/localhost:5173 http:\/\/127\.0\.0\.1:5173 ws:\/\/localhost:5173 ws:\/\/127\.0\.0\.1:5173/);
});

test('CSP rejects wildcard, credentialed, or path-carrying origins', () => {
  assert.throws(() => assertDesktopServiceOrigin('*'));
  assert.throws(() => assertDesktopServiceOrigin('https://*.example.com'));
  assert.throws(() => assertDesktopServiceOrigin('https://user:pass@example.com'));
  assert.throws(() => assertDesktopServiceOrigin('https://example.com/path'));
  assert.throws(() => assertDesktopServiceOrigin('nexus://app'));
  assert.throws(() => assertDesktopServiceOrigin('not a url'));
  assert.equal(assertDesktopServiceOrigin('https://example.com/'), 'https://example.com');
});

// ---------------------------------------------------------------------------
// Status frame bounds (preload/main both enforce)
// ---------------------------------------------------------------------------

test('status frame bound: detail tail over 2 KiB and frame over 4 KiB rejected', async () => {
  const { assertDesktopStatusFrame } = await import('../dist/desktop-contract.js');
  const valid = assertDesktopStatusFrame({ state: 'running', port: 8420, detail: 'ok' });
  assert.equal(valid.state, 'running');
  expectCode(() => assertDesktopStatusFrame({ state: 'running', port: 8420, detail: 'd'.repeat(2049) }), 'input_too_large');
  expectCode(
    () => assertDesktopStatusFrame({ state: 'running', port: 99999 }),
    'invalid_input',
  );
  expectCode(() => assertDesktopStatusFrame({ state: 'unknown', port: 1 }), 'invalid_input');
  expectCode(() => assertDesktopStatusFrame({ state: 'running', port: 1, extra: 1 }), 'invalid_input');
});

test('desktopError carries a machine-readable code', () => {
  const err = desktopError('invalid_origin', 'nope');
  assert.equal(errorCode(err), 'invalid_origin');
  assert.equal(errorCode(new Error('plain')), 'internal');
});

// ---------------------------------------------------------------------------
// Preload/contract channel parity: the sandboxed preload mirrors the channel
// names locally (it compiles CommonJS and cannot import the ESM contract —
// see preload.ts header); the compiled artifact must stay in lockstep.
// ---------------------------------------------------------------------------

test('compiled preload uses the exact contract channels and no proof surface', async () => {
  const { readFileSync } = await import('node:fs');
  const { fileURLToPath } = await import('node:url');
  const preloadPath = fileURLToPath(new URL('../dist/preload.js', import.meta.url));
  const source = readFileSync(preloadPath, 'utf8');
  const contract = await import('../dist/desktop-contract.js');
  for (const channel of [
    contract.DESKTOP_INVOKE_CHANNEL,
    contract.DESKTOP_STATUS_CHANNEL,
    contract.DESKTOP_RUNTIME_CHANNEL,
  ]) {
    assert.ok(source.includes(channel), `preload must use channel ${channel}`);
  }
  for (const operation of contract.DESKTOP_OPERATIONS) {
    assert.ok(
      source.includes(`'${operation}'`),
      `preload operation union must include ${operation}`,
    );
  }
  // Preload must be self-contained: importing the contract module would
  // re-emit it as CommonJS and clobber the ESM artifact the main process loads.
  assert.ok(!/require\(["']\.\/desktop-contract/.test(source), 'preload must not require desktop-contract');
  assert.ok(!/from ["']\.\/desktop-contract/.test(source), 'preload must not import desktop-contract');
  // Product preload must not expose the retired proof bridge or raw primitives.
  assert.ok(!source.includes('nexusProof'));
  assert.ok(!source.includes('nexus-proof:'));
  assert.ok(!source.includes('runProofStep'));
  assert.ok(source.includes('contextBridge'));
});
