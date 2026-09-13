#!/usr/bin/env node
/**
 * P3-T3 decision generator.
 *
 * The row set is declarative: every criterion the proof matrix requires is
 * materialised per architecture, and each verdict is *derived* from the evidence
 * document it cites. A row is `PASS` only when the document exists, parses,
 * matches the expected schema/target/source identity, and every required
 * predicate inside it holds; a document that reports a genuine measured failure
 * becomes `FAIL` (which is what makes the decision a no-go); anything absent,
 * malformed, inconsistent or stale becomes `MISSING`/`BLOCKED`/`STALE`.
 *
 * Nothing is hardcoded: supplying a complete signed dual-architecture matrix
 * makes this generator emit `go` without editing it (C4, I11), and a measured
 * failure anywhere yields `no-go` (C3, I10).
 *
 * Usage: node apps/desktop-electron/scripts/proof-decision.mjs [--out <file>] [--evidence-root <dir>]
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { evaluateRuntimeEvidence, runtimeCriterionVerdict } from './proof-contract.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..');

/**
 * The harness tree lives in the primary worktree, not in a feature worktree
 * (`.mstar/**` is gitignored). Resolve the evidence root against the repository
 * that actually holds the documents; `--evidence-root` overrides it.
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
const EV_PREFIX = '.mstar/iterations/v1.189/guides/evidence/';
const EV = (relative) => `${EV_PREFIX}${relative}`;
const absoluteFor = (displayPath) =>
  join(EVIDENCE, displayPath.startsWith(EV_PREFIX) ? displayPath.slice(EV_PREFIX.length) : displayPath);

const APP_BUNDLE = (archKey) =>
  `electron-packages/${archKey}/Nexus RFT Feasibility-darwin-${archKey}/Nexus RFT Feasibility.app`;

/** Architecture descriptors. Ordering is the row ordering. */
const ARCHES = [
  { key: 'arm64', target: 'aarch64-apple-darwin', suffix: 'darwin-arm64', install: { '22.22.0': 'install-macarm22', '24.20.0': 'install-macarm24' }, platforms: 'macos' },
  { key: 'x64', target: 'x86_64-apple-darwin', suffix: 'darwin-x64', install: { '22.22.0': 'install-macx6422', '24.20.0': 'install-macx6424' }, platforms: 'macos' },
  { key: 'win', target: 'x86_64-pc-windows-msvc', suffix: 'win32-x64-msvc', install: { '22.22.0': 'install-win22', '24.20.0': 'install-win24' }, platforms: 'win' },
  { key: 'linux', target: 'x86_64-unknown-linux-gnu', suffix: 'linux-x64-gnu', install: { '22.22.0': 'install-linux22', '24.20.0': 'install-linux24' }, platforms: 'linux' },
];
const NODE_COHORTS = [
  { version: '22.22.0', id: 'node22' },
  { version: '24.20.0', id: 'node24' },
];
/** macOS GUI architectures the SEC-1 / Electron rows apply to. */
const GUI_ARCHES = ARCHES.filter((arch) => arch.platforms === 'macos');

const NATIVE_PAYLOAD_LIMIT_MIB = 50;
const ELECTRON_ZIP_LIMIT_MIB = 250;
const ELECTRON_INSTALLED_LIMIT_MIB = 600;
const MAINT_MAX_SECONDS = 1800;
const RUNTIME_CRITERIA = [
  ['START-1', 'START-1 cold/warm launch to interactive real graph'],
  ['RES-1', 'RES-1 idle/p95-active total-owned-process RSS'],
  ['RES-2', 'RES-2 100-cycle retained growth / surviving owned children'],
  ['SEC-renderer', 'SEC-1 renderer sandbox/isolation/navigation guard'],
  ['LIFECYCLE-native', 'LIFECYCLE-native packaged native graph/patch/provider effects'],
  ['LIFECYCLE-fault', 'LIFECYCLE-fault owner-loss fence and explicit reopen'],
];

// --- identity and evidence access -------------------------------------------

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

const source = sourceIdentity();
const rows = [];

