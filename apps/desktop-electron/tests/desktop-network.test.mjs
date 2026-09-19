#!/usr/bin/env node
/**
 * Exact-origin network auth policy tests (v1.192 P0-T5).
 *
 * Defends the frozen rule: the stored X-API-Key is attached ONLY to the
 * exact configured origin (scheme/host/port) AND the /v1/daemon/ path
 * prefix, only for fetch/XHR. Foreign hosts, redirects that leave the
 * pinned origin, fingerprint probes, images and an inactive endpoint never
 * receive the credential; renderer-supplied auth headers are stripped
 * first. Tests exercise the real hook wiring through a fake session.
 *
 *   node --test apps/desktop-electron/tests/desktop-network.test.mjs
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  AUTH_HEADER,
  applyAuthHeaders,
  attachDesktopNetworkHooks,
  decideAuthInjection,
} from '../dist/desktop-network.js';

const AUTH = { endpointOrigin: 'https://daemon.example.com:8443', apiKey: 'sk-live-secret' };
const GOOD_URL = 'https://daemon.example.com:8443/v1/daemon/runtime/discovery';
const FETCH = { url: GOOD_URL, resourceType: 'fetch' };

/**
 * Read a header from Electron's real `webRequest` header-map shape
 * (`Record<string, string>`), case-insensitively.
 */
function headerValue(headers, name) {
  if (headers == null || typeof headers !== 'object' || Array.isArray(headers)) {
    throw new Error(`requestHeaders must be Electron's header map, got: ${JSON.stringify(headers)}`);
  }
  const key = Object.keys(headers).find((k) => k.toLowerCase() === name.toLowerCase());
  return key === undefined ? undefined : headers[key];
}

/** Count headers matching a name (case-insensitive) in the header map. */
function headerCount(headers, name) {
  return Object.keys(headers).filter((k) => k.toLowerCase() === name.toLowerCase()).length;
}

// ---------------------------------------------------------------------------
// Pure decision: exact origin + path prefix + resource type + active config
// ---------------------------------------------------------------------------

test('injects for fetch to the exact pinned origin and daemon prefix', () => {
  assert.deepEqual(decideAuthInjection(AUTH, FETCH), { inject: 'sk-live-secret' });
  assert.equal(
    headerValue(applyAuthHeaders(AUTH, FETCH), 'x-api-key'),
    'sk-live-secret',
  );
});

test('never injects on a foreign host, even same path', () => {
  assert.deepEqual(
    decideAuthInjection(AUTH, { url: 'https://evil.example.com/v1/daemon/status', resourceType: 'fetch' }),
    { inject: null },
  );
  assert.equal(
    headerValue(applyAuthHeaders(AUTH, { url: 'https://evil.example.com/v1/daemon/status', resourceType: 'fetch' }), 'x-api-key'),
    undefined,
  );
});

test('never injects on the same host via a different scheme or port', () => {
  for (const url of [
    'http://daemon.example.com:8443/v1/daemon/status', // scheme downgrade
    'https://daemon.example.com/v1/daemon/status', // default port ≠ pinned 8443
    'https://sub.daemon.example.com:8443/v1/daemon/status', // different host
  ]) {
    assert.deepEqual(decideAuthInjection(AUTH, { url, resourceType: 'fetch' }), { inject: null }, url);
  }
});

test('redirect target that left the pinned origin drops the credential', () => {
  // Original request authenticated...
  assert.deepEqual(decideAuthInjection(AUTH, FETCH), { inject: 'sk-live-secret' });
  // ...redirect (301/302) decided per request: foreign target gets nothing.
  assert.deepEqual(
    decideAuthInjection(AUTH, { url: 'https://other.example.net/v1/daemon/status', resourceType: 'fetch' }),
    { inject: null },
  );
});

test('never injects outside the daemon path prefix on the pinned origin', () => {
  for (const url of [
    'https://daemon.example.com:8443/v1/works', // other API family on same origin
    'https://daemon.example.com:8443/', // fingerprint probe / root
    'https://daemon.example.com:8443/v1/daemonx', // prefix boundary (no separator)
  ]) {
    assert.deepEqual(decideAuthInjection(AUTH, { url, resourceType: 'fetch' }), { inject: null }, url);
  }
  // Exact prefix with a real path segment is fine.
  assert.deepEqual(
    decideAuthInjection(AUTH, { url: 'https://daemon.example.com:8443/v1/daemon/', resourceType: 'xhr' }),
    { inject: 'sk-live-secret' },
  );
});

test('never injects for non-fetch resource types (image, media, subresource)', () => {
  for (const resourceType of ['image', 'media', 'mainFrame', 'subFrame', 'script', 'stylesheet']) {
    assert.deepEqual(
      decideAuthInjection(AUTH, { url: GOOD_URL, resourceType }),
      { inject: null },
      resourceType,
    );
  }
});

