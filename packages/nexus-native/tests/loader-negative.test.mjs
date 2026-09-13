import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import {
  assertCompatibility,
  expectedPlatformPackage,
  expectedTargetTriple,
  fenceCompatibilityPair,
  readPackageManifest,
} from '../dist/loader.js';
import {
  CORE_CHANGES_REQUEST_SHAPE,
  CORE_HOST_QUERY_SHAPE,
  NATIVE_OPEN_OPTIONS_SHAPE,
  PROVIDER_CALL_SHAPE,
  REQUIRED_NAPI_MINIMUM,
  WORLD_KB_PATCH_ENTITY_SHAPE,
  encodeWireBuffer,
  stringifyWire,
} from '../dist/validate.js';

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

  test('rejects a manifest whose declared N-API minimum is not the contract value', () => {
    assert.throws(
      () => assertCompatibility({ ...valid, napi_minimum: REQUIRED_NAPI_MINIMUM + 1 }, expected),
      /napi_minimum must be the contract value/,
    );
  });

  test('rejects a DB range that disagrees with the adjacent manifest', () => {
    const adjacent = { ...valid };
    assert.throws(
      () =>
        fenceCompatibilityPair(
          { ...valid, db_schema_min: 12, db_schema_max: 12 },
          { ...adjacent, db_schema_min: 11, db_schema_max: 11 },
          expected,
        ),
      /db_schema_min mismatch/,
    );
  });

  test('rejects a DB max that disagrees with the adjacent manifest', () => {
    const adjacent = { ...valid };
    assert.throws(
      () =>
        fenceCompatibilityPair(
          { ...valid, db_schema_min: 12, db_schema_max: 13 },
          { ...adjacent, db_schema_min: 12, db_schema_max: 12 },
          expected,
        ),
      /db_schema_max mismatch/,
    );
  });

  test('rejects a hash that disagrees with the adjacent manifest', () => {
    assert.throws(
      () =>
        fenceCompatibilityPair(
          { ...valid, contract_tree_sha256: 'c'.repeat(64) },
          { ...valid, contract_tree_sha256: 'd'.repeat(64) },
          expected,
        ),
      /contract_tree_sha256 mismatch/,
    );
  });

  test('accepts a fully agreeing manifest pair', () => {
    fenceCompatibilityPair({ ...valid }, { ...valid }, expected);
  });
});

