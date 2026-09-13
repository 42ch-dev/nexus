import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  assertCompatibility,
  expectedPlatformPackage,
  expectedTargetTriple,
  readPackageManifest,
} from '../dist/loader.js';

const expected = {
  target_triple: expectedTargetTriple(),
  package_version: '0.1.0',
};

const valid = {
  native_api_version: 1,
  writer_protocol: 1,
  target_triple: expected.target_triple,
  package_version: expected.package_version,
  contract_tree_sha256: 'a'.repeat(64),
  db_schema_min: 12,
  db_schema_max: 12,
  napi_minimum: 8,
};

describe('loader negative', () => {
  test('accepts a manifest matching runtime-derived expectations', () => {
    assertCompatibility(valid, expected);
  });

  test('rejects placeholder contract hash', () => {
    assert.throws(() =>
      assertCompatibility(
        { ...valid, contract_tree_sha256: '0'.repeat(64) },
        expected,
      ),
    );
  });

  test('rejects contract hash shape', () => {
    assert.throws(() =>
      assertCompatibility({ ...valid, contract_tree_sha256: 'not-a-hash' }, expected),
    );
  });

  test('rejects a wrong-target artifact', () => {
    assert.throws(() =>
      assertCompatibility({ ...valid, target_triple: 'x86_64-unknown-linux-gnu' }, expected),
    );
  });

  test('rejects package version mismatch against the platform package', () => {
    assert.throws(() =>
      assertCompatibility(valid, { ...expected, package_version: '9.9.9' }),
    );
  });

  test('rejects contract hash mismatch against the adjacent manifest', () => {
    assert.throws(() =>
      assertCompatibility(valid, { ...expected, contract_tree_sha256: 'b'.repeat(64) }),
    );
  });

  test('rejects low napi minimum requirement', () => {
    assert.throws(() => assertCompatibility({ ...valid, napi_minimum: 99 }, expected));
  });

  test('rejects an inverted db schema range', () => {
    assert.throws(() =>
      assertCompatibility({ ...valid, db_schema_min: 12, db_schema_max: 3 }, expected),
    );
  });

  test('runtime target triple is derived from this host, not the artifact', () => {
    const { platform, arch } = process;
    if (platform === 'darwin' && arch === 'arm64') {
      assert.equal(expectedTargetTriple(), 'aarch64-apple-darwin');
    } else if (platform === 'darwin' && arch === 'x64') {
      assert.equal(expectedTargetTriple(), 'x86_64-apple-darwin');
    } else if (platform === 'win32') {
      assert.equal(expectedTargetTriple(), 'x86_64-pc-windows-msvc');
    } else {
      assert.equal(expectedTargetTriple(), 'x86_64-unknown-linux-gnu');
    }
  });

  test('platform package declares required os/cpu (and linux libc) fields', () => {
    const target = expectedPlatformPackage();
    const manifest = readPackageManifest(target.name);
    assert.equal(manifest.version, '0.1.0');
    assert.ok(Array.isArray(manifest.os) && manifest.os.length > 0, 'os is required');
    assert.ok(manifest.os.includes(process.platform));
    assert.ok(Array.isArray(manifest.cpu) && manifest.cpu.length > 0, 'cpu is required');
    assert.ok(manifest.cpu.includes(process.arch));
    if (target.libc === 'glibc') {
      assert.ok(Array.isArray(manifest.libc) && manifest.libc.includes('gnu'), 'libc is required');
    }
  });
});
