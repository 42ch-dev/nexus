#!/usr/bin/env node
/**
 * P3-T3 decision generator.
 *
 * `electron-decision.json` is produced from the single authoritative row array
 * below; every count in the emitted document is derived from that array, so the
 * summary can never drift from the rows it summarizes (P3-T3 review I4).
 *
 * Usage: node apps/desktop-electron/scripts/proof-decision.mjs [--out <file>]
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');
const EVIDENCE = join(ROOT, '.mstar', 'iterations', 'v1.189', 'guides', 'evidence');

const EV = (relative) => `.mstar/iterations/v1.189/guides/evidence/${relative}`;

/**
 * The authoritative locked-row set. One entry per PKG/RES/START/SEC/MAINT row;
 * `verdict` is the observed outcome, never a target.
 */
const ROWS = [
  {
    row: 'PKG-1 darwin-arm64 native install/load — Node 22.22.0',
    verdict: 'PASS',
    raw_evidence: [EV('install-macarm22/install-proof.json'), EV('native-packages/darwin-arm64/package-receipt.json')],
    summary:
      'pass, 33/33 checks under compiler denial; real graph/create/update/provider/cancel/shutdown/close through the installed payload',
  },
  {
    row: 'PKG-1 darwin-arm64 native install/load — Node 24.20.0',
    verdict: 'PASS',
    raw_evidence: [EV('install-macarm24/install-proof.json')],
    summary: 'pass, 33/33 checks; observed NAPI 10; official nodejs.org runtime verified by sha256',
  },
  {
    row: 'PKG-1 darwin-x64 native install/load',
    verdict: 'MISSING',
    raw_evidence: [],
    summary: 'no native x64 macOS runner or artifact; Rosetta/ad-hoc substitution is prohibited as native proof',
  },
  {
    row: 'PKG-1 win32-x64-msvc native install/load',
    verdict: 'MISSING',
    raw_evidence: [],
    summary: 'no Windows x64 runner or artifact',
  },
  {
    row: 'PKG-1 linux-x64-gnu native install/load',
    verdict: 'MISSING',
    raw_evidence: [],
    summary: 'no Linux x64 glibc-2.28 runner or artifact',
  },
  {
    row: 'PKG-1 native binary inspection (darwin-arm64)',
    verdict: 'PASS',
    raw_evidence: [EV('native-binary-darwin-arm64/binary-inspection.json')],
    summary: 'mach-o/arm64, minos=11.0 from LC_BUILD_VERSION, no non-system load dependencies, bundled SQLite',
  },
  {
    row: 'PKG-2 native npm compressed payload <= 50 MiB/target',
    verdict: 'PASS',
    raw_evidence: [EV('native-packages/darwin-arm64/package-receipt.json')],
    summary: 'darwin-arm64 platform tarball 6.6 MiB (limit 50); other targets not built (see PKG-1 rows)',
  },
  {
    row: 'PKG-2 Electron .app zip <= 250 MiB / installed bundle <= 600 MiB per arch',
    verdict: 'PARTIAL',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json'), EV('electron-packages/arm64/package-receipt.json')],
    summary: 'arm64 measured zip 137.4 MiB and installed 326.6 MiB, both within limits; x64 not measured',
  },
  {
    row: 'START-1 cold/warm launch to interactive real graph',
    verdict: 'NOT OBSERVED',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json')],
    summary: 'readiness oracle implemented; the packaged run did not reach an interactive graph (see the runtime anomaly)',
  },
  {
    row: 'RES-1 idle/p95-active total-owned-process RSS',
    verdict: 'NOT OBSERVED',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json')],
    summary: '10-minute soak driver implemented; no sample reported because the packaged owner did not start',
  },
  {
    row: 'RES-2 100-cycle retained growth / surviving owned children',
    verdict: 'NOT OBSERVED',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json')],
    summary: 'cycle driver implemented; no meaningful measurement without a startable owner',
  },
  {
    row: 'SEC-1 signed/hardened/notarized/stapled arm64+x64 execution with native load',
    verdict: 'BLOCKED',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json'), EV('native-binary-darwin-arm64/binary-inspection.json')],
    summary:
      '0 valid code-signing identities and no notary credentials; codesign --verify --deep --strict fails on the unsigned bundle as expected',
  },
  {
    row: 'SEC-1 renderer sandbox/isolation/navigation guard',
    verdict: 'NOT OBSERVED',
    raw_evidence: [EV('electron-arm64/runtime-lifecycle.json')],
    summary: 'security phase implemented; sealed-renderer assertions were not reached in the packaged run',
  },
  {
    row: 'MAINT-1 Electron patch upgrade rebuild <= 30 min, one rebuild/arch',
    verdict: 'NOT OBSERVED',
    raw_evidence: [],
    summary: 'no patch-rebuild trace executed; patch releases in the pinned major exist but were not rebuilt',
  },
];

