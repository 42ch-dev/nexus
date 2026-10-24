#!/usr/bin/env node
/**
 * I-000061 — the plain-HTTP LAN entrance gate matrix.
 *
 * Both gates are pure functions, so this suite needs no socket and never opens
 * a listener on a non-loopback address; every row is a direct call.
 *
 * Startup (`validateStartupBind`, bind.ts:31-52) must stay:
 *  - unix transport exempt (its socket is guarded by directory/file mode);
 *  - loopback always allowed, with or without a key or TLS;
 *  - non-loopback: `--allow-remote` (403) first, then an API key (401);
 *  - TLS optional — a keyed remote bind without TLS material must be admitted,
 *    which is what keeps plain-HTTP LAN dev working (access spec §3.1).
 *
 * Request (`validateBindPolicy`, security.ts:44-65) must stay:
 *  - loopback (including a missing remote address) always allowed;
 *  - non-loopback without `--allow-remote` refused, authenticated or not;
 *  - non-loopback without TLS refused ONLY for unauthenticated requests —
 *    `authenticated` is set by API-key/session-cookie admission, and
 *    `/__bootstrap` always passes `true` because the token is the proof
 *    (server.ts:387).
 *
 * Red-on-revert (demonstrated by the task, not asserted here): reintroducing a
 * TLS startup precondition fails "plain-HTTP LAN bind is admitted"; dropping the
 * `!authenticated` guard fails "unauthenticated no-TLS remote request refused".
 */
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, test } from 'node:test';

const serviceRoot = join(dirname(fileURLToPath(import.meta.url)), '..');

const { isLoopbackBindHost, validateStartupBind } = await import(join(serviceRoot, 'dist/bind.js'));
const { isLoopbackAddress, validateBindPolicy } = await import(join(serviceRoot, 'dist/security.js'));

/** A resolved-config stand-in: the gates read only these fields. */
function config(overrides = {}) {
  return {
    home: '/nonexistent-nexus-home',
    host: '127.0.0.1',
    port: 18_061,
    allowRemote: false,
    domainOnly: true,
    transport: 'http',
    apiKey: null,
    allowedOrigins: [],
    embeddedMcp: false,
    webDist: null,
    ...overrides,
  };
}

function startupError(overrides) {
  try {
    validateStartupBind(config(overrides));
    return null;
  } catch (error) {
    return error;
  }
}

function assertRefused(error, status, code) {
  assert.ok(error, 'the gate must refuse this row');
  assert.equal(error.status, status);
  assert.equal(error.code, code);
}

function assertAdmitted(failure) {
  assert.equal(failure, null, 'the gate must admit this row');
}

const LOOPBACK_HOSTS = ['127.0.0.1', '127.0.0.53', 'localhost', 'LOCALHOST', '::1', '[::1]'];

describe('validateStartupBind — gate before listen', () => {
  test('unix transport is exempt from the remote-bind policy', () => {
    assertAdmitted(
      startupError({ transport: 'unix', host: '0.0.0.0', allowRemote: false, apiKey: null }),
    );
    assertAdmitted(
      startupError({ transport: 'unix', host: '::', allowRemote: false, apiKey: null }),
    );
  });

  test('loopback binds are allowed without a key or TLS', () => {
    for (const host of LOOPBACK_HOSTS) {
      assertAdmitted(startupError({ host }));
    }
  });

  test('non-loopback without --allow-remote is refused 403 even with a key and TLS', () => {
    for (const host of ['0.0.0.0', '::', '10.0.0.5', '192.168.1.20', '127.0.0.1.evil']) {
      assertRefused(
        startupError({ host, allowRemote: false, apiKey: 'k', tlsCert: 'cert', tlsKey: 'key' }),
        403,
        'forbidden',
      );
    }
  });

  test('non-loopback with --allow-remote still requires an API key (401)', () => {
    // With TLS material: the key requirement is independent of TLS.
    assertRefused(
      startupError({ host: '10.0.0.5', allowRemote: true, tlsCert: 'cert', tlsKey: 'key' }),
      401,
      'auth_required',
    );
    assertRefused(startupError({ host: '0.0.0.0', allowRemote: true }), 401, 'auth_required');
  });

  test('plain-HTTP LAN bind is admitted: --allow-remote plus a key needs no TLS', () => {
    assertAdmitted(startupError({ host: '10.0.0.5', allowRemote: true, apiKey: 'k' }));
    assertAdmitted(startupError({ host: '0.0.0.0', allowRemote: true, apiKey: 'k' }));
  });

  test('keyed remote bind with TLS material is still admitted', () => {
    assertAdmitted(
      startupError({
        host: '10.0.0.5',
        allowRemote: true,
        apiKey: 'k',
        tlsCert: 'cert',
        tlsKey: 'key',
      }),
    );
  });

  test('the startup gate throws synchronously and runs before the listener is created', () => {
    assert.throws(
      () => validateStartupBind(config({ host: '10.0.0.5' })),
      (error) => error.status === 403,
      'the gate must refuse synchronously, not through a deferred path',
    );
    const source = readFileSync(join(serviceRoot, 'dist/index.js'), 'utf8');
    const gateAt = source.indexOf('validateStartupBind(config)');
    const listenAt = source.indexOf('listenServer(');
    assert.ok(gateAt >= 0, 'startService must call the startup gate');
    assert.ok(listenAt >= 0, 'startService must listen through listenServer');
    assert.ok(gateAt < listenAt, 'the startup gate must run before the listener is created');
  });
});

