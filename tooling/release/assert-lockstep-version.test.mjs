import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  NATIVE_LOADER_PACKAGE_PATH,
  NATIVE_SURFACE_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  readWorkspaceMemberPackages,
} from "./version-surfaces.mjs";
import { checkLockstep } from "./assert-lockstep-version.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  FIXTURE_BASELINE_VERSION,
  mutateRepoFile,
  readRepoFile,
  removeRepoFile,
  runReleaseScript,
} from "./test-harness.mjs";

test("checkLockstep accepts the fixture surface set", () => {
  const dir = createTempRepo();
  try {
    const report = checkLockstep(dir);
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.version, FIXTURE_BASELINE_VERSION);
    assert.equal(report.surfaces.length, 4 + NATIVE_SURFACE_PATHS.length);
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

test("checkLockstep reports a drifted native platform pin", () => {
  const dir = createTempRepo();
  try {
    mutateRepoFile(dir, NATIVE_LOADER_PACKAGE_PATH, (contents) =>
      contents.replace(
        /"@42ch\/nexus-native-darwin-arm64": "[^"]*"/,
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

test("every workspace member resolves to a lock entry at the fixture version", () => {
  const dir = createTempRepo();
  try {
    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    const members = readWorkspaceMemberPackages(dir);
    assert.ok(members.length > 20, `expected > 20 members, got ${members.length}`);
    for (const member of members) {
      assert.equal(
        parseCargoLockPackageVersion(lock, member.name),
        FIXTURE_BASELINE_VERSION,
        member.name,
      );
    }
  } finally {
    cleanupTempRepo(dir);
  }
});
