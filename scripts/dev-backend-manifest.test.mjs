import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

import {
  REMEDIATION_COMMAND,
  BackendCompatibilityError,
  CURRENT_WRITER_PROTOCOL,
  assertCompatibleBackend,
  computeContractHash,
  computeDbSchemaRange,
  computeDbSchemaRangeFromNames,
  manifestPathForArtifact,
  readBackendManifest,
  resolveDaemonEndpoint,
  sha256File,
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

test('failed refresh leaves previous manifest when rewrite never commits', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'manifest-atomic-'));
  try {
    const manifestPath = join(dir, 'nexus42.manifest.json');
    const first = {
      artifactPath: join(dir, 'nexus42'),
      sha256: 'a'.repeat(64),
      targetTriple: 'aarch64-apple-darwin',
      packageVersion: '0.1.0',
      contractHash: 'b'.repeat(64),
      nativeApiVersion: null,
      writerProtocol: CURRENT_WRITER_PROTOCOL,
      dbSchemaRange: { min: '20260417_000001_initial', max: '20260417_000001_initial' },
    };
    await writeManifestAtomic(manifestPath, first);
    const before = JSON.parse(await readFile(manifestPath, 'utf8'));

    await assert.rejects(async () => {
      throw new Error('simulated cargo build failure');
      await writeManifestAtomic(manifestPath, { ...first, sha256: 'c'.repeat(64) });
    });

    const after = JSON.parse(await readFile(manifestPath, 'utf8'));
    assert.deepEqual(after, before);
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
