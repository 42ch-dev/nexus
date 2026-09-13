#!/usr/bin/env node
/**
 * P3-T2/T3 driver — validate a packaged Electron proof app and emit evidence JSON.
 * Signed/notarized verification is required for SEC-1; missing credentials encode as blocked.
 */
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appRoot = join(__dirname, '..');
const repoRoot = resolve(appRoot, '..', '..');
const bundleId = 'com.nexus42.rft-electron-proof';
const productName = 'Nexus RFT Feasibility';

function parseArgs(argv) {
  const out = { arch: null, signedRequired: false, out: null, appPath: null, home: process.env.NEXUS_PROOF_HOME ?? null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--arch') out.arch = argv[++i];
    else if (arg === '--signed-required') out.signedRequired = true;
    else if (arg === '--out') out.out = argv[++i];
    else if (arg === '--app') out.appPath = argv[++i];
    else if (arg === '--home') out.home = argv[++i];
    else if (arg === '--help' || arg === '-h') out.help = true;
  }
  return out;
}

function usage() {
  console.error(`Usage: node scripts/proof-package.mjs --arch arm64|x64 --out <dir> [--signed-required] [--app <path/to/App.app>] [--home <proof-home>]

Environment:
  NEXUS_PROOF_HOME  disposable seeded home (required unless --home)
  APPLE_SIGNING_IDENTITY / explicit --sign-identity at package time for signed builds`);
}

function defaultAppPath(arch, baseOut) {
  return join(baseOut, `${productName}-darwin-${arch}`, `${productName}.app`);
}

function run(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, { encoding: 'utf8', ...opts });
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  };
}

function verifyCodesign(appPath) {
  const deep = run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  const display = run('codesign', ['-dv', '--verbose=4', appPath]);
  return {
    deep_status: deep.status,
    deep_stdout: deep.stdout,
    deep_stderr: deep.stderr,
    display_status: display.status,
    display_stdout: display.stdout,
    display_stderr: display.stderr,
  };
}

function verifyGate(appPath) {
  const exe = join(appPath, 'Contents', 'MacOS', productName);
  return run('spctl', ['--assess', '--type', 'execute', '--verbose=4', exe]);
}

function verifyNotary(appPath) {
  return run('spctl', ['--assess', '--type', 'open', '--context', 'notary', '--verbose=4', appPath]);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help || !args.arch || !args.out) {
    usage();
    process.exit(args.help ? 0 : 1);
  }
  if (process.platform !== 'darwin') {
    throw new Error('Electron packaged proof requires a macOS runner');
  }

  const outDir = resolve(args.out);
  mkdirSync(outDir, { recursive: true });
  const packageDir = resolve(args.out, '..', 'electron-packages', args.arch);
  const appPath = resolve(args.appPath ?? defaultAppPath(args.arch, packageDir));

  const evidence = {
    status: 'blocked',
    bundle_id: bundleId,
    arch: args.arch,
    app_path: appPath,
    signed_required: args.signedRequired,
    checks: {},
    missing_inputs: [],
    note: 'P3-T2 scaffold — PM/P3-T3 Execute fills runtime samples after signed package exists.',
  };

  if (!existsSync(appPath)) {
    evidence.missing_inputs.push(`packaged app missing: ${appPath}`);
    evidence.checks.package_present = { pass: false, app_path: appPath };
    writeFileSync(join(outDir, 'proof-package.json'), `${JSON.stringify(evidence, null, 2)}\n`);
    console.error(`missing packaged app: ${appPath}`);
    process.exit(1);
  }
  evidence.checks.package_present = { pass: true, app_path: appPath };

  if (!args.home) {
    evidence.missing_inputs.push('NEXUS_PROOF_HOME (or --home) seeded disposable home');
  } else if (!existsSync(args.home)) {
    evidence.missing_inputs.push(`proof home missing: ${args.home}`);
  }

  if (args.signedRequired) {
    if (!process.env.APPLE_SIGNING_IDENTITY && !args.appPath) {
      evidence.missing_inputs.push('signed package input (package with --sign-identity or provide signed --app)');
    }
    evidence.checks.codesign = verifyCodesign(appPath);
    evidence.checks.spctl_execute = verifyGate(appPath);
    evidence.checks.notary = verifyNotary(appPath);
    const signedOk =
      evidence.checks.codesign.deep_status === 0 &&
      evidence.checks.spctl_execute.status === 0;
    evidence.checks.signed_execution = { pass: signedOk };
    if (!signedOk) {
      evidence.status = 'fail';
    }
  } else {
    evidence.checks.codesign = verifyCodesign(appPath);
    evidence.note =
      'Unsigned diagnostic run — does not satisfy SEC-1. Re-run with --signed-required after signing/notarization.';
  }

  if (evidence.missing_inputs.length > 0) {
    evidence.status = 'blocked';
  } else if (evidence.status !== 'fail') {
    evidence.status = args.signedRequired ? 'pass' : 'blocked';
  }

  writeFileSync(join(outDir, 'proof-package.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(JSON.stringify(evidence, null, 2));
  process.exit(evidence.status === 'pass' ? 0 : 1);
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
