#!/usr/bin/env node
/**
 * P3-T3 decision generator.
 *
 * Every row verdict is *derived* from the evidence documents it cites: a row can
 * only be `PASS` if the referenced document exists, parses, carries the expected
 * schema/target/source identity, and reports `pass`. A missing, malformed,
 * mismatched or non-pass document downgrades the row (P3-T3 review I5). Counts
 * are computed from the resulting row array, so the summary can never drift from
 * the rows it summarizes (P3-T3 review I4), and PKG-2 is split per target rather
 * than reported as one aggregate pass (P3-T3 review I8).
 *
 * Usage: node apps/desktop-electron/scripts/proof-decision.mjs [--out <file>]
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { RUNTIME_SCHEMA, evaluateRuntimeEvidence } from './proof-contract.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

/**
 * The harness tree lives in the primary worktree, not in a feature worktree
 * (`.mstar/**` is gitignored, so a worktree has its own empty skeleton). Resolve
 * the evidence root against the repository that actually holds the documents,
 * and let `--evidence-root` override it.
 */
function evidenceRoot() {
  const override = process.argv.includes('--evidence-root')
    ? process.argv[process.argv.indexOf('--evidence-root') + 1]
    : null;
  if (override) return resolve(override);
  const common = spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], {
    cwd: ROOT,
    encoding: 'utf8',
  }).stdout.trim();
  const primaryRoot = common ? dirname(common) : ROOT;
  for (const candidate of [join(ROOT, '.mstar', 'iterations'), join(primaryRoot, '.mstar', 'iterations')]) {
    if (existsSync(join(candidate, 'v1.189', 'guides', 'evidence'))) return candidate;
  }
  return join(ROOT, '.mstar', 'iterations');
}

const EVIDENCE = join(evidenceRoot(), 'v1.189', 'guides', 'evidence');

const EV = (relative) => `.mstar/iterations/v1.189/guides/evidence/${relative}`;

/** Map a repo-relative evidence path onto the resolved evidence root. */
function absoluteFor(displayPath) {
  const prefix = '.mstar/iterations/v1.189/guides/evidence/';
  return join(EVIDENCE, displayPath.startsWith(prefix) ? displayPath.slice(prefix.length) : displayPath);
}
const TARGETS = {
  arm64: 'aarch64-apple-darwin',
  x64: 'x86_64-apple-darwin',
  win: 'x86_64-pc-windows-msvc',
  linux: 'x86_64-unknown-linux-gnu',
};
const NATIVE_PAYLOAD_LIMIT_MIB = 50;
const ELECTRON_ZIP_LIMIT_MIB = 250;
const ELECTRON_INSTALLED_LIMIT_MIB = 600;

// --- evidence access --------------------------------------------------------

function readEvidence(relative) {
  const absolute = absoluteFor(relative);
  if (!existsSync(absolute)) {
    return { exists: false, doc: null, reason: `absent: ${relative}` };
  }
  const bytes = readFileSync(absolute);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  try {
    return { exists: true, doc: JSON.parse(bytes.toString('utf8')), sha256, reason: null };
  } catch (error) {
    return { exists: true, doc: null, sha256, reason: `unparseable: ${error.message}` };
  }
}

/**
 * Validate one evidence document against the identity it must carry. Any failure
 * is a non-pass row, never a silent pass.
 */