function readEvidence(relative) {
  const absolute = absoluteFor(relative);
  if (!existsSync(absolute)) return { exists: false, doc: null, reason: `absent: ${relative}` };
  const bytes = readFileSync(absolute);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  try {
    return { exists: true, doc: JSON.parse(bytes.toString('utf8')), sha256, reason: null };
  } catch (error) {
    return { exists: true, doc: null, sha256, reason: `unparseable: ${error.message}` };
  }
}

function pinnedVersion(pkgName) {
  for (const candidate of [
    join(ROOT, 'node_modules', pkgName, 'package.json'),
    join(ROOT, 'apps', 'desktop-electron', 'node_modules', pkgName, 'package.json'),
  ]) {
    if (existsSync(candidate)) {
      try {
        return JSON.parse(readFileSync(candidate, 'utf8')).version ?? null;
      } catch {
        return null;
      }
    }
  }
  return null;
}

/**
 * Derive a row verdict from one evidence document.
 *
 * `identity` selects which fields must match; `predicates` returns the required
 * named checks and their outcomes. The resulting state maps to the decision
 * vocabulary as: pass -> PASS, fail -> FAIL (a real measured failure),
 * stale -> STALE, missing -> MISSING, blocked -> BLOCKED.
 */
function deriveRow({ id, row, evidence, schema, target, sourceSha = source.source_sha, identity = {}, predicates = () => ({ required: [], failures: [], notes: [] }), describe = () => null }) {
  const loaded = readEvidence(evidence);
  const problems = [];
  let info = [];
  let state = 'pass';

  if (!loaded.exists) {
    state = 'missing';
    problems.push(`absent: ${evidence}`);
  } else if (!loaded.doc) {
    state = 'blocked';
    problems.push(loaded.reason);
  } else {
    const doc = loaded.doc;
    if (schema && doc.schema !== schema) problems.push(`schema ${doc.schema ?? 'absent'} != ${schema}`);
    if (target && doc.target !== target) problems.push(`target ${doc.target ?? 'absent'} != ${target}`);
    if (sourceSha && doc.source_sha !== sourceSha) {
      problems.push(`source_sha ${doc.source_sha ?? 'absent'} != head ${sourceSha}`);
    }
    for (const [field, expected] of Object.entries(identity)) {
      if (expected === undefined || expected === null) continue;
      const actual = field.split('.').reduce((acc, key) => (acc === undefined || acc === null ? undefined : acc[key]), doc);
      if (actual !== expected) problems.push(`${field} ${actual ?? 'absent'} != ${expected}`);
    }

    const structural = problems.length > 0;
    if (structural) {
      state = 'stale';
    } else {
      const { required = [], failures = [], notes = [] } = predicates(doc) ?? {};
      const missing = required.filter((name) => !docNamedCheck(doc, name));
      const failedChecks = [...missing.map((name) => `${name} missing`), ...failures];
      const declaredStatus = doc.status;
      if (declaredStatus === 'fail') {
        state = 'fail';
        problems.push(...notes, 'document records a measured failure');
      } else if (declaredStatus !== 'pass') {
        state = 'blocked';
        problems.push(`status ${declaredStatus ?? 'absent'} is neither pass nor fail`);
      } else if (failedChecks.length > 0) {
        // status: pass with a false predicate is an inconsistency, not a
        // measured failure — fail closed rather than guess (I10).
        state = 'blocked';
        problems.push(...notes, `status is pass but predicates failed: ${failedChecks.join('; ')}`);
      } else {
        // Informational context only; a passing row has no problems.
        info = notes;
      }
    }
  }

  const verdict = { pass: 'PASS', fail: 'FAIL', stale: 'STALE', missing: 'MISSING', blocked: 'BLOCKED' }[state];
  rows.push({
    id,
    row,
    verdict,
    raw_evidence: [evidence],
    summary: describe(loaded.doc) ?? problems.join('; ') ?? null,
    verification: {
      derived: true,
      evidence_path: evidence,
      evidence_sha256: loaded.sha256 ?? null,
      evidence_state: state,
      problems,
      notes: info,
    },
  });
  return loaded.doc;
}

/** Look up a named entry in a T1 receipt's `checks` array. */
function docNamedCheck(doc, name) {
  return (doc?.checks ?? []).some((check) => check?.name === name && check.ok === true);
}