describe('validateStartupBind / isLoopbackBindHost classification', () => {
  test('loopback classification admits localhost, 127/8 and ::1 only', () => {
    for (const host of LOOPBACK_HOSTS) assert.equal(isLoopbackBindHost(host), true, host);
    assert.equal(isLoopbackBindHost('0:0:0:0:0:0:0:1'), true);
    for (const host of ['0.0.0.0', '::', '10.0.0.5', '192.168.1.20', '127.0.0.1.evil', 'localhost.evil']) {
      assert.equal(isLoopbackBindHost(host), false, host);
    }
  });
});

describe('validateBindPolicy — request gate', () => {
  const remoteConfig = { allowRemote: true, apiKey: 'k' };

  test('loopback requests are admitted without a key or TLS', () => {
    assertAdmitted(validateBindPolicy(config(), '127.0.0.1', false));
    assertAdmitted(validateBindPolicy(config(), '::1', false));
    assertAdmitted(validateBindPolicy(config(), '::ffff:127.0.0.1', false));
  });

  test('a missing remote address is treated as loopback', () => {
    assertAdmitted(validateBindPolicy(config(), undefined, false));
    assertAdmitted(validateBindPolicy(config(), '', false));
    assert.equal(isLoopbackAddress(undefined), true);
  });

  test('non-loopback without --allow-remote is refused 403, authenticated or not', () => {
    for (const authenticated of [false, true]) {
      assertRefused(
        validateBindPolicy(config({ tlsCert: 'cert', tlsKey: 'key' }), '10.0.0.5', authenticated),
        403,
        'forbidden',
      );
    }
  });

  test('non-loopback with --allow-remote and TLS admits unauthenticated requests', () => {
    assertAdmitted(
      validateBindPolicy(
        config({ ...remoteConfig, tlsCert: 'cert', tlsKey: 'key' }),
        '10.0.0.5',
        false,
      ),
    );
  });

  test('unauthenticated no-TLS remote request is refused (the only TLS refusal)', () => {
    const failure = validateBindPolicy(config(remoteConfig), '10.0.0.5', false);
    assertRefused(failure, 403, 'forbidden');
    assert.match(failure.message, /tls|authenticated/i);
  });

  test('plain-HTTP LAN request is admitted once authenticated', () => {
    assertAdmitted(validateBindPolicy(config(remoteConfig), '10.0.0.5', true));
    // Half TLS material is not TLS material.
    assertAdmitted(validateBindPolicy(config({ ...remoteConfig, tlsCert: 'cert' }), '10.0.0.5', true));
    assertAdmitted(validateBindPolicy(config({ ...remoteConfig, tlsKey: 'key' }), '10.0.0.5', true));
  });

  test('half TLS material does not admit an unauthenticated remote request', () => {
    assertRefused(
      validateBindPolicy(config({ ...remoteConfig, tlsCert: 'cert' }), '10.0.0.5', false),
      403,
      'forbidden',
    );
    assertRefused(
      validateBindPolicy(config({ ...remoteConfig, tlsKey: 'key' }), '10.0.0.5', false),
      403,
      'forbidden',
    );
  });

  test('/__bootstrap passes authenticated=true, so a tokenized LAN URL needs no TLS', () => {
    // server.ts:387 — the one-time token is the proof; only --allow-remote can
    // refuse this request, never TLS.
    assertAdmitted(validateBindPolicy(config(remoteConfig), '10.0.0.5', true));
    assertRefused(validateBindPolicy(config({ apiKey: 'k' }), '10.0.0.5', true), 403, 'forbidden');
  });

  test('IPv4-mapped and bracketed remote addresses stay non-loopback', () => {
    assertRefused(validateBindPolicy(config({}), '::ffff:10.0.0.5', false), 403, 'forbidden');
    assertRefused(validateBindPolicy(config({}), '10.0.0.5', false), 403, 'forbidden');
  });
});
