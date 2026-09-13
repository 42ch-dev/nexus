#!/usr/bin/env node
/**
 * P3-T2/T3 package + decision gate.
 *
 * Verifies a packaged Electron proof app against the SEC-1 signature predicates
 * and binds the runtime evidence to *this* artifact before any decision is
 * rendered. The output `status` uses the locked decision vocabulary exactly:
 *
 *   go      — signed + stapled + hardened, and complete provenance-bound runtime
 *             evidence passed every required row
 *   no-go   — a valid, unconfounded runtime document recorded a measured failure
 *   blocked — anything else: absent/malformed/incomplete/stale/confounded
 *             evidence, or missing signing/notary inputs
 *
 * Missing input is never a measured failure and a measured failure is never a
 * missing input (P3-T3 review I1).
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { NATIVE_LOAD_CHECK_ID, decisionForState, evaluateRuntimeEvidence } from './proof-contract.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const appRoot = join(__dirname, '..');
const repoRoot = resolve(appRoot, '..', '..');
const bundleId = 'com.nexus42.rft-electron-proof';
const productName = 'Nexus RFT Feasibility';
const expectedEntitlementsFile = join(appRoot, 'resources', 'entitlements.plist');

function parseArgs(argv) {
  const out = {
    arch: null,
    signedRequired: false,
    out: null,
    appPath: null,
    home: process.env.NEXUS_PROOF_HOME ?? null,
  };
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

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function verifyCodesign(appPath) {
  const deep = run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  const display = run('codesign', ['-dv', '--verbose=4', appPath]);
  const combined = `${display.stdout}\n${display.stderr}`;
  const flags = /flags=0x[0-9a-f]+\(([^)]*)\)/i.exec(combined);
  // Gatekeeper's execute assessment on the real executable, not the bundle dir.
  return {
    deep_status: deep.status,
    deep_stdout: deep.stdout,
    deep_stderr: deep.stderr,
    display_status: display.status,
    display_stdout: display.stdout,
    display_stderr: display.stderr,
    identifier: /Identifier=([^\n]+)/.exec(combined)?.[1] ?? null,
    team_identifier: /TeamIdentifier=([^\n]+)/.exec(combined)?.[1] ?? null,
    signature: /Signature=([^\n]+)/.exec(combined)?.[1] ?? null,
    flags: flags?.[1] ?? null,
    hardened_runtime: Boolean(flags?.[1]?.includes('runtime')),
    authority: (combined.match(/Authority=([^\n]+)/g) ?? []).map((line) => line.replace('Authority=', '')),
  };
}

function verifyGate(appPath) {
  const exe = join(appPath, 'Contents', 'MacOS', productName);
  return run('spctl', ['--assess', '--type', 'execute', '--verbose=4', exe]);
}

function verifyNotary(appPath) {
  return run('spctl', ['--assess', '--type', 'open', '--context', 'notary', '--verbose=4', appPath]);
}

/** Explicit staple predicate required by SEC-1, separate from the notary assessment. */
function verifyStaple(appPath) {
  return run('xcrun', ['stapler', 'validate', '-v', appPath]);
}

function readSignedEntitlements(appPath) {
  const res = run('codesign', ['-d', '--entitlements', ':-', appPath]);
  const text = `${res.stdout}${res.stderr}`;
  const plistStart = text.indexOf('<?xml');
  if (plistStart < 0) {
    return { status: res.status, raw: text.slice(-2000), parsed: null, parse_error: 'no embedded plist' };
  }
  const tmp = join('/tmp', `nexus-entitlements-${process.pid}.plist`);
  writeFileSync(tmp, text.slice(plistStart));
  const json = run('plutil', ['-convert', 'json', '-o', '-', tmp]);
  try {
    return { status: res.status, raw: text.slice(-2000), parsed: JSON.parse(json.stdout), plist_sha256: sha256File(tmp) };
  } catch {
    return { status: res.status, raw: text.slice(-2000), parsed: null, parse_error: json.stderr };
  }
}