function unavailableRow({ id, row, verdict, summary, raw_evidence = [], reason }) {
  rows.push({ id, row, verdict, raw_evidence, summary, verification: { derived: false, reason } });
}

// --- PKG-1: native install/load, package receipt, binary inspection ----------

/** Predicates every install-proof document must satisfy (I10). */
const INSTALL_REQUIRED_CHECKS = [
  'package_receipt_pass',
  'empty_project_install',
  'installed_payload_paths',
  'installed_artifact_matches_packed_payload',
  'installed_platform_manifest_is_frozen_metadata',
  'installed_manifest_fences_against_real_artifact',
  'graph_read_through_installed_payload',
  'create_through_installed_payload',
  'update_through_installed_payload',
  'provider_probe_available',
  'provider_launch_session',
  'provider_operation_started',
  'provider_stream_delta_and_terminal',
  'provider_shutdown_ok',
  'happy_close_confirmed',
  'provider_cancel_accepted',
  'cancel_close_confirmed',
  'compilers_shadowed_in_child',
  'no_compiler_invocation_attempted',
  'fixture_children_reaped',
];

for (const arch of ARCHES) {
  for (const cohort of NODE_COHORTS) {
    const nodeVersion = cohort.version;
    const dir = arch.install[nodeVersion];
    deriveRow({
      id: `PKG1-install-${arch.key}-${cohort.id}`,
      row: `PKG-1 ${arch.suffix} native install/load — Node ${nodeVersion}`,
      evidence: EV(`${dir}/install-proof.json`),
      schema: 'rft-p3-t1-native-install-proof/v1',
      target: arch.target,
      identity: { node_version_requested: nodeVersion },
      predicates: (doc) => {
        const failed = [];
        for (const name of INSTALL_REQUIRED_CHECKS) {
          if (!docNamedCheck(doc, name)) failed.push(name);
        }
        const negativeNames = (doc.checks ?? []).filter((c) => String(c.name).startsWith('negative:'));
        if (negativeNames.length < 10) failed.push('fewer than 10 pre-open negative checks');
        const okCount = (doc.checks ?? []).filter((c) => c.ok).length;
        return {
          required: ['package_receipt_pass', 'empty_project_install'],
          failures: failed,
          notes: [`${okCount}/${(doc.checks ?? []).length} checks pass`],
        };
      },
      describe: (doc) =>
        doc
          ? `pass, ${doc.checks.filter((c) => c.ok).length}/${doc.checks.length} checks; node ${doc.node_version_requested}`
          : null,
    });
  }
}

for (const arch of ARCHES) {
  const packagePath = EV(`native-packages/${arch.suffix}/package-receipt.json`);
  const receipt = deriveRow({
    id: `PKG1-package-${arch.key}`,
    row: `PKG-1 ${arch.suffix} native package receipt (frozen payload + compatibility)`,
    evidence: packagePath,
    schema: 'rft-p3-t1-package-receipt/v1',
    target: arch.target,
    predicates: (doc) => {
      const failed = [];
      const artifact = doc.artifact ?? {};
      if (typeof artifact.sha256 !== 'string' || artifact.sha256.length !== 64) failed.push('artifact sha256 absent');
      if (typeof artifact.bytes !== 'number' || artifact.bytes <= 0) failed.push('artifact bytes not positive');
      if ((doc.compatibility?.source ?? '') !== 'executed artifact compatibility()') {
        failed.push('compatibility not derived from the executed artifact');
      }
      if (!Array.isArray(doc.packages) || doc.packages.length < 2) failed.push('package tarball list incomplete');
      for (const field of ['target_triple', 'native_api_version', 'writer_protocol', 'contract_tree_sha256']) {
        if (doc.compatibility?.manifest?.[field] === undefined) failed.push(`compatibility.${field} absent`);
      }
      if (doc.compatibility?.manifest?.target_triple !== arch.target) failed.push('compatibility target mismatch');
      return { failures: failed, notes: [] };
    },
    describe: (doc) => (doc ? `pass; artifact sha256 ${doc.artifact.sha256.slice(0, 16)}…` : null),
  });
  const artifactSha = receipt?.artifact?.sha256 ?? null;

  deriveRow({
    id: `PKG1-binary-${arch.key}`,
    row: `PKG-1 native binary inspection (${arch.suffix})`,
    evidence: EV(`native-binary-${arch.suffix}/binary-inspection.json`),
    schema: 'rft-p3-t1-binary-inspection/v1',
    target: arch.target,
    // This schema records no source_sha; it is bound transitively through the
    // artifact digest it shares with the source-verified package receipt.
    sourceSha: null,
    predicates: (doc) => {
      const failed = [];
      if (artifactSha && doc.artifact?.sha256 !== artifactSha) {
        failed.push(`artifact sha256 ${doc.artifact?.sha256 ?? 'absent'} != package receipt ${artifactSha}`);
      }
      for (const check of doc.checks ?? []) {
        if (check.ok !== true) failed.push(`inspection check ${check.name} not passing`);
      }
      if ((doc.checks ?? []).length === 0) failed.push('no inspection checks recorded');
      return { failures: failed, notes: [] };
    },
    describe: (doc) =>
      doc
        ? `pass; ${doc.findings?.container?.container}/${doc.findings?.container?.machine}, minos=${doc.findings?.minimum_os}`
        : null,
  });
}

