import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const scriptsDir = dirname(fileURLToPath(import.meta.url));
const verifier = join(scriptsDir, '..', 'scripts', 'verify-package.mjs');

/**
 * Governed desktop product version from `resources/product.json`. The receipt
 * and the mocked Info.plist below both carry it, so the verifier's
 * `CFBundleShortVersionString === receipt.version` check stays anchored to the
 * real release version instead of a drifting literal.
 */
const PRODUCT_VERSION = (() => {
  const manifest = JSON.parse(
    readFileSync(new URL('../resources/product.json', import.meta.url), 'utf8'),
  );
  if (typeof manifest.version !== 'string' || manifest.version.length === 0) {
    throw new Error('desktop product.json: missing a string "version"');
  }
  return manifest.version;
})();

function writeExecutable(path, source) {
  writeFileSync(path, source);
  chmodSync(path, 0o755);
}

test('missing minimum-macOS load command fails package verification', () => {
  const root = mkdtempSync(join(tmpdir(), 'nexus-verify-package-'));
  try {
    const packageDir = join(root, 'darwin-arm64');
    const appPath = join(packageDir, 'Nexus.app', 'Contents');
    const commandBin = join(root, 'commands');
    mkdirSync(join(appPath, 'MacOS'), { recursive: true });
    mkdirSync(commandBin, { recursive: true });

    writeFileSync(join(packageDir, 'receipt.json'), JSON.stringify({
      schema_version: 1,
      product_name: 'Nexus',
      bundle_id: 'io.nexus42.desktop',
      version: PRODUCT_VERSION,
      git_revision: 'fixture',
      dirty: false,
      arch: 'arm64',
      platform: 'darwin',
      minimum_macos: '13.0',
      node_version: 'v22.0.0',
      pnpm_version: '10.0.0',
      electron_version: '44.4.5',
      packager_version: '20.3.0',
      native_contract_hash: 'a'.repeat(64),
      native_target: 'aarch64-apple-darwin',
      inputs: { app_file_manifest: [] },
      artifacts: {},
      signing_performed: false,
      notarization_performed: false,
      inherited_signature_metadata: {
        product_pipeline_signature: 'none',
        vendor_signature_preserved: true,
      },
      checks: {},
    }));
    writeFileSync(join(appPath, 'Info.plist'), 'fixture plist');
    writeFileSync(join(appPath, 'MacOS', 'Nexus'), 'missing-minos Mach-O fixture');

    // Keep the fixture independent of host Mach-O tools while preserving the
    // verifier's real CLI path: file reports Mach-O, otool reports no floor.
    writeExecutable(join(commandBin, 'file'), '#!/bin/sh\nprintf "%s\\n" "Mach-O 64-bit executable arm64"\n');
    writeExecutable(join(commandBin, 'otool'), '#!/bin/sh\nprintf "%s\\n" "Load command 0"\n');
    writeExecutable(join(commandBin, 'plutil'), `#!/bin/sh\nprintf "%s\\n" '{"CFBundleIdentifier":"io.nexus42.desktop","CFBundleName":"Nexus","CFBundleShortVersionString":"${PRODUCT_VERSION}"}'\n`);

    const result = spawnSync(process.execPath, [verifier, '--dir', packageDir], {
      encoding: 'utf8',
      env: { ...process.env, PATH: `${commandBin}:${process.env.PATH ?? ''}` },
    });

    assert.equal(result.status, 1);
    assert.match(result.stderr, /minimum macOS load command/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