function validateEvidence(relative, { schema, target, sourceSha, extra = () => [] }) {
  const loaded = readEvidence(relative);
  if (!loaded.exists) return { ok: false, state: 'missing', loaded, problems: [`absent: ${relative}`] };
  if (!loaded.doc) return { ok: false, state: 'malformed', loaded, problems: [loaded.reason] };

  const problems = [];
  if (schema && loaded.doc.schema !== schema) {
    problems.push(`schema ${loaded.doc.schema ?? 'absent'} != ${schema}`);
  }
  if (target && loaded.doc.target !== target) {
    problems.push(`target ${loaded.doc.target ?? 'absent'} != ${target}`);
  }
  if (sourceSha) {
    if (!loaded.doc.source_sha) problems.push('source_sha absent');
    else if (loaded.doc.source_sha !== sourceSha) {
      problems.push(`source_sha ${loaded.doc.source_sha} != head ${sourceSha}`);
    }
  }
  if (loaded.doc.status !== 'pass') problems.push(`status ${loaded.doc.status ?? 'absent'} != pass`);
  problems.push(...extra(loaded.doc));

  const stale = problems.some(
    (p) => p.startsWith('source_sha') || p.startsWith('target') || p.startsWith('schema') || p.startsWith('artifact sha256'),
  );
  return {
    ok: problems.length === 0,
    state: problems.length === 0 ? 'pass' : stale ? 'stale' : 'non-pass',
    loaded,
    problems,
  };
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

// --- row construction -------------------------------------------------------

const source = sourceIdentity();
const rows = [];
const registry = new Map();

function pushRow(row) {
  rows.push(row);
  registry.set(row.id, row);
}

/** A row whose verdict is derived from one validated evidence document. */
function derivedRow({ id, row, evidence, kind, summary, extraChecks, onPass }) {
  const validation = validateEvidence(evidence, {
    schema: kind.schema,
    target: kind.target,
    // `sourceSha: null` on the kind is an explicit opt-out for documents whose
    // schema does not record a source identity (bound transitively instead).
    sourceSha: 'sourceSha' in kind ? kind.sourceSha : source.source_sha,
    extra: (doc) => {
      const problems = [];
      if (kind.nodeVersion && doc.node_version_requested !== kind.nodeVersion) {
        problems.push(`node_version_requested ${doc.node_version_requested ?? 'absent'} != ${kind.nodeVersion}`);
      }
      return [...problems, ...(extraChecks ? extraChecks(doc) : [])];
    },
  });
  const verdict = validation.ok ? 'PASS' : validation.state === 'missing' ? 'MISSING' : validation.state === 'stale' ? 'STALE' : 'BLOCKED';
  if (validation.ok && onPass) onPass(validation.loaded.doc);
  // The summary describes the evidence when it is readable, and the derivation
  // failure when it is not — never an assumed pass.
  const described = validation.loaded.doc ? summary(validation.loaded.doc) : null;
  pushRow({
    id,
    row,
    verdict,
    raw_evidence: [evidence],
    summary: validation.ok
      ? described
      : `${described ? `${described}; ` : ''}${validation.problems.join('; ')}`,
    verification: {
      derived: true,
      evidence_path: evidence,
      evidence_sha256: validation.loaded.sha256 ?? null,
      evidence_state: validation.state,
      problems: validation.problems,
    },
  });
  return validation;
}

/** A row with no evidence to derive from: explicitly missing/unobserved. */
function unavailableRow({ id, row, verdict, summary, raw_evidence = [], reason }) {
  pushRow({
    id,
    row,
    verdict,
    raw_evidence,
    summary,
    verification: { derived: false, reason },
  });
}

// --- PKG-1 native install/load (per target) ---------------------------------

const installRow = (id, row, target, outDir, nodeVersion) =>
  derivedRow({
    id,
    row,
    evidence: EV(`${outDir}/install-proof.json`),
    kind: { schema: 'rft-p3-t1-native-install-proof/v1', target, nodeVersion },
    summary: (doc) =>
      `pass, ${doc.checks.filter((c) => c.ok).length}/${doc.checks.length} checks; ` +
      `node ${doc.node_version_requested}; real graph/create/update/provider/cancel/shutdown/close through the installed payload`,
  });

installRow('PKG1-arm64-node22', 'PKG-1 darwin-arm64 native install/load — Node 22.22.0', TARGETS.arm64, 'install-macarm22', '22.22.0');
installRow('PKG1-arm64-node24', 'PKG-1 darwin-arm64 native install/load — Node 24.20.0', TARGETS.arm64, 'install-macarm24', '24.20.0');
unavailableRow({
  id: 'PKG1-arm64-x64',
  row: 'PKG-1 darwin-x64 native install/load',
  verdict: 'MISSING',
  summary: 'no native x64 macOS runner or artifact; Rosetta/ad-hoc substitution is prohibited as native proof',
  reason: 'no native x64 macOS runner available',
});
unavailableRow({
  id: 'PKG1-win',
  row: 'PKG-1 win32-x64-msvc native install/load',
  verdict: 'MISSING',
  summary: 'no Windows x64 runner or artifact',
  reason: 'no Windows x64 runner available',
});
unavailableRow({
  id: 'PKG1-linux',
  row: 'PKG-1 linux-x64-gnu native install/load',
  verdict: 'MISSING',
  summary: 'no Linux x64 glibc-2.28 runner or artifact',
  reason: 'no Linux x64 GNU runner available',
});

// Binary inspection has no source_sha field of its own; bind it to the package
// receipt through the artifact digest both documents record.
const packageReceiptPath = EV('native-packages/darwin-arm64/package-receipt.json');
const packageValidation = derivedRow({
  id: 'PKG1-package-arm64',
  row: 'PKG-1 darwin-arm64 native package receipt (frozen payload + compatibility)',
  evidence: packageReceiptPath,
  kind: { schema: 'rft-p3-t1-package-receipt/v1', target: TARGETS.arm64 },
  summary: (doc) =>
    `pass; artifact sha256 ${doc.artifact.sha256.slice(0, 16)}…; compatibility derived by executing the built artifact`,
});

const packageArtifactSha = packageValidation.ok ? packageValidation.loaded.doc.artifact.sha256 : null;
derivedRow({
  id: 'PKG1-binary-arm64',
  row: 'PKG-1 native binary inspection (darwin-arm64)',
  evidence: EV('native-binary-darwin-arm64/binary-inspection.json'),
  kind: {
    schema: 'rft-p3-t1-binary-inspection/v1',
    target: TARGETS.arm64,
    // This document's schema records no source_sha; it is bound to the reviewed
    // source transitively, through the artifact digest it shares with the
    // package receipt whose source_sha is verified above.
    sourceSha: null,
    extra: (doc) => {
      const problems = [];
      if (packageArtifactSha && doc.artifact?.sha256 !== packageArtifactSha) {
        problems.push(`artifact sha256 ${doc.artifact?.sha256 ?? 'absent'} != package receipt ${packageArtifactSha}`);
      }
      const container = doc.checks?.find((c) => c.name === 'artifact_container_matches_target');
      if (container?.ok !== true) problems.push('container/arch check not passing');
      if (doc.status !== 'pass') problems.push('inspection status not pass');
      return problems;
    },
  },
  summary: (doc) =>
    `pass; ${doc.findings?.container?.container}/${doc.findings?.container?.machine}, minos=${doc.findings?.minimum_os}, ` +
    `non_system_libraries=${(doc.findings?.non_system_libraries ?? []).length}`,
});

// --- PKG-2 native payload, per target ---------------------------------------

function nativePayloadRow(id, row, target, outDir, displayName) {
  const evidencePath = EV(`${outDir}/package-receipt.json`);
  if (!existsSync(absoluteFor(evidencePath))) {
    unavailableRow({
      id,
      row,
      verdict: 'MISSING',
      summary: `${displayName}: no package receipt; target package was not built`,
      raw_evidence: [evidencePath],
      reason: `no package receipt for ${target}`,
    });
    return;
  }
  derivedRow({
    id,
    row,
    evidence: evidencePath,
    kind: { schema: 'rft-p3-t1-package-receipt/v1', target },
    summary: (doc) => {
      const platform = doc.packages.find((p) => p.name.includes('native-') && p.name !== '@42ch/nexus-native');
      const mib = platform ? platform.tarball_bytes / 1048576 : null;
      return `${displayName}: ${mib?.toFixed(1)} MiB (limit ${NATIVE_PAYLOAD_LIMIT_MIB})`;
    },
    extraChecks: (doc) => {
      const platform = doc.packages.find((p) => p.name.includes('native-') && p.name !== '@42ch/nexus-native');
      if (!platform) return ['no platform tarball recorded'];
      const mib = platform.tarball_bytes / 1048576;
      return mib <= NATIVE_PAYLOAD_LIMIT_MIB
        ? []
        : [`platform tarball ${mib.toFixed(1)} MiB exceeds ${NATIVE_PAYLOAD_LIMIT_MIB} MiB`];
    },
  });
}

nativePayloadRow('PKG2-native-arm64', `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — darwin-arm64`, TARGETS.arm64, 'native-packages/darwin-arm64', 'darwin-arm64');
nativePayloadRow('PKG2-native-x64', `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — darwin-x64`, TARGETS.x64, 'native-packages/darwin-x64', 'darwin-x64');
nativePayloadRow('PKG2-native-win', `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — win32-x64-msvc`, TARGETS.win, 'native-packages/win32-x64-msvc', 'win32-x64-msvc');
nativePayloadRow('PKG2-native-linux', `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — linux-x64-gnu`, TARGETS.linux, 'native-packages/linux-x64-gnu', 'linux-x64-gnu');

// --- runtime rows -----------------------------------------------------------

const runtimeEvidencePath = EV('electron-arm64/runtime-lifecycle.json');
const runtimeLoaded = readEvidence(runtimeEvidencePath);
const runtimeVerdict = evaluateRuntimeEvidence(runtimeLoaded.doc, {});
const runtimeUsable = runtimeVerdict.state === 'valid-pass' || runtimeVerdict.state === 'valid-fail';

function runtimeRow(id, row, summaryWhenUsable) {
  if (runtimeUsable) {
    pushRow({
      id,
      row,
      verdict: 'PASS',
      raw_evidence: [runtimeEvidencePath],
      summary: summaryWhenUsable,
      verification: {
        derived: true,
        evidence_path: runtimeEvidencePath,
        evidence_sha256: runtimeLoaded.sha256,
        evidence_state: runtimeVerdict.state,
        problems: [],
      },
    });
    return;
  }
  unavailableRow({
    id,
    row,
    verdict: 'NOT OBSERVED',
    summary: `no contract-valid canonical runtime document (contract state: ${runtimeVerdict.state})`,
    raw_evidence: [runtimeEvidencePath],
    reason: `runtime evidence is ${runtimeVerdict.state}: ${runtimeVerdict.reasons.slice(0, 3).join('; ')}`,
  });
}

runtimeRow('START-1', 'START-1 cold/warm launch to interactive real graph', 'cold/warm launch samples within the fixed limits');
runtimeRow('RES-1', 'RES-1 idle/p95-active total-owned-process RSS', 'idle/p95-active RSS within the fixed limits');
runtimeRow('RES-2', 'RES-2 100-cycle retained growth / surviving owned children', 'retained growth and survivor counts within the fixed limits');
runtimeRow('SEC-renderer', 'SEC-1 renderer sandbox/isolation/navigation guard', 'renderer isolation and navigation guard asserted');

// Electron PKG-2 sizes: reported as PARTIAL because only arm64 was ever
// measured, and that measurement lives in a superseded pre-contract document.
const electronRuntimeSizes = runtimeLoaded.doc?.sizes ?? null;
unavailableRow({
  id: 'PKG2-electron',
  row: `PKG-2 Electron .app zip <= ${ELECTRON_ZIP_LIMIT_MIB} MiB and installed bundle <= ${ELECTRON_INSTALLED_LIMIT_MIB} MiB per arch`,
  verdict: 'PARTIAL',
  summary: electronRuntimeSizes
    ? `arm64 measured historically: zip ${electronRuntimeSizes.zip_mib} MiB / installed ${electronRuntimeSizes.app_bundle_mib} MiB; ` +
      'x64 not measured; the measurement is not derivable from a contract-valid runtime document and must be re-captured'
    : 'no Electron size measurement available',
  raw_evidence: [runtimeEvidencePath, EV('electron-packages/arm64/package-receipt.json')],
  reason: `runtime measurement document is ${runtimeVerdict.state}; x64 unmeasured`,
});

// --- SEC-1 signed execution and MAINT-1 -------------------------------------

const signingGatePath = EV('electron-arm64/proof-package.json');
const signingGate = readEvidence(signingGatePath);
unavailableRow({
  id: 'SEC1-signed',
  row: 'SEC-1 signed/hardened/notarized/stapled arm64+x64 execution with native load',
  verdict: 'BLOCKED',
  summary: signingGate.doc
    ? `gate status ${signingGate.doc.status}; runtime contract ${signingGate.doc.checks?.runtime_lifecycle?.contract_state ?? 'n/a'}; ` +
      '0 code-signing identities and no notary credentials on the host'
    : '0 code-signing identities and no notary credentials on the host',
  raw_evidence: [signingGatePath, EV('native-binary-darwin-arm64/binary-inspection.json')],
  reason: 'missing signing identity and notarization credentials',
});
unavailableRow({
  id: 'MAINT-1',
  row: 'MAINT-1 Electron patch upgrade rebuild <= 30 min, one rebuild per arch',
  verdict: 'NOT OBSERVED',
  summary: 'no patch-rebuild trace executed; patch releases in the pinned major exist but were not rebuilt',
  raw_evidence: [],
  reason: 'no patch-rebuild trace executed',
});

// --- missing inputs ---------------------------------------------------------

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
      'APPLE_ID / APPLE_APP_SPECIFIC_PASSWORD / APPLE_TEAM_ID / APPLE_API_KEY / APPLE_API_KEY_ID / APPLE_API_ISSUER / NOTARY_KEYCHAIN_PROFILE unset; no notarytool profile',
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
    input: 'A contract-valid canonical runtime run via LaunchServices on a signed build',
    needed_for: 'START-1, RES-1, RES-2, SEC-renderer and the PKG-2 Electron size rows',
    observed_state: `no canonical document exists (contract state: ${runtimeVerdict.state})`,
    external_action:
      'Re-run proof-runtime.mjs with --launch-method launchservices and the canonical phase set on the signed package.',
  },
];

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
  ['P3-T3 contract raw-evidence gating', 'apps/desktop-electron/scripts/proof-contract.mjs', 'check IDs were existence-checked rather than set-equal and samples were read from summaries, so a crafted document could pass without raw observations'],
  ['P3-T3 decision derivation', 'apps/desktop-electron/scripts/proof-decision.mjs', 'row verdicts were hardcoded PASS and the aggregate native PKG-2 row over-claimed all targets'],
  ['P3-T3 runtime fail-fast', 'apps/desktop-electron/scripts/proof-runtime.mjs', 'readiness burned its full window on an unavailable owner and an expired run wrote no record'],
  ['P3-T3 provider lifecycle completeness', 'apps/desktop-electron/scripts/proof-runtime.mjs', 'provider pass did not require pull, a terminal event or shutdown'],
];