const MISSING_INPUTS = [
  {
    input: 'Apple Developer signing identity (Developer ID Application)',
    needed_for: 'SEC-1 and any signed arm64 package',
    observed_state: '0 valid code-signing identities in the login keychain; APPLE_SIGNING_IDENTITY unset',
    external_action: 'Install the Developer ID Application certificate and key on the proof host and re-package with --sign-identity.',
  },
  {
    input: 'Notarization credentials and a notarytool keychain profile',
    needed_for: 'SEC-1 staple/notary predicates',
    observed_state:
      'APPLE_ID / APPLE_APP_SPECIFIC_PASSWORD / APPLE_TEAM_ID / APPLE_API_KEY / APPLE_API_KEY_ID / APPLE_APP_ISSUER / NOTARY_KEYCHAIN_PROFILE unset; no notarytool profile',
    external_action: 'Provide notarization credentials, notarize+staple during packaging, then re-run the gate.',
  },
  {
    input: 'Native x86_64 macOS runner with Xcode 16.4',
    needed_for: 'PKG-1 darwin-x64, PKG-2 x64 size, SEC-1 x64 execution',
    observed_state: 'host is arm64; rustup has only aarch64-apple-darwin; no x64 artifact exists',
    external_action: 'Provide a real Intel macOS runner (CI row is pinned to macos-15-intel).',
  },
  {
    input: 'Windows x64 and Linux x64 GNU runners',
    needed_for: 'PKG-1 win32-x64-msvc and linux-x64-gnu rows',
    observed_state: 'no such host available; no artifacts present',
    external_action: 'Run .github/workflows/rft-native-proof.yml on its pinned runners.',
  },
  {
    input: 'A valid LaunchServices launch of the packaged app',
    needed_for: 'Attributing the packaged runtime anomaly and obtaining usable START/RES/SEC measurements',
    observed_state:
      'the only packaged run used direct executable launch, which is recorded as a confounder, so its result is not a product result',
    external_action: 'Re-run the runtime driver with --launch-method launchservices on a signed build.',
  },
];

/**
 * Execution defects found while running the proof. Kept here so the decision and
 * the report cannot disagree about how many were found (review I4).
 */
const RUNTIME_FIXES = [
  ['P3-T1 entity id convention', 'packages/nexus-native/scripts/proof-install.mjs', 'consumer minted kb_install_proof, violating the core-enforced kb_<hex> convention'],
  ['P3-T1 graph projection field', 'packages/nexus-native/scripts/proof-install.mjs', 'read version via entity_id, but the graph projection emits key_block_id'],
  ['P3-T1 install pack idempotency', 'packages/nexus-native/scripts/proof-install.mjs', 'pnpm pack overwrites a same-named tarball, so a re-run saw no new tarball'],
  ['P3-T1 package artifact load', 'packages/nexus-native/scripts/package.mjs', 'require() of the cargo .dylib parsed it as JavaScript'],
  ['P3-T1 package pack idempotency', 'packages/nexus-native/scripts/package.mjs', 'same overwrite cause as the install rows'],
  ['P3-T1 packer-injected LICENSE', 'packages/nexus-native/scripts/package.mjs', 'frozen-payload assertion rejected the byte-identical LICENSE pnpm injects'],
  ['P3-T1 inspector install name', 'packages/nexus-native/scripts/inspect-binary.mjs', 'otool -L lists the artifact own LC_ID_DYLIB first, failing the system-libraries check'],
  ['P3-T2 platform package resolution', 'apps/desktop-electron/src/env.ts', 'assertNativePayloadPresent resolved from the app dir; the package is an optional dependency of the loader'],
  ['P3-T2 packager import and staging', 'apps/desktop-electron/scripts/package.mjs', 'no default export in packager 20.3.0; packDependency bound the wrong tarball; pnpm staging fetched from the registry'],
  ['P3-T2 preload module format', 'apps/desktop-electron/{tsconfig.json,tsconfig.preload.json,package.json}', 'sandbox:true forces classic CJS preload, so the ESM preload never ran'],
  ['P3-T3 proof-runtime asar/size', 'apps/desktop-electron/scripts/proof-runtime.mjs', 'asar header was read at the wrong offset (vacuous no-native-in-asar check) and the size walk followed bundle symlinks (~2.7x inflation)'],
];

function countBy(list, key) {
  return list.reduce((acc, entry) => {
    acc[entry[key]] = (acc[entry[key]] ?? 0) + 1;
    return acc;
  }, {});
}

function sourceIdentity() {
  const sha = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).stdout.trim();
  const porcelain = spawnSync('git', ['status', '--porcelain'], { cwd: ROOT, encoding: 'utf8' }).stdout;
  const diff = spawnSync('git', ['diff', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).stdout;
  return {
    source_sha: sha || 'unknown',
    tree_digest: createHash('sha256').update(`${sha}\0${porcelain}\0${diff}`).digest('hex'),
    tree_dirty: porcelain.trim().length > 0,
  };
}