// --- PKG-2: native npm payload, per target ----------------------------------

for (const arch of ARCHES) {
  const evidence = EV(`native-packages/${arch.suffix}/package-receipt.json`);
  if (!existsSync(absoluteFor(evidence))) {
    unavailableRow({
      id: `PKG2-native-${arch.key}`,
      row: `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — ${arch.suffix}`,
      verdict: 'MISSING',
      summary: `${arch.suffix}: no package receipt; target package was not built`,
      raw_evidence: [evidence],
      reason: `no package receipt for ${arch.target}`,
    });
    continue;
  }
  deriveRow({
    id: `PKG2-native-${arch.key}`,
    row: `PKG-2 native npm payload <= ${NATIVE_PAYLOAD_LIMIT_MIB} MiB — ${arch.suffix}`,
    evidence,
    schema: 'rft-p3-t1-package-receipt/v1',
    target: arch.target,
    predicates: (doc) => {
      const platform = (doc.packages ?? []).find((p) => p.name.includes('native-') && p.name !== '@42ch/nexus-native');
      if (!platform) return { failures: ['no platform tarball recorded'], notes: [] };
      const mib = platform.tarball_bytes / 1048576;
      return {
        failures: mib <= NATIVE_PAYLOAD_LIMIT_MIB ? [] : [`platform tarball ${mib.toFixed(1)} MiB exceeds ${NATIVE_PAYLOAD_LIMIT_MIB} MiB`],
        notes: [`${mib.toFixed(1)} MiB of ${NATIVE_PAYLOAD_LIMIT_MIB} MiB`],
      };
    },
    describe: (doc) => {
      const platform = (doc?.packages ?? []).find((p) => p.name.includes('native-') && p.name !== '@42ch/nexus-native');
      return platform ? `${arch.suffix}: ${(platform.tarball_bytes / 1048576).toFixed(1)} MiB (limit ${NATIVE_PAYLOAD_LIMIT_MIB})` : null;
    },
  });
}

// --- runtime criteria, per GUI architecture ---------------------------------