function readExpectedEntitlements() {
  if (!existsSync(expectedEntitlementsFile)) return { path: expectedEntitlementsFile, parsed: null };
  const json = run('plutil', ['-convert', 'json', '-o', '-', expectedEntitlementsFile]);
  try {
    return {
      path: expectedEntitlementsFile,
      parsed: JSON.parse(json.stdout),
      source_sha256: sha256File(expectedEntitlementsFile),
    };
  } catch {
    return {
      path: expectedEntitlementsFile,
      parsed: null,
      source_sha256: sha256File(expectedEntitlementsFile),
      parse_error: json.stderr,
    };
  }
}

/**
 * Canonical form of an entitlement plist: keys sorted recursively so two
 * documents with identical content compare equal regardless of ordering.
 */
function canonicaliseEntitlements(value) {
  if (Array.isArray(value)) return value.map(canonicaliseEntitlements);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicaliseEntitlements(value[key])]),
    );
  }
  return value;
}

/**
 * Compare the signed entitlements to the documented narrow set *including
 * values*. Matching key names alone would accept a signed app whose booleans
 * changed (P3-T3 review I6); a parse or codesign failure is rejected outright.
 */
function compareEntitlements(signed, expected) {
  const problems = [];
  const expectedParsed = expected.parsed;
  if (!expectedParsed || Object.keys(expectedParsed).length === 0) {
    problems.push('expected entitlements plist is missing or unparseable');
  }
  if (signed.status !== 0) problems.push(`codesign -d --entitlements exited ${signed.status}`);
  if (!signed.parsed) problems.push(`signed entitlements unparseable: ${signed.parse_error ?? 'unknown'}`);
  let values_match = null;
  let keys_match = null;
  if (expectedParsed && signed.parsed) {
    const expectedKeys = Object.keys(expectedParsed).sort();
    const signedKeys = Object.keys(signed.parsed).sort();
    keys_match = JSON.stringify(expectedKeys) === JSON.stringify(signedKeys);
    values_match =
      JSON.stringify(canonicaliseEntitlements(signed.parsed)) ===
      JSON.stringify(canonicaliseEntitlements(expectedParsed));
    if (!keys_match) problems.push(`entitlement key sets differ: signed=[${signedKeys.join(',')}] expected=[${expectedKeys.join(',')}]`);
    if (!values_match) problems.push('signed entitlement values differ from the documented narrow set');
  }
  return {
    pass: problems.length === 0 && values_match === true,
    keys_match,
    values_match,
    problems,
    signed: signed.parsed,
    expected: expectedParsed,
    expected_source: { path: expected.path, sha256: expected.source_sha256 },
    signed_entitlements_plist_sha256: signed.plist_sha256 ?? null,
    signed_raw_tail: signed.raw,
  };
}

function readBundleIdentifier(appPath) {
  return verifyCodesign(appPath).identifier;
}

function runtimeEvidencePath(outDir) {
  return join(outDir, 'runtime-lifecycle.json');
}