describe('facade wire validation', () => {
  test('rejects empty native open options', () => {
    assert.throws(() => stringifyWire({}, NATIVE_OPEN_OPTIONS_SHAPE, 'options'), /required/);
  });

  test('rejects unknown native open option fields', () => {
    assert.throws(
      () =>
        stringifyWire(
          { user_home: '/tmp/x', access: 'read_only', extra: 1 },
          NATIVE_OPEN_OPTIONS_SHAPE,
          'options',
        ),
      /unknown field/,
    );
  });

  test('rejects a bad access enum value', () => {
    assert.throws(
      () =>
        stringifyWire(
          { user_home: '/tmp/x', access: 'root' },
          NATIVE_OPEN_OPTIONS_SHAPE,
          'options',
        ),
      /allowed values/,
    );
  });

  test('preserves omission vs explicit null in options', () => {
    const omitted = JSON.parse(stringifyWire({ user_home: '/tmp/x', access: 'read_only' }, NATIVE_OPEN_OPTIONS_SHAPE, 'options'));
    assert.ok(!('allow_uninitialized' in omitted));
    const explicit = JSON.parse(stringifyWire({ user_home: '/tmp/x', access: 'read_only', allow_uninitialized: false }, NATIVE_OPEN_OPTIONS_SHAPE, 'options'));
    assert.equal(explicit.allow_uninitialized, false);
  });

  test('rejects a provider call without payload', () => {
    assert.throws(
      () =>
        stringifyWire(
          { method: 'probe', request_id: 'r', deadline_ms: 1000 },
          PROVIDER_CALL_SHAPE,
          'request',
        ),
      /payload.*required/,
    );
  });

  test('rejects a non-object provider payload', () => {
    assert.throws(
      () =>
        stringifyWire(
          { method: 'probe', request_id: 'r', deadline_ms: 1000, payload: 'nope' },
          PROVIDER_CALL_SHAPE,
          'request',
        ),
      /expected an object/,
    );
  });

  test('rejects an unknown provider method', () => {
    assert.throws(
      () =>
        stringifyWire(
          { method: 'delete', request_id: 'r', deadline_ms: 1000, payload: {} },
          PROVIDER_CALL_SHAPE,
          'request',
        ),
      /allowed values/,
    );
  });

  test('rejects a world patch with neither field', () => {
    assert.throws(
      () =>
        stringifyWire(
          { entity_id: 'kb_abc123', expected_version: 0, patch: {} },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /at least 1 property/,
    );
  });

  test('rejects an unknown nested patch field', () => {
    assert.throws(
      () =>
        stringifyWire(
          { entity_id: 'kb_abc123', expected_version: 0, patch: { nope: true } },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /unknown field/,
    );
  });

  test('rejects an invalid nested block_type', () => {
    assert.throws(
      () =>
        stringifyWire(
          { entity_id: 'kb_abc123', expected_version: 0, patch: { block_type: 'widget' } },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /allowed values/,
    );
  });

  test('rejects an out-of-range nested title length', () => {
    assert.throws(
      () =>
        stringifyWire(
          { entity_id: 'kb_abc123', expected_version: 0, patch: { title: '' } },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /minimum length/,
    );
  });

  test('rejects unsafe integers nested inside a patch', () => {
    assert.throws(
      () =>
        stringifyWire(
          { entity_id: 'kb_abc123', expected_version: 0, patch: { body: { revision: 9007199254740993 } } },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /exactly representable/,
    );
  });

  test('rejects a host query without a discriminator', () => {
    assert.throws(() => stringifyWire({}, CORE_HOST_QUERY_SHAPE, 'request'), /required/);
  });

  test('rejects an out-of-range changes limit', () => {
    assert.throws(
      () => stringifyWire({ after_sequence: '0', limit: 4096 }, CORE_CHANGES_REQUEST_SHAPE, 'request'),
      /above the maximum/,
    );
  });

  test('rejects a non-numeric-decimal after_sequence', () => {
    assert.throws(
      () => stringifyWire({ after_sequence: 'abc' }, CORE_CHANGES_REQUEST_SHAPE, 'request'),
      /pattern/,
    );
  });

  test('accepts a module with an object value', () => {
    const encoded = JSON.parse(
      stringifyWire(
        {
          entity_id: 'kb_abc123',
          expected_version: 0,
          patch: { modules: { mental: { belief: 1 } } },
        },
        WORLD_KB_PATCH_ENTITY_SHAPE,
        'request',
      ),
    );
    assert.deepEqual(encoded.patch.modules, { mental: { belief: 1 } });
  });

  test('accepts a module with an array value', () => {
    const encoded = JSON.parse(
      stringifyWire(
        {
          entity_id: 'kb_abc123',
          expected_version: 0,
          patch: { modules: { observation: [{ id: 'o1' }] } },
        },
        WORLD_KB_PATCH_ENTITY_SHAPE,
        'request',
      ),
    );
    assert.ok(Array.isArray(encoded.patch.modules.observation));
  });

  test('accepts a hyphen/underscore/digit module key', () => {
    assert.doesNotThrow(() =>
      stringifyWire(
        {
          entity_id: 'kb_abc123',
          expected_version: 0,
          patch: { modules: { 'l5-mind_state2': {} } },
        },
        WORLD_KB_PATCH_ENTITY_SHAPE,
        'request',
      ),
    );
  });

  test('rejects a module key that breaks the property-name pattern', () => {
    assert.throws(
      () =>
        stringifyWire(
          {
            entity_id: 'kb_abc123',
            expected_version: 0,
            patch: { modules: { 'Bad!': {} } },
          },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /key does not match the required pattern/,
    );
  });

  test('rejects a scalar module value', () => {
    assert.throws(
      () =>
        stringifyWire(
          {
            entity_id: 'kb_abc123',
            expected_version: 0,
            patch: { modules: { 'Bad!': 'scalar' } },
          },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /key does not match the required pattern/,
    );
    assert.throws(
      () =>
        stringifyWire(
          {
            entity_id: 'kb_abc123',
            expected_version: 0,
            patch: { modules: { mental: 'scalar' } },
          },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /must be an object or an array/,
    );
  });

  test('rejects a null module value', () => {
    assert.throws(
      () =>
        stringifyWire(
          {
            entity_id: 'kb_abc123',
            expected_version: 0,
            patch: { modules: { mental: null } },
          },
          WORLD_KB_PATCH_ENTITY_SHAPE,
          'request',
        ),
      /must be an object or an array/,
    );
  });

  test('accepts and encodes a representative valid payload', () => {
    const buffer = encodeWireBuffer(
      { entity_id: 'kb_abc123', expected_version: 0, patch: { title: 'Wire Hero' } },
      WORLD_KB_PATCH_ENTITY_SHAPE,
      'request',
    );
    assert.ok(buffer instanceof Uint8Array && buffer.length > 0);
  });
});