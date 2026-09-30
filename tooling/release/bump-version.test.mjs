import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  LOCKSTEP_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  readSurfaceVersions,
  readWorkspaceMemberPackages,
} from "./version-surfaces.mjs";
import { bumpVersion } from "./bump-version.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  readRepoFile,
  REPO_ROOT,
  runReleaseScript,
  writeRepoFile,
} from "./test-harness.mjs";

const CURRENT_VERSION = JSON.parse(readRepoFile(REPO_ROOT, ROOT_PACKAGE_PATH)).version;

test("bumpVersion writes all five surfaces and reports the change", () => {
  const dir = createTempRepo();
  try {
    const report = bumpVersion({ repoRoot: dir, target: "0.2.0" });
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.from, CURRENT_VERSION);
    assert.deepEqual(
      report.changed.map((entry) => entry.path),
      [...LOCKSTEP_PATHS, CARGO_LOCK_PATH],
    );
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, "0.2.0", surface.path);
    }
    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    for (const member of readWorkspaceMemberPackages(dir)) {
      assert.equal(parseCargoLockPackageVersion(lock, member.name), "0.2.0", member.name);
    }
    // The standalone (non-workspace) module crate must not be dragged along.
    assert.equal(
      parseCargoLockPackageVersion(readRepoFile(REPO_ROOT, CARGO_LOCK_PATH), "nexus-module-manifest"),
      parseCargoLockPackageVersion(lock, "nexus-module-manifest"),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion refuses non-clean, equal, and decreasing versions", () => {
  const dir = createTempRepo();
  try {
    for (const target of ["0.2", "v0.2.0", "0.2.0-rc.1", CURRENT_VERSION, "0.0.1"]) {
      const report = bumpVersion({ repoRoot: dir, target });
      assert.equal(report.ok, false, target);
      assert.deepEqual(report.changed, [], target);
    }
    // Nothing was written by the refused attempts.
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, CURRENT_VERSION, surface.path);
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion reports a broken Cargo.lock instead of writing a partial bump", () => {
  const dir = createTempRepo();
  try {
    const lock = readRepoFile(dir, CARGO_LOCK_PATH).replace(
      /\[\[package\]\]\nname = "nexus42"\nversion = "[^"]+"\n/,
      "",
    );
    writeRepoFile(dir, CARGO_LOCK_PATH, lock);
    const report = bumpVersion({ repoRoot: dir, target: "0.2.0" });
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some((problem) => problem.includes("missing [[package]] entry for nexus42")),
      report.problems.join("; "),
    );
    // All-or-nothing: no version surface was written.
    assert.deepEqual(report.changed, []);
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, CURRENT_VERSION, surface.path);
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI reports a broken Cargo.lock with a non-zero exit", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(
      dir,
      CARGO_LOCK_PATH,
      readRepoFile(dir, CARGO_LOCK_PATH).replace(
        /\[\[package\]\]\nname = "nexus42"\nversion = "[^"]+"\n/,
        "",
      ),
    );
    const run = runReleaseScript("bump-version.mjs", ["0.2.0"], dir);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /missing \[\[package\]\] entry for nexus42/);
    assert.equal(JSON.parse(readRepoFile(dir, ROOT_PACKAGE_PATH)).version, CURRENT_VERSION);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI bumps and prints next steps", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("bump-version.mjs", ["0.2.0"], dir);
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, /Bumped 0\.1\.0 -> 0\.2\.0/);
    assert.match(run.stdout, /Lockstep OK: 0\.2\.0/);
    assert.match(run.stdout, /git commit -m "chore\(release\): bump version to 0\.2\.0"/);
    assert.match(run.stdout, /generate-changelog\.mjs --version 0\.2\.0 --prepend CHANGELOG\.md/);
    const surface = JSON.parse(readRepoFile(dir, PRODUCT_JSON_PATH));
    assert.equal(surface.version, "0.2.0");
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI refuses a duplicate version with a clear error", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("bump-version.mjs", [CURRENT_VERSION], dir);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /must be greater than the current/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI --help exits zero and documents the surfaces", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("bump-version.mjs", ["--help"], dir);
    assert.equal(run.status, 0);
    assert.match(run.stdout, /Usage: node tooling\/release\/bump-version\.mjs/);
    assert.match(run.stdout, /Cargo\.lock/);
  } finally {
    cleanupTempRepo(dir);
  }
});