test('inherited/prototype resource-type names never match the allowlist', () => {
  for (const resourceType of ['constructor', 'toString', 'hasOwnProperty', '__proto__', 'valueOf']) {
    assert.deepEqual(
      decideAuthInjection(AUTH, { url: GOOD_URL, resourceType }),
      { inject: null },
      resourceType,
    );
    assert.equal(
      headerValue(applyAuthHeaders(AUTH, { url: GOOD_URL, resourceType }), 'x-api-key'),
      undefined,
      resourceType,
    );
  }
});

test('hook response uses the real Electron header-map shape, not a HeaderEntry[]', () => {
  const request = fakeSession();
  const authed = request({ url: GOOD_URL, resourceType: 'xhr' }, () => AUTH);
  assert.ok(authed !== null && typeof authed === 'object' && !Array.isArray(authed),
    'callback response must be Electron\'s Record<string, string> header map');
  assert.equal(authed['X-API-Key'], 'sk-live-secret');
});

test('inactive or keyless endpoint: no auth anywhere', () => {
  assert.deepEqual(decideAuthInjection(null, FETCH), { inject: null });
  assert.equal(headerValue(applyAuthHeaders(null, FETCH), 'x-api-key'), undefined);
});

test('malformed request URLs never carry auth', () => {
  assert.deepEqual(
    decideAuthInjection(AUTH, { url: 'not a url', resourceType: 'fetch' }),
    { inject: null },
  );
});

// ---------------------------------------------------------------------------
// Header application: renderer-supplied auth stripped first
// ---------------------------------------------------------------------------

test('renderer-supplied X-API-Key is stripped before injection decision', () => {
  const headers = applyAuthHeaders(AUTH, {
    url: GOOD_URL,
    resourceType: 'fetch',
    requestHeaders: { 'X-API-Key': 'sk-evil-renderer', 'Accept': 'application/json' },
  });
  const values = headerCount(headers, AUTH_HEADER);
  assert.equal(values, 1, 'exactly one auth header survives');
  assert.equal(headerValue(headers, 'accept'), 'application/json');
});

test('renderer-supplied auth is stripped even when no injection is allowed', () => {
  const headers = applyAuthHeaders(AUTH, {
    url: 'https://evil.example.com/v1/daemon/status',
    resourceType: 'fetch',
    requestHeaders: { 'x-api-key': 'sk-evil-renderer' },
  });
  assert.equal(Object.keys(headers).length, 0, 'foreign request leaves with no auth header at all');
});

// ---------------------------------------------------------------------------
// Real hook wiring through a fake session: actual header isolation
// ---------------------------------------------------------------------------

function fakeSession() {
  const listeners = [];
  const session = {
    webRequest: {
      onBeforeSendHeaders(filter, listener) {
        listeners.push({ filter, listener });
      },
    },
  };
  /** Drive one request through the installed hook; returns final headers. */
  return function request(details, getAuth) {
    listeners.length = 0;
    attachDesktopNetworkHooks(session, getAuth);
    assert.equal(listeners.length, 1);
    let response;
    listeners[0].listener(details, (r) => {
      response = r;
    });
    return response.requestHeaders;
  };
}

test('hook injects only for the exact pinned origin via the session listener', () => {
  const request = fakeSession();
  const authed = request({ url: GOOD_URL, resourceType: 'xhr' }, () => AUTH);
  assert.equal(headerValue(authed, 'x-api-key'), 'sk-live-secret');

  const foreign = request({ url: 'https://evil.example.com/v1/daemon/status', resourceType: 'xhr' }, () => AUTH);
  assert.equal(headerValue(foreign, 'x-api-key'), undefined);

  // Connection switched off / cleared: the re-read authority returns null.
  const inactive = request({ url: GOOD_URL, resourceType: 'xhr' }, () => null);
  assert.equal(headerValue(inactive, 'x-api-key'), undefined);
});

test('hook strips renderer auth on every request class', () => {
  const request = fakeSession();
  const cases = [
    { url: GOOD_URL, expected: 'sk-live-secret' }, // main key replaces renderer key
    { url: 'https://evil.example.com/', expected: undefined },
    { url: 'https://daemon.example.com:8443/v1/works', expected: undefined },
  ];
  for (const { url, expected } of cases) {
    const headers = request(
      { url, resourceType: 'fetch', requestHeaders: { 'x-api-key': 'sk-evil' } },
      () => AUTH,
    );
    const values = headerCount(headers ?? {}, AUTH_HEADER);
    assert.equal(values, expected === undefined ? 0 : 1, url);
    assert.equal(headerValue(headers, AUTH_HEADER), expected, url);
  }
});

test('hook authenticates the pinned-origin daemon request and nothing else on redirect chains', () => {
  const request = fakeSession();
  const chain = [
    { url: GOOD_URL, resourceType: 'fetch' },
    { url: 'https://daemon.example.com:8443/v1/daemon/stop', resourceType: 'fetch' },
    { url: 'https://redirected.example.org/v1/daemon/stop', resourceType: 'fetch' },
  ];
  const results = chain.map((d) => headerValue(request(d, () => AUTH), 'x-api-key'));
  assert.deepEqual(results, ['sk-live-secret', 'sk-live-secret', undefined]);
});
