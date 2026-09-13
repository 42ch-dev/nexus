import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  assertCompatibility,
  expectedPlatformPackage,
  readPackageManifest,
} from '../dist/loader.js';

const valid = {
  native_api_version: 1,
  writer_protocol: 1,
  target_triple: 'aarch64-unknown-macos',
  package_version: '0.1.0',
  contract_tree_sha256: 'a'.repeat(64),
  db_schema_min: 12,
  db_schema_max: 12,
  napi_minimum: 8,
};

describe('loader negative', () => {
  test('rejects placeholder contract hash', () => {
    assert.throws(() =>
      assertCompatibility({
        ...valid,
        contract_tree_sha256: '0000000000000000000000000000000000000000000000000000000000000000',
      }),
    );
  });

  test('rejects contract hash mismatch', () => {
    assert.throws(() =>
      assertCompatibility(valid, {
        ...valid,
        contract_tree_sha256: 'b'.repeat(64),
      }),
    );
  });

  test('rejects package version mismatch', () => {
    assert.throws(() => assertCompatibility(valid, valid, '9.9.9'));
  });

  test('rejects low napi minimum requirement', () => {
    assert.throws(() => assertCompatibility({ ...valid, napi_minimum: 99 }));
  });

  test('platform manifest matches workspace package', () => {
    const expected = expectedPlatformPackage();
    const manifest = readPackageManifest(expected.name);
    assert.equal(manifest.version, '0.1.0');
    assert.ok(manifest.os?.includes(process.platform));
    assert.ok(manifest.cpu?.includes(process.arch));
  });
});