for (const arch of GUI_ARCHES) {
  const evidence = EV(`electron-${arch.key}/runtime-lifecycle.json`);
  const loaded = readEvidence(evidence);
  const bundlePath = absoluteFor(APP_BUNDLE(arch.key));
  const expectations = {
    // The runtime records the realpath of the bundle it measured, so the
    // expectation must be canonicalised the same way (a /tmp symlink would
    // otherwise make every match look stale).
    app_path: existsSync(bundlePath) ? realpathSync(bundlePath) : undefined,
    app_bundle_id: 'com.nexus42.rft-electron-proof',
    arch: arch.key,
    electron_version: pinnedVersion('electron') ?? undefined,
    packager_version: pinnedVersion('@electron/packager') ?? undefined,
    source_sha: source.source_sha,
    tree_digest: source.tree_digest,
    tree_dirty: source.tree_dirty,
  };
  const verdict = evaluateRuntimeEvidence(loaded.doc, expectations);

  for (const [checkId, rowLabel] of RUNTIME_CRITERIA) {
    const criterion = runtimeCriterionVerdict(verdict.state, loaded.doc, checkId);
    if (criterion === 'PASS' || criterion === 'FAIL') {
      rows.push({
        id: `${checkId}-${arch.key}`,
        row: `${rowLabel} (${arch.key})`,
        verdict: criterion,
        raw_evidence: [evidence],
        summary: `${checkId} ${criterion === 'PASS' ? 'passed' : 'failed'} in the canonical ${arch.key} run`,
        verification: {
          derived: true,
          evidence_path: evidence,
          evidence_sha256: loaded.sha256 ?? null,
          evidence_state: verdict.state,
          criterion,
          problems: criterion === 'FAIL' ? [`${checkId} failed`] : [],
        },
      });
    } else {
      unavailableRow({
        id: `${checkId}-${arch.key}`,
        row: `${rowLabel} (${arch.key})`,
        verdict: 'NOT OBSERVED',
        summary: `no contract-valid canonical runtime document (contract state: ${verdict.state})`,
        raw_evidence: [evidence],
        reason: `runtime evidence is ${verdict.state}: ${verdict.reasons.slice(0, 3).join('; ')}`,
      });
    }
  }
}

// --- PKG-2 Electron size, per GUI architecture ------------------------------

for (const arch of GUI_ARCHES) {
  deriveRow({
    id: `PKG2-electron-${arch.key}`,
    row:
      `PKG-2 Electron .app zip <= ${ELECTRON_ZIP_LIMIT_MIB} MiB and installed bundle ` +
      `<= ${ELECTRON_INSTALLED_LIMIT_MIB} MiB — ${arch.key}`,
    evidence: EV(`electron-${arch.key}/electron-size.json`),
    schema: 'rft-p3-t3-electron-size/v1',
    target: null,
    identity: { arch: arch.key },
    predicates: (doc) => {
      const failed = [];
      if (doc.sizes?.zip_mib == null || doc.sizes.zip_mib > ELECTRON_ZIP_LIMIT_MIB) failed.push('zip over limit');
      if (doc.sizes?.app_bundle_mib == null || doc.sizes.app_bundle_mib > ELECTRON_INSTALLED_LIMIT_MIB) {
        failed.push('installed bundle over limit');
      }
      for (const check of doc.checks ?? []) {
        if (check.ok !== true) failed.push(`size check ${check.id} not passing`);
      }
      return {
        failures: failed,
        notes:
          doc.sizes?.zip_mib != null
            ? [`zip ${doc.sizes.zip_mib} MiB / installed ${doc.sizes.app_bundle_mib} MiB`]
            : [],
      };
    },
    describe: (doc) =>
      doc?.sizes ? `zip ${doc.sizes.zip_mib} MiB / installed ${doc.sizes.app_bundle_mib} MiB (${arch.key})` : null,
  });
}

// --- SEC-1 signed execution, per GUI architecture (C4) ----------------------

