import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import {
  PACKAGE_CONTRACT,
  PackageContractError,
  assertDependencyClosure,
  assertNativeCompatibility,
  assertNoSigningEnvironment,
  assertNoSymlinkEscape,
  assertPreflightFiles,
  assertReceipt,
  parsePackageArgs,
} from '../scripts/package-contract.mjs';

const validNative = {
  native_api_version: 1,
  writer_protocol: 1,
  target_triple: 'aarch64-apple-darwin',
  package_version: '0.1.0',
  contract_tree_sha256: 'a'.repeat(64),
  db_schema_min: 1,
  db_schema_max: 1,
  napi_minimum: 8,
};

function throwsCode(fn, code) {
  assert.throws(fn, (error) => error instanceof PackageContractError && error.code === code);
}

test('package options are closed and default to the native architecture', () => {
  assert.deepEqual(parsePackageArgs([], 'arm64'), { arch: 'arm64', out: null, help: false });
  assert.deepEqual(parsePackageArgs(['--arch', 'x64', '--out', './dist'], 'arm64'), {
    arch: 'x64',
    out: './dist',
    help: false,
  });
  throwsCode(() => parsePackageArgs(['--signed-required'], 'arm64'), 'package.args.unknown');
  throwsCode(() => parsePackageArgs(['--release'], 'arm64'), 'package.args.unknown');
  throwsCode(() => parsePackageArgs(['--arch', 'arm64', '--arch', 'arm64'], 'arm64'), 'package.args.duplicate');
  throwsCode(() => parsePackageArgs(['--unknown'], 'arm64'), 'package.args.unknown');
});

test('missing web dist is rejected before staging with no partial output', () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-package-preflight-'));
  const output = join(root, 'artifacts');
  let error;
  try {
    assertPreflightFiles([{
      path: join(root, 'web', 'index.html'),
      label: 'web dist',
      code: 'package.preflight.missing_web_dist',
      action: 'run pnpm run build:web',
    }]);
  } catch (caught) {
    error = caught;
  }
  assert.equal(error?.code, 'package.preflight.missing_web_dist');
  assert.match(error?.message ?? '', /run pnpm run build:web/);
  assert.equal(existsSync(output), false);
});

test('missing dependency closure is rejected with frozen-install guidance and no partial output', () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-package-dependencies-'));
  const output = join(root, 'artifacts');
  const lockfile = join(root, 'pnpm-lock.yaml');
  const virtualStore = join(root, 'node_modules', '.pnpm');
  writeFileSync(lockfile, 'lockfileVersion: 9.0\n');
  mkdirSync(join(root, 'node_modules'), { recursive: true });
  let error;
  try {
    assertDependencyClosure({
      lockfile,
      virtualStore,
      workspaceRoots: [join(root, 'apps', 'desktop-electron', 'node_modules')],
    });
  } catch (caught) {
    error = caught;
  }
  assert.equal(error?.code, 'package.preflight.missing_dependency_closure');
  assert.match(error?.message ?? '', /pnpm install --frozen-lockfile/);
  assert.equal(existsSync(output), false);
});

test('credential-triggered environments are rejected before packaging effects', () => {
  assert.doesNotThrow(() => assertNoSigningEnvironment({ PATH: '/usr/bin', HOME: '/tmp' }));
  throwsCode(() => assertNoSigningEnvironment({ PATH: '/usr/bin', APPLE_SIGNING_IDENTITY: 'unexpected' }), 'package.unsigned.environment');
});

test('native contract requires compatibility metadata and reset binding', () => {
  assert.equal(assertNativeCompatibility(validNative, { arch: 'arm64' }).target_triple, 'aarch64-apple-darwin');
  assert.doesNotThrow(() => assertNativeCompatibility(validNative, { arch: 'arm64', resetBinding: true }));
  throwsCode(() => assertNativeCompatibility({ ...validNative, target_triple: 'x86_64-unknown-linux-gnu' }, { arch: 'arm64' }), 'package.native.arch');
  throwsCode(() => assertNativeCompatibility(validNative, { arch: 'arm64', resetBinding: false }), 'package.native.reset');
  throwsCode(() => assertNativeCompatibility({ ...validNative, contract_tree_sha256: '0'.repeat(64) }, { arch: 'arm64' }), 'package.native.compatibility');
});

test('staged inputs reject symlink escape but allow links inside staging root', () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-package-contract-'));
  mkdirSync(join(root, 'inside'));
  writeFileSync(join(root, 'inside', 'input.txt'), 'ok');
  symlinkSync(join(root, 'inside', 'input.txt'), join(root, 'inside-link'));
  assert.doesNotThrow(() => assertNoSymlinkEscape(root));
  symlinkSync('/tmp', join(root, 'escape'));
  throwsCode(() => assertNoSymlinkEscape(root), 'package.staging.symlink');
});

test('receipt is the closed unsigned version-1 shape', () => {
  const receipt = {
    schema_version: 1,
    product_name: PACKAGE_CONTRACT.productName,
    bundle_id: PACKAGE_CONTRACT.bundleId,
    version: '0.1.0',
    git_revision: 'a'.repeat(40),
    dirty: false,
    arch: 'arm64',
    platform: 'darwin',
    minimum_macos: '13.0',
    node_version: '22.22.0',
    pnpm_version: '11.0.0',
    electron_version: PACKAGE_CONTRACT.electronVersion,
    packager_version: PACKAGE_CONTRACT.packagerVersion,
    native_contract_hash: 'b'.repeat(64),
    native_target: 'aarch64-apple-darwin',
    inputs: {},
    artifacts: {},
    signing_performed: false,
    notarization_performed: false,
    inherited_signature_metadata: {},
    checks: {},
  };
  assert.equal(assertReceipt(receipt), receipt);
  throwsCode(() => assertReceipt({ ...receipt, signing_performed: true }), 'package.receipt.unsigned');
  throwsCode(() => assertReceipt({ ...receipt, extra: true }), 'package.receipt.schema');
});

test('driver explicitly disables the packager integrity mutation and has no signing configuration', () => {
  const source = readFileSync(new URL('../scripts/package.mjs', import.meta.url), 'utf8');
  assert.match(source, /asarIntegrityDigest:\s*false/);
  assert.doesNotMatch(source, /\bosxSign\b|\bosxNotarize\b|APPLE_SIGNING_IDENTITY|entitlements/);
  assert.match(source, /signing_performed:\s*false/);
});
