import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

import {
  REMEDIATION_COMMAND,
  BackendCompatibilityError,
  CURRENT_WRITER_PROTOCOL,
  RunningDaemonCompatibilityError,
  assertCompatibleBackend,
  assertCompatibleRunningDaemon,
  assertDaemonHealthIdentityMatchesManifest,
  computeContractHash,
  computeDbSchemaRange,
  computeDbSchemaRangeFromNames,
  isDaemonCliStatusRunning,
  manifestPathForArtifact,
  readBackendManifest,
  refreshBackend,
  resolveDaemonEndpoint,
  sha256File,
  validateDaemonHealth,
  waitForDaemonHealth,
  writeManifestAtomic,
} from './dev-backend-manifest.mjs';

function expectRemediation(err) {
  assert.match(String(err.message), new RegExp(REMEDIATION_COMMAND.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
}

test('readBackendManifest validates required manifest fields', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-schema-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    await writeFile(artifactPath, 'bin');
    const manifestPath = manifestPathForArtifact(artifactPath);
    await writeFile(manifestPath, JSON.stringify({ artifactPath, sha256: 'abc' }));
    await assert.rejects(() => readBackendManifest(artifactPath), /Backend manifest missing required field/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('assertCompatibleBackend refuses missing artifact with remediation text', async () => {
  await assert.rejects(
    () =>
      assertCompatibleBackend({
        artifactPath: join(tmpdir(), 'missing-nexus42'),
        contractHash: '0'.repeat(64),
        protocolVersion: CURRENT_WRITER_PROTOCOL,
      }),
    err => {
      assert.ok(err instanceof BackendCompatibilityError);
      expectRemediation(err);
      return true;
    },
  );
});

test('assertCompatibleBackend refuses hash mismatch with remediation text', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-hash-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    await writeFile(artifactPath, 'first-bytes');
    const contractHash = await computeContractHash();
    const manifest = {
      artifactPath,
      sha256: 'a'.repeat(64),
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash,
      nativeApiVersion: null,
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: computeDbSchemaRangeFromNames(['20260417_000001_initial']),
    };
    await writeFile(manifestPathForArtifact(artifactPath), `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      () => assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }),
      err => {
        assert.ok(err instanceof BackendCompatibilityError);
        expectRemediation(err);
        assert.match(err.message, /digest does not match manifest sha256/);
        return true;
      },
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('assertCompatibleBackend refuses contract hash mismatch with remediation text', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-contract-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    await writeFile(artifactPath, 'artifact');
    const digest = await sha256File(artifactPath);
    const manifest = {
      artifactPath,
      sha256: digest,
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash: 'b'.repeat(64),
      nativeApiVersion: null,
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: computeDbSchemaRangeFromNames(['20260417_000001_initial']),
    };
    await writeFile(manifestPathForArtifact(artifactPath), `${JSON.stringify(manifest, null, 2)}\n`);
    const currentContractHash = await computeContractHash();
    await assert.rejects(
      () =>
        assertCompatibleBackend({
          artifactPath,
          contractHash: currentContractHash,
          protocolVersion: CURRENT_WRITER_PROTOCOL,
        }),
      err => {
        assert.ok(err instanceof BackendCompatibilityError);
        expectRemediation(err);
        assert.match(err.message, /contractHash does not match current contract hash/);
        return true;
      },
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('assertCompatibleBackend refuses stale writer_protocol with remediation text', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-protocol-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    await writeFile(artifactPath, 'artifact');
    const digest = await sha256File(artifactPath);
    const contractHash = await computeContractHash();
    const range = await computeDbSchemaRange();
    const manifest = {
      artifactPath,
      sha256: digest,
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash,
      nativeApiVersion: null,
      writerProtocol: 99,
      dbSchemaRange: range,
    };
    await writeFile(manifestPathForArtifact(artifactPath), `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      () => assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }),
      err => {
        assert.ok(err instanceof BackendCompatibilityError);
        expectRemediation(err);
        assert.match(err.message, /writerProtocol 99 does not match required protocol 0/);
        return true;
      },
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('assertCompatibleBackend wraps malformed manifest with remediation text', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-malformed-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    await writeFile(artifactPath, 'artifact');
    await writeFile(manifestPathForArtifact(artifactPath), '{not-json');
    const contractHash = await computeContractHash();
    await assert.rejects(
      () => assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }),
      err => {
        assert.ok(err instanceof BackendCompatibilityError);
        expectRemediation(err);
        assert.match(err.message, /invalid/i);
        return true;
      },
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('assertCompatibleBackend refuses manifest artifactPath identity mismatch', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-identity-'));
  try {
    const artifactPath = join(dir, 'nexus42');
    const otherPath = join(dir, 'other-nexus42');
    await writeFile(artifactPath, 'artifact');
    const digest = await sha256File(artifactPath);
    const contractHash = await computeContractHash();
    const range = await computeDbSchemaRange();
    const manifest = {
      artifactPath: otherPath,
      sha256: digest,
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash,
      nativeApiVersion: null,
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: range,
    };
    await writeFile(manifestPathForArtifact(artifactPath), `${JSON.stringify(manifest, null, 2)}\n`);
    await assert.rejects(
      () => assertCompatibleBackend({ artifactPath, contractHash, protocolVersion: CURRENT_WRITER_PROTOCOL }),
      err => {
        assert.ok(err instanceof BackendCompatibilityError);
        expectRemediation(err);
        assert.match(err.message, /does not identify the requested artifact/);
        return true;
      },
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('refreshBackend preserves prior manifest when cargo build fails', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'refresh-failure-'));
  try {
    const targetDir = join(dir, 'target');
    const artifactPath = join(targetDir, 'debug', 'nexus42');
    const manifestPath = manifestPathForArtifact(artifactPath);
    await mkdir(dirname(artifactPath), { recursive: true });
    await writeFile(artifactPath, 'prior-artifact');
    const digest = await sha256File(artifactPath);
    const contractHash = await computeContractHash();
    const range = await computeDbSchemaRange();
    const manifest = {
      artifactPath,
      sha256: digest,
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash,
      nativeApiVersion: null,
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: range,
    };
    await writeManifestAtomic(manifestPath, manifest);
    const before = JSON.parse(await readFile(manifestPath, 'utf8'));

    await assert.rejects(
      () =>
        refreshBackend({
          profile: 'debug',
          targetDir,
          commandRunner: async () => {
            throw new Error('simulated cargo build failure');
          },
        }),
      /simulated cargo build failure/,
    );

    const after = JSON.parse(await readFile(manifestPath, 'utf8'));
    assert.deepEqual(after, before);
    assert.equal(await sha256File(artifactPath), digest);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test('resolveDaemonEndpoint rejects port and explicit URL conflicts', () => {
  assert.throws(
    () =>
      resolveDaemonEndpoint({
        portEnv: '18420',
        urlEnv: 'http://127.0.0.1:8420',
      }),
    /conflicts with VITE_DAEMON_URL port/,
  );
});

test('resolveDaemonEndpoint derives loopback URL from nondefault port', () => {
  const endpoint = resolveDaemonEndpoint({ portEnv: '18420' });
  assert.equal(endpoint.baseUrl, 'http://127.0.0.1:18420');
  assert.equal(endpoint.port, 18420);
});

test('resolveDaemonEndpoint materializes default port for URL-only host', () => {
  const endpoint = resolveDaemonEndpoint({ urlEnv: 'http://127.0.0.1' });
  assert.equal(endpoint.baseUrl, 'http://127.0.0.1:8420');
  assert.equal(endpoint.port, 8420);
});

test('resolveDaemonEndpoint rejects malformed port values', () => {
  assert.throws(() => resolveDaemonEndpoint({ portEnv: '18420junk' }), /Invalid NEXUS42_DAEMON_PORT/);
});

test('validateDaemonHealth rejects non-ok status responses', async () => {
  await assert.rejects(
    () =>
      validateDaemonHealth('http://127.0.0.1:1', {
        fetchImpl: async () => ({
          ok: true,
          text: async () => JSON.stringify({ status: 'bad', version: '0.1.0' }),
        }),
      }),
    /expected "ok"/,
  );
});

test('assertCompatibleRunningDaemon refuses incompatible running daemon with actionable guidance', async () => {
  const manifest = {
    packageVersion: '0.1.0',
  };
  await assert.rejects(
    () =>
      assertCompatibleRunningDaemon({
        baseUrl: 'http://127.0.0.1:19999',
        manifest,
        port: 19999,
        fetchImpl: async () => ({
          ok: true,
          text: async () => JSON.stringify({ status: 'bad', version: '9.9.9' }),
        }),
      }),
    err => {
      assert.ok(err instanceof RunningDaemonCompatibilityError);
      assert.match(err.message, /Stop it with: nexus42 daemon stop --port 19999/);
      assert.match(err.message, new RegExp(REMEDIATION_COMMAND.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
      return true;
    },
  );
});

test('isDaemonCliStatusRunning treats successful not-running status as not running', () => {
  const output = `Daemon Status:
  URL: http://127.0.0.1:8420
  Status: ✗ Not running

Start with: nexus42 daemon start`;
  assert.equal(isDaemonCliStatusRunning(output), false);
});

test('isDaemonCliStatusRunning treats reported running status as running', () => {
  const output = `Daemon Status:
  URL: http://127.0.0.1:8420
  Status: ✓ Running
  Version: "0.1.0"
  PID: 4242`;
  assert.equal(isDaemonCliStatusRunning(output), true);
});

test('assertDaemonHealthIdentityMatchesManifest refuses same-version stale contract hash', () => {
  const manifest = {
    contractHash: 'a'.repeat(64),
    writerProtocol: CURRENT_WRITER_PROTOCOL,
    dbSchemaRange: computeDbSchemaRangeFromNames(['20260417_000001_initial']),
    sha256: 'b'.repeat(64),
  };
  const reason = assertDaemonHealthIdentityMatchesManifest(
    {
      status: 'ok',
      version: '0.1.0',
      contractHash: 'c'.repeat(64),
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: manifest.dbSchemaRange,
    },
    manifest,
  );
  assert.match(reason, /contractHash/);
});

test('assertCompatibleRunningDaemon refuses same-version stale contract identity without killing', async () => {
  const manifest = {
    packageVersion: '0.1.0',
    contractHash: 'a'.repeat(64),
    writerProtocol: CURRENT_WRITER_PROTOCOL,
    dbSchemaRange: computeDbSchemaRangeFromNames(['20260417_000001_initial']),
    sha256: 'b'.repeat(64),
  };
  await assert.rejects(
    () =>
      assertCompatibleRunningDaemon({
        baseUrl: 'http://127.0.0.1:18888',
        manifest,
        port: 18888,
        fetchImpl: async () => ({
          ok: true,
          text: async () =>
            JSON.stringify({
              status: 'ok',
              version: manifest.packageVersion,
              contractHash: 'c'.repeat(64),
              writerProtocol: CURRENT_WRITER_PROTOCOL,
              dbSchemaRange: manifest.dbSchemaRange,
            }),
        }),
        execImpl: async () => {
          throw new Error('must not inspect or kill foreign daemon process');
        },
      }),
    err => {
      assert.ok(err instanceof RunningDaemonCompatibilityError);
      assert.match(err.message, /contractHash/);
      assert.match(err.message, /Stop it with: nexus42 daemon stop --port 18888/);
      assert.doesNotMatch(err.message, /must not inspect or kill foreign daemon process/);
      return true;
    },
  );
});
test('waitForDaemonHealth retries until daemon responds', async () => {
  let attempts = 0;
  const result = await waitForDaemonHealth('http://127.0.0.1:8420', {
    fetchImpl: async () => {
      attempts += 1;
      if (attempts < 3) {
        throw new Error('connect ECONNREFUSED');
      }
      return {
        ok: true,
        text: async () => JSON.stringify({ status: 'ok', version: '0.1.0' }),
      };
    },
    sleepImpl: async () => {},
    pollIntervalMs: 1,
    deadlineMs: 1000,
  });
  assert.equal(attempts, 3);
  assert.equal(result.health.version, '0.1.0');
});

