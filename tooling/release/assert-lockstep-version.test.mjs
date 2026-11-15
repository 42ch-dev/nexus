import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  LOCKSTEP_PATHS,
  NATIVE_LOADER_PACKAGE_PATH,
  NATIVE_SURFACE_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  VERSION_BRACKETS_PATH,
  parseCargoLockPackageVersion,
  readVersionBrackets,
  readWorkspaceMemberPackages,
} from "./version-surfaces.mjs";
import { checkLockstep } from "./assert-lockstep-version.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  expectedMemberLockVersions,
  FIXTURE_BASELINE_VERSION,
  mutateRepoFile,
  readRepoFile,
  removeRepoFile,
  runReleaseScript,
  writeRepoFile,
} from "./test-harness.mjs";

test("checkLockstep accepts the fixture surface set", () => {
  const dir = createTempRepo();
  try {
    const report = checkLockstep(dir);
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.version, FIXTURE_BASELINE_VERSION);
    assert.equal(
      report.surfaces.length,
      LOCKSTEP_PATHS.length + NATIVE_SURFACE_PATHS.length,
    );
    assert.equal(report.lockMembers.length, readWorkspaceMemberPackages(dir).length);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkLockstep reports a divergent JSON surface", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, PRODUCT_JSON_PATH, (contents) =>
      contents.replace(/"version": "[^"]*"/, '"version": "9.9.9"'),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.equal(report.version, null);
    assert.ok(
      report.problems.some((problem) => problem.includes(PRODUCT_JSON_PATH)),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkLockstep reports a non-workspace native platform pin", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, NATIVE_LOADER_PACKAGE_PATH, (contents) =>
      contents.replace(
        /"@42ch\/nexus-native-darwin-arm64": "workspace:\*"/,
        '"@42ch/nexus-native-darwin-arm64": "9.9.9"',
      ),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some((problem) =>
        problem.includes("@42ch/nexus-native-darwin-arm64"),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkLockstep reports a stale Cargo.lock member entry", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, CARGO_LOCK_PATH, (contents) =>
      contents.replace(
        /(\[\[package\]\]\nname = "nexus42"\nversion = ")[^"]+(")/,
        "$19.9.9$2",
      ),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) => problem.includes("nexus42") && problem.includes("9.9.9"),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkLockstep fails closed when Cargo.lock is missing", () => {
  const dir = createTempRepo();
  try {
    removeRepoFile(dir, CARGO_LOCK_PATH);
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(report.problems.some((problem) => problem.includes(`${CARGO_LOCK_PATH}: missing`)));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI exits non-zero and names the mismatch", () => {
  const dir = createTempRepo();
  try {
    const okRun = runReleaseScript("assert-lockstep-version.mjs", [], dir);
    assert.equal(okRun.status, 0, okRun.stderr);
    assert.match(okRun.stdout, /Lockstep OK:/);

    mutateRepoFile(dir, ROOT_PACKAGE_PATH, (contents) =>
      contents.replace(/"version": "[^"]*"/, '"version": "1.2.3"'),
    );
    const badRun = runReleaseScript("assert-lockstep-version.mjs", [], dir);
    assert.equal(badRun.status, 1);
    assert.match(badRun.stderr, /Lockstep mismatch/);
    assert.match(badRun.stderr, /package\.json=1\.2\.3/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI --help exits zero", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("assert-lockstep-version.mjs", ["--help"], dir);
    assert.equal(run.status, 0);
    assert.match(run.stdout, /Usage: node tooling\/release\/assert-lockstep-version\.mjs/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("every workspace member resolves to its own bracket's lock entry", () => {
  const dir = createTempRepo();
  try {
    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    const expected = expectedMemberLockVersions(dir, FIXTURE_BASELINE_VERSION);
    assert.ok(expected.size > 20, `expected > 20 members, got ${expected.size}`);
    for (const [name, version] of expected) {
      assert.equal(parseCargoLockPackageVersion(lock, name), version, name);
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

/** Contract-bracket crate the bump simulation moves on its own. */
const CONTRACT_CRATE = "nexus-workflow-ir";
const CONTRACT_MANIFEST = "crates/nexus-workflow-ir/Cargo.toml";
/** A contract patch bump; the product surfaces stay at the fixture baseline. */
const CONTRACT_BUMP = "9.9.1";

/**
 * Move one contract crate's manifest literal and its lock row to `version`,
 * leaving every product surface alone.
 *
 * @param {string} dir
 * @param {string} version
 */
function bumpContractCrate(dir, version) {
  mutateRepoFile(dir, CONTRACT_MANIFEST, (contents) =>
    contents.replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`),
  );
  mutateRepoFile(dir, CARGO_LOCK_PATH, (contents) =>
    contents.replace(
      new RegExp(`(\\[\\[package\\]\\]\\nname = "${CONTRACT_CRATE}"\\nversion = ")[^"]+(")`),
      `$1${version}$2`,
    ),
  );
}

test("a contract crate bumped alone leaves product lockstep green", () => {
  const dir = createTempRepo();
  try {
    bumpContractCrate(dir, CONTRACT_BUMP);
    const report = checkLockstep(dir);
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.version, FIXTURE_BASELINE_VERSION);
    assert.equal(
      parseCargoLockPackageVersion(readRepoFile(dir, CARGO_LOCK_PATH), CONTRACT_CRATE),
      CONTRACT_BUMP,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a contract crate back on version.workspace = true is rejected", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, CONTRACT_MANIFEST, (contents) =>
      contents.replace(/^version\s*=\s*"[^"]*"/m, "version.workspace = true"),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(CONTRACT_MANIFEST) &&
          problem.includes("expected a literal version"),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a non-contract member with a literal version is rejected even at the product version", () => {
  const dir = createTempRepo();
  try {
    const manifestPath = "crates/nexus-core/Cargo.toml";
    // The same number as the product: independence is the declaration form, not
    // the value, so this must still fail.
    mutateRepoFile(dir, manifestPath, (contents) =>
      contents.replace(
        /^version\.workspace\s*=\s*true\s*$/m,
        `version = "${FIXTURE_BASELINE_VERSION}"`,
      ),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(manifestPath) && problem.includes("declares literal version"),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

/** Contract crate outside the root workspace; it pins itself in its own lock. */
const STANDALONE_CRATE = "nexus-module-sdk";
const STANDALONE_LOCK = "modules/nexus-module-sdk/Cargo.lock";

test("the contract bracket reports the lockfile that owns each crate", () => {
  const dir = createTempRepo();
  try {
    const report = checkLockstep(dir);
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.deepEqual(
      report.contract.map((row) => row.name).sort(),
      readVersionBrackets(dir)
        .map((entry) => entry.name)
        .sort(),
    );
    const byName = new Map(report.contract.map((row) => [row.name, row]));
    assert.equal(byName.get(CONTRACT_CRATE)?.lock, CARGO_LOCK_PATH);
    assert.equal(byName.get(STANDALONE_CRATE)?.lock, STANDALONE_LOCK);

    // The exemption is printed, not silent: a reader sees which lock pins the
    // crates the root lockfile does not carry.
    const run = runReleaseScript("assert-lockstep-version.mjs", [], dir);
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, new RegExp(STANDALONE_LOCK));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("dropping a publishable standalone crate from the carrier is rejected", () => {
  const dir = createTempRepo();
  try {
    const carrier = JSON.parse(readRepoFile(dir, VERSION_BRACKETS_PATH));
    carrier.contract = carrier.contract.filter(
      (entry) => entry.name !== STANDALONE_CRATE,
    );
    writeRepoFile(dir, VERSION_BRACKETS_PATH, `${JSON.stringify(carrier, null, 2)}\n`);

    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(STANDALONE_CRATE) &&
          problem.includes("publish = true") &&
          problem.includes(VERSION_BRACKETS_PATH),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a standalone contract crate is checked against its own lockfile", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, STANDALONE_LOCK, (contents) =>
      contents.replace(
        new RegExp(
          `(\\[\\[package\\]\\]\\nname = "${STANDALONE_CRATE}"\\nversion = ")[^"]+(")`,
        ),
        "$19.9.2$2",
      ),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(STANDALONE_LOCK) &&
          problem.includes(STANDALONE_CRATE) &&
          problem.includes("9.9.2"),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a standalone contract crate with no lockfile of its own is rejected", () => {
  const dir = createTempRepo();
  try {
    removeRepoFile(dir, STANDALONE_LOCK);
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(`${STANDALONE_LOCK}: missing`) &&
          problem.includes(STANDALONE_CRATE),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a contract manifest literal that disagrees with its lock row is rejected", () => {
  const dir = createTempRepo();
  try {
    // The manifest moves alone: the lock row keeps the version the crate was
    // registered at, which is the disagreement a bump would have to fix.
    mutateRepoFile(dir, CONTRACT_MANIFEST, (contents) =>
      contents.replace(/^version\s*=\s*"[^"]*"/m, `version = "${CONTRACT_BUMP}"`),
    );
    const report = checkLockstep(dir);
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some(
        (problem) =>
          problem.includes(
            `contract crate ${CONTRACT_CRATE} pinned at ${FIXTURE_BASELINE_VERSION}`,
          ) && problem.includes(`literal ${CONTRACT_BUMP}`),
      ),
      report.problems.join("; "),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