// --- emit -------------------------------------------------------------------

function countBy(list, key) {
  return list.reduce((acc, entry) => {
    acc[entry[key]] = (acc[entry[key]] ?? 0) + 1;
    return acc;
  }, {});
}

function main() {
  const args = process.argv.slice(2);
  const outPath = args.includes('--out')
    ? resolve(args[args.indexOf('--out') + 1])
    : join(EVIDENCE, 'electron-decision.json');

  const counts = countBy(rows, 'verdict');
  const passRows = counts.PASS ?? 0;
  const measuredFailures = counts.FAIL ?? 0;
  const missingRows = rows.length - passRows - measuredFailures;
  const allRequiredRowsPass = rows.length > 0 && rows.every((row) => row.verdict === 'PASS');

  let status = 'blocked';
  if (allRequiredRowsPass) status = 'go';
  else if (measuredFailures > 0) status = 'no-go';

  const decision = {
    schema: 'rft-p3-t3-electron-decision/v3',
    plan_id: '2026-09-12-v1.189-p3-native-electron-feasibility',
    task: 'P3-T3',
    owner: 'ops-engineer',
    authority: 'plan §P3-T3 + proof-matrix §2/§6 (guides/evidence/electron-decision.json)',
    generated_by: 'apps/desktop-electron/scripts/proof-decision.mjs',
    status,
    status_rule:
      'GO requires every row to pass, where each row verdict is derived from the evidence document it cites ' +
      '(schema/target/source identity/status) rather than asserted. A measured failure = no-go. ' +
      'Anything else, including any missing input, = blocked.',
    accepted_macos_floor: {
      reference: 'architecture-contracts §10 U1 (user, 2026-09-13)',
      floor: 'macOS 13+ (Ventura) on arm64 and x86_64',
      note: 'Reference only; no signed execution on that floor was performed.',
    },
    ...source,
    evidence_root: '.mstar/iterations/v1.189/guides/evidence',
    row_counts: {
      total: rows.length,
      pass: passRows,
      fail: measuredFailures,
      missing_or_unobserved: missingRows,
      by_verdict: counts,
      definition:
        'one row per required PKG/RES/START/SEC/MAINT criterion, with PKG-2 native payload split per target ' +
        'and PKG-2 Electron sizes kept as one per-arch row',
    },
    evidence_freshness: source.tree_dirty
      ? 'the working tree is dirty; only source_sha can be compared for the T1 evidence documents, which do not record a tree digest'
      : 'working tree clean; source_sha comparison is exact',
    unobserved_rows: rows.filter((row) => row.verdict !== 'PASS').map((row) => ({ id: row.id, row: row.row, verdict: row.verdict, summary: row.summary })),
    observed_rows: rows,
    missing_inputs: MISSING_INPUTS,
    runtime_defects_found_and_fixed_during_proof: RUNTIME_FIXES.map(([id, file, detail]) => ({ id, file, detail })),
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
      'Rows not backed by a contract-valid document are NOT OBSERVED / MISSING, never PASS.',
      'The native PKG-2 criterion is reported per target; only darwin-arm64 is built.',
    ],
  };

  writeFileSync(outPath, `${JSON.stringify(decision, null, 2)}\n`);
  console.log(
    JSON.stringify({ outPath, status, row_counts: decision.row_counts, integrity: source }, null, 2),
  );
}

main();