function decide({ contractState, signedOk, stapleOk, hardenedOk, entitlementsOk, nativeLoadOk }) {
  let decision = decisionForState(contractState, {
    signedOk: signedOk && entitlementsOk && nativeLoadOk,
    stapleOk,
    hardenedOk,
  });
  if (decision === 'no-go' && (!entitlementsOk || !nativeLoadOk)) {
    // A valid runtime failure still stands on its own; signature/entitlement
    // gaps do not downgrade a measured product failure into a block.
    decision = 'no-go';
  }
  return decision;
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
  const requestedAppPath = resolve(args.appPath ?? defaultAppPath(args.arch, packageDir));
  const appPath = existsSync(requestedAppPath) ? realpathSync(requestedAppPath) : requestedAppPath;

  const evidence = {
    schema: 'rft-p3-t3-package-gate/v2',
    status: 'blocked',
    bundle_id: bundleId,
    arch: args.arch,
    app_path: requestedAppPath,
    app_realpath: appPath,
    signed_required: args.signedRequired,
    checks: {},
    missing_inputs: [],
    reasons: [],
    note:
      'Decision vocabulary is go|no-go|blocked. Missing/absent/malformed/stale/confounded evidence blocks; ' +
      'a valid unconfounded measured failure is no-go; go additionally requires signed+stapled+hardened ' +
      'execution with a provenance-bound successful native utility load.',
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

  // --- signature / staple / hardened-runtime predicates ----------------------
  const codesign = verifyCodesign(appPath);
  const entitlements = readSignedEntitlements(appPath);
  const expectedEntitlements = readExpectedEntitlements();
  const entitlementComparison = compareEntitlements(entitlements, expectedEntitlements);
  const entitlementsOk = entitlementComparison.pass === true;

  if (args.signedRequired) {
    if (!process.env.APPLE_SIGNING_IDENTITY && !args.appPath) {
      evidence.missing_inputs.push('signed package input (package with --sign-identity or provide signed --app)');
    }
    evidence.checks.codesign = codesign;
    evidence.checks.spctl_execute = verifyGate(appPath);
    evidence.checks.notary = verifyNotary(appPath);
    evidence.checks.stapler_validate = verifyStaple(appPath);
    evidence.checks.entitlements = entitlementComparison;
    evidence.checks.bundle_identifier = { expected: bundleId, actual: readBundleIdentifier(appPath) };

    const failedSignatureInputs = [];
    if (codesign.deep_status !== 0) failedSignatureInputs.push('codesign --verify --deep --strict');
    if (evidence.checks.spctl_execute.status !== 0) failedSignatureInputs.push('spctl --type execute');
    if (evidence.checks.notary.status !== 0) failedSignatureInputs.push('spctl --context notary');
    if (evidence.checks.stapler_validate.status !== 0) failedSignatureInputs.push('stapler validate');
    if (evidence.checks.bundle_identifier.actual !== bundleId) failedSignatureInputs.push('bundle identifier mismatch');
    if (!codesign.hardened_runtime) failedSignatureInputs.push('hardened runtime flag absent');
    if (!entitlementsOk) {
      failedSignatureInputs.push(
        `signed entitlements do not match the documented narrow set: ${entitlementComparison.problems.join('; ') || 'value mismatch'}`,
      );
    }

    evidence.checks.signature_predicates = {
      pass: failedSignatureInputs.length === 0,
      failed: failedSignatureInputs,
      hardened_runtime_flags: codesign.flags,
      signature: codesign.signature,
      team_identifier: codesign.team_identifier,
      authority: codesign.authority,
    };
  } else {
    evidence.checks.codesign = codesign;
    evidence.checks.entitlements = entitlementComparison;
    evidence.checks.signature_predicates = {
      pass: false,
      failed: ['--signed-required not set'],
      note: 'Unsigned diagnostic run — does not satisfy SEC-1.',
    };
  }

  const signedOk = evidence.checks.signature_predicates?.pass === true;
  const stapleOk = evidence.checks.stapler_validate?.status === 0;
  const hardenedOk = codesign.hardened_runtime === true;

  // --- runtime evidence, bound to THIS artifact -----------------------------
  const runtimePath = runtimeEvidencePath(outDir);
  let runtimeDoc = null;
  if (existsSync(runtimePath)) {
    try {
      runtimeDoc = JSON.parse(readFileSync(runtimePath, 'utf8'));
    } catch (error) {
      runtimeDoc = null;
      evidence.checks.runtime_lifecycle = {
        parse_error: String(error),
        path: runtimePath,
      };
      evidence.missing_inputs.push(`runtime lifecycle evidence is malformed: ${runtimePath}`);
    }
  } else {
    evidence.missing_inputs.push(`runtime lifecycle evidence missing: ${runtimePath}`);
  }

  const runtimeExpectations = {
    app_path: appPath,
    app_bundle_id: bundleId,
    arch: args.arch,
    electron_version: readPinVersion('electron'),
    packager_version: readPinVersion('@electron/packager'),
  };
  const verdict = evaluateRuntimeEvidence(runtimeDoc, runtimeExpectations);
  evidence.checks.runtime_lifecycle = {
    path: runtimePath,
    present: Boolean(runtimeDoc),
    contract_state: verdict.state,
    contract_reasons: verdict.reasons,
    detail: verdict.detail,
    provenance: runtimeDoc?.provenance ?? null,
    phases_executed: runtimeDoc?.phases_executed ?? null,
    checks_summary: Array.isArray(runtimeDoc?.checks)
      ? runtimeDoc.checks.map((c) => ({ id: c.id, ok: c.ok === true }))
      : null,
    native_utility_load: runtimeDoc?.native_utility_load ?? null,
    bound_to: runtimeExpectations,
  };
  const nativeLoadOk = runtimeDoc?.native_utility_load?.ok === true;

  if (verdict.state === 'missing' || verdict.state === 'malformed') {
    evidence.missing_inputs.push(`runtime lifecycle evidence unusable (${verdict.state})`);
  } else if (verdict.state === 'confounded') {
    evidence.reasons.push(
      `runtime run is confounded, not a product result: ${verdict.reasons.join('; ')}. ` +
        'Re-run with --launch-method launchservices on a real signed build to obtain a usable measurement.',
    );
  } else if (verdict.state === 'stale') {
    evidence.missing_inputs.push(`runtime evidence does not belong to this artifact: ${verdict.reasons.join('; ')}`);
  } else if (verdict.state === 'incomplete') {
    evidence.missing_inputs.push(`runtime evidence incomplete: ${verdict.reasons.join('; ')}`);
  }

  // --- decision -------------------------------------------------------------
  evidence.status = decide({
    contractState: verdict.state,
    signedOk,
    stapleOk,
    hardenedOk,
    entitlementsOk,
    nativeLoadOk,
  });
  if (evidence.status === 'blocked' && evidence.reasons.length === 0 && evidence.missing_inputs.length > 0) {
    evidence.reasons.push(...evidence.missing_inputs);
  }
  evidence.decision_inputs = {
    runtime_contract_state: verdict.state,
    signature_predicates_pass: signedOk,
    stapler_ok: stapleOk,
    hardened_runtime: hardenedOk,
    entitlements_match: entitlementsOk,
    native_utility_load_proven: nativeLoadOk,
    native_load_check_id: NATIVE_LOAD_CHECK_ID,
    rules:
      'go = valid-pass runtime + signature predicates + staple + hardened + provenance-bound native load; ' +
      'no-go = valid unconfounded measured failure; blocked = everything else',
  };

  writeFileSync(join(outDir, 'proof-package.json'), `${JSON.stringify(evidence, null, 2)}\n`);
  console.log(
    JSON.stringify(
      {
        status: evidence.status,
        runtime_contract_state: verdict.state,
        missing_inputs: evidence.missing_inputs,
        reasons: evidence.reasons,
      },
      null,
      2,
    ),
  );
  process.exit(evidence.status === 'go' ? 0 : 1);
}

function readPinVersion(name) {
  try {
    const candidates = [
      join(appRoot, 'node_modules', name, 'package.json'),
      join(repoRoot, 'apps', 'desktop-electron', 'node_modules', name, 'package.json'),
    ];
    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        return JSON.parse(readFileSync(candidate, 'utf8')).version ?? null;
      }
    }
    return null;
  } catch {
    return null;
  }
}

function isDirectInvocation() {
  return process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url));
}

if (isDirectInvocation()) {
  main().catch((err) => {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  });
}

export { canonicaliseEntitlements, compareEntitlements };