for (const arch of GUI_ARCHES) {
  const evidence = EV(`electron-${arch.key}/proof-package.json`);
  const loaded = readEvidence(evidence);
  const gate = loaded.doc;
  const gateIdentityExpected = {
    bundle_id: 'com.nexus42.rft-electron-proof',
    arch: arch.key,
  };

  if (!loaded.exists) {
    unavailableRow({
      id: `SEC1-signed-${arch.key}`,
      row: `SEC-1 signed/hardened/notarized/stapled execution with native load (${arch.key})`,
      verdict: 'MISSING',
      summary: `no package-gate document for ${arch.key}`,
      raw_evidence: [evidence],
      reason: 'no signed gate document',
    });
    continue;
  }

  const structural = [];
  if (!gate) structural.push(loaded.reason);
  else {
    if (gate.schema !== 'rft-p3-t3-package-gate/v2') structural.push(`schema ${gate.schema ?? 'absent'} != rft-p3-t3-package-gate/v2`);
    for (const [field, expected] of Object.entries(gateIdentityExpected)) {
      const actual = gate[field];
      if (actual !== expected) structural.push(`${field} ${actual ?? 'absent'} != ${expected}`);
    }
    if (gate.app_realpath && !gate.app_realpath.includes(`darwin-${arch.key}`)) {
      structural.push(`app_realpath ${gate.app_realpath} is not the ${arch.key} package`);
    }
  }

  let verdict;
  let summary;
  if (structural.length > 0) {
    verdict = 'BLOCKED';
    summary = `package gate unusable: ${structural.join('; ')}`;
  } else if (gate.status === 'go') {
    verdict = 'PASS';
    summary = `signed gate reports go for ${arch.key}`;
  } else if (gate.status === 'no-go') {
    verdict = 'FAIL';
    summary = `signed gate reports a measured failure (no-go) for ${arch.key}`;
  } else if (gate.status === 'blocked') {
    // Still derived, not hardcoded: the *reason* is read from the document, and
    // the moment that document reports go this row becomes PASS.
    const inputs = (gate.missing_inputs ?? []).slice(0, 2).join('; ');
    const reasons = (gate.reasons ?? []).slice(0, 2).join('; ');
    verdict = 'BLOCKED';
    summary = `gate blocked: ${reasons || inputs || gate.checks?.runtime_lifecycle?.contract_state || 'see proof-package.json'}`;
  } else {
    verdict = 'BLOCKED';
    summary = `gate status ${gate.status ?? 'absent'} is not a decision value`;
  }

  rows.push({
    id: `SEC1-signed-${arch.key}`,
    row: `SEC-1 signed/hardened/notarized/stapled execution with native load (${arch.key})`,
    verdict,
    raw_evidence: [evidence],
    summary,
    verification: {
      derived: true,
      evidence_path: evidence,
      evidence_sha256: loaded.sha256 ?? null,
      evidence_state: gate?.status ?? 'blocked',
      gate_status: gate?.status ?? null,
      gate_decision_inputs: gate?.decision_inputs ?? null,
      problems: structural,
    },
  });
}

// --- MAINT-1 ----------------------------------------------------------------

deriveRow({
  id: 'MAINT-1',
  row: 'MAINT-1 Electron patch upgrade rebuild <= 30 min, one rebuild per arch',
  evidence: EV('maintenance-rebuild.json'),
  schema: 'rft-p3-t3-maintenance-rebuild/v1',
  target: null,
  predicates: (doc) => {
    const failed = [];
    if (typeof doc.elapsed_seconds !== 'number' || doc.elapsed_seconds > MAINT_MAX_SECONDS) {
      failed.push(`elapsed ${doc.elapsed_seconds ?? 'absent'}s exceeds ${MAINT_MAX_SECONDS}s`);
    }
    if (doc.manual_binary_patch === true) failed.push('a manual binary patch was applied');
    if (doc.rebuild_count !== 1) failed.push(`rebuild_count ${doc.rebuild_count ?? 'absent'} != 1`);
    if (doc.packaged_electron_version !== doc.target_electron_version) {
      failed.push(
        `packaged Electron ${doc.packaged_electron_version ?? 'absent'} != target ${doc.target_electron_version ?? 'absent'}`,
      );
    }
    return { failures: failed, notes: [] };
  },
  describe: (doc) =>
    doc
      ? `rebuilt to Electron ${doc.packaged_electron_version} in ${doc.elapsed_seconds}s with ${doc.rebuild_count} rebuild`
      : null,
});

// --- missing inputs ---------------------------------------------------------