function evidenceDigests() {
  const digests = {};
  for (const row of ROWS) {
    for (const relative of row.raw_evidence) {
      const absolute = join(ROOT, relative);
      if (existsSync(absolute)) {
        digests[relative] = { sha256: createHash('sha256').update(readFileSync(absolute)).digest('hex') };
      } else {
        digests[relative] = { missing: true };
      }
    }
  }
  return digests;
}

function main() {
  const args = process.argv.slice(2);
  const outPath = args.includes('--out')
    ? resolve(args[args.indexOf('--out') + 1])
    : join(EVIDENCE, 'electron-decision.json');

  const counts = countBy(ROWS, 'verdict');
  const passRows = counts.PASS ?? 0;
  const measuredFailures = counts.FAIL ?? 0;
  // Everything that is neither a pass nor a measured failure is missing input.
  const missingRows = ROWS.length - passRows - measuredFailures;
  const allRequiredRowsPass = ROWS.every((row) => row.verdict === 'PASS');

  // Decision rule (plan P3-T3 / proof-matrix §6): GO only when every locked row
  // passes; a measured failure is no-go; otherwise missing input blocks.
  let status = 'blocked';
  if (allRequiredRowsPass) status = 'go';
  else if (measuredFailures > 0) status = 'no-go';

  const decision = {
    schema: 'rft-p3-t3-electron-decision/v2',
    plan_id: '2026-09-12-v1.189-p3-native-electron-feasibility',
    task: 'P3-T3',
    owner: 'ops-engineer',
    authority: 'plan §P3-T3 + proof-matrix §2/§6 (guides/evidence/electron-decision.json)',
    generated_by: 'apps/desktop-electron/scripts/proof-decision.mjs',
    status,
    status_rule:
      'GO requires every locked row to pass (signed dual-architecture execution, all native matrix rows, all ' +
      'resource/startup/security/maintenance thresholds). A valid unconfounded measured failure = no-go. ' +
      'Anything else, including any missing input, = blocked. Counts below are derived from the row array.',
    accepted_macos_floor: {
      reference: 'architecture-contracts §10 U1 (user, 2026-09-13)',
      floor: 'macOS 13+ (Ventura) on arm64 and x86_64',
      note: 'Reference only; no signed execution on that floor was performed.',
    },
    ...sourceIdentity(),
    evidence_root: '.mstar/iterations/v1.189/guides/evidence',
    row_counts: {
      total: ROWS.length,
      pass: passRows,
      fail: measuredFailures,
      missing_or_unobserved: missingRows,
      by_verdict: counts,
    },
    unobserved_rows: ROWS.filter((row) => row.verdict !== 'PASS').map((row) => ({
      row: row.row,
      verdict: row.verdict,
      summary: row.summary,
    })),
    observed_rows: ROWS,
    missing_inputs: MISSING_INPUTS,
    runtime_defects_found_and_fixed_during_proof: RUNTIME_FIXES.map(([id, file, detail]) => ({
      id,
      file,
      detail,
    })),
    evidence_digests: evidenceDigests(),
    // The arm64 runtime document on disk was produced before the v2 evidence
    // contract existed. It is a diagnostic, not canonical evidence: the gate
    // rejects it as `malformed` (wrong schema), which is the intended behaviour.
    superseded_evidence: [
      {
        path: EV('electron-arm64/runtime-lifecycle.json'),
        reason:
          'produced by the pre-contract driver and from a `direct` executable launch; the v2 contract rejects it as ' +
          'malformed, and its measurements are additionally confounded. Retained for archaeology only.',
        replaced_by: 'a canonical run of apps/desktop-electron/scripts/proof-runtime.mjs with --launch-method launchservices',
      },
    ],
    consequences: {
      m1_status: 'NOT complete; P3 cannot GO.',
      m2: 'Blocked. Per plan STOP/DoD, dependent M2 must not proceed on this record.',
      tauri_default: 'Unchanged. The current Tauri host remains the shipped desktop path.',
      npm_publish: 'Not authorized and not performed.',
    },
    next_external_actions: MISSING_INPUTS.map((entry) => entry.external_action),
    honesty_notes: [
      'No signed, notarized, stapled, x64, Windows or Linux result is claimed.',
      'Ad-hoc native dylib signing is a development signature and is never SEC-1 evidence.',
      'The unsigned bundle codesign --verify --deep --strict failure is reported verbatim.',
      'Rows that were not measured are recorded as NOT OBSERVED with the blocking reason.',
    ],
  };

  writeFileSync(outPath, `${JSON.stringify(decision, null, 2)}\n`);
  console.log(
    JSON.stringify({ outPath, status, row_counts: decision.row_counts, integrity: sourceIdentity() }, null, 2),
  );
}

main();