const MISSING_INPUTS = [
  {
    input: 'Apple Developer signing identity (Developer ID Application)',
    needed_for: 'SEC-1 and any signed package',
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
    needed_for: 'PKG-1 darwin-x64, PKG-2 x64, SEC-1 x64 execution',
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
    input: 'A contract-valid canonical runtime run on a signed build',
    needed_for: 'START-1, RES-1, RES-2, SEC-renderer, LIFECYCLE rows',
    observed_state: 'the canonical arm64 run recorded the utility_owner_unavailable confounder; no x64 run exists',
    external_action:
      'Re-run proof-runtime.mjs with --launch-method launchservices and the canonical phase set on each signed package.',
  },
  {
    input: 'A per-architecture Electron size capture (arm64 and x64)',
    needed_for: 'the PKG-2 Electron size rows',
    observed_state:
      'no electron-size.json exists yet; the earlier arm64 measurement predates the per-arch size document and is not ' +
      'derivable from a contract-valid source',
    external_action:
      'Run proof-runtime.mjs once per architecture. Measuring the bundle on disk does not require the app to run or the ' +
      'package to be signed, so this row can be captured now.',
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
  ['P3-T3 runtime criterion mapping', 'apps/desktop-electron/scripts/proof-decision.mjs', 'valid-fail runtime evidence was emitted as PASS rows, losing the required no-go'],
  ['P3-T3 SEC-1 permanence', 'apps/desktop-electron/scripts/proof-decision.mjs', 'the SEC-1 signed row was hardcoded BLOCKED, so the reducer could never emit go'],
  ['P3-T3 canonical metadata enforcement', 'apps/desktop-electron/scripts/proof-contract.mjs', 'mode, status and sample_plan were not validated, so a diagnostic-shaped document could gate'],
  ['P3-T3 source/tree binding', 'apps/desktop-electron/scripts/proof-decision.mjs and proof-package.mjs', 'runtime evidence was not compared against the current source SHA / tree digest'],
  ['P3-T3 receipt predicate coverage', 'apps/desktop-electron/scripts/proof-decision.mjs', 'T1 rows trusted the top-level status instead of validating every required named check'],
  ['P3-T3 per-arch Electron size', 'apps/desktop-electron/scripts/proof-decision.mjs and proof-runtime.mjs', 'the size criterion was permanently PARTIAL; it is now derived per architecture from electron-size.json'],
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

  // GO only when every row passes. A genuinely measured failure is a no-go even
  // alongside unobserved rows — a demonstrated failure decides. Everything else
  // (missing/stale/blocked rows) is a missing input, which blocks.
  let status = 'blocked';
  if (allRequiredRowsPass) status = 'go';
  else if (measuredFailures > 0) status = 'no-go';

  const decision = {
    schema: 'rft-p3-t3-electron-decision/v4',
    plan_id: '2026-09-12-v1.189-p3-native-electron-feasibility',
    task: 'P3-T3',
    owner: 'ops-engineer',
    authority: 'plan §P3-T3 + proof-matrix §2/§6 (guides/evidence/electron-decision.json)',
    generated_by: 'apps/desktop-electron/scripts/proof-decision.mjs',
    status,
    status_rule:
      'GO requires every row to pass, where each verdict is derived from the evidence document it cites ' +
      '(schema, target, source/tree identity, declared status, and every required named predicate). ' +
      'A genuine measured failure = no-go. Anything else, including any missing input, = blocked.',
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
        'one row per required criterion per architecture where the criterion is architecture-bound: PKG-1 ' +
        'install (4 targets x 2 Node cohorts), PKG-1 package receipt and binary inspection (4 targets each), ' +
        'PKG-2 native payload (4 targets), runtime criteria and PKG-2 Electron size and SEC-1 signing (2 GUI ' +
        'arches each), plus MAINT-1',
    },
    row_derivation: {
      'PASS': 'document present, schema/target/source match, declared status pass, every required predicate true',
      'FAIL': 'document records a genuine measured failure (declared status fail)',
      'STALE': 'document exists but its schema/target/source identity does not match the current review',
      'MISSING': 'document absent',
      'BLOCKED': 'document unparseable, or declared status inconsistent with its own predicates',
      'NOT OBSERVED': 'runtime criterion with no contract-valid canonical document for that architecture',
    },
    evidence_freshness: source.tree_dirty
      ? 'the working tree is dirty; runtime evidence is compared against the live tree digest and will be reported STALE until regenerated at the committed head'
      : 'working tree clean; source_sha and tree_digest comparisons are exact',
    unobserved_rows: rows
      .filter((row) => row.verdict !== 'PASS')
      .map((row) => ({ id: row.id, row: row.row, verdict: row.verdict, summary: row.summary })),
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
      'Rows not backed by a contract-valid document are NOT OBSERVED / MISSING / STALE / BLOCKED, never PASS.',
      'Supplying a complete signed dual-architecture matrix makes this generator emit go without editing it.',
    ],
  };

  writeFileSync(outPath, `${JSON.stringify(decision, null, 2)}\n`);
  console.log(JSON.stringify({ outPath, status, row_counts: decision.row_counts, integrity: source }, null, 2));
}

main();
