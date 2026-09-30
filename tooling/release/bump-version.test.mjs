import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  CARGO_LOCK_REGEN_COMMAND,
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
  FIXTURE_BASELINE_VERSION,
  FIXTURE_TARGET_VERSION,
  readRepoFile,
  runReleaseScript,
  writeRepoFile,
} from "./test-harness.mjs";

/** Drop `nexus42` from a fixture lockfile so the pre-flight check must trip. */
const brokenLockfile = (dir) =>
  readRepoFile(dir, CARGO_LOCK_PATH).replace(
    /\[\[package\]\]\nname = "nexus42"\nversion = "[^"]+"\n/,
    "",
  );

test("bumpVersion writes all five surfaces and reports the change", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    const report = bumpVersion({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.from, FIXTURE_BASELINE_VERSION);
    assert.deepEqual(
      report.changed.map((entry) => entry.path),
      [...LOCKSTEP_PATHS, CARGO_LOCK_PATH],
    );
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_TARGET_VERSION, surface.path);
    }

    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    const members = readWorkspaceMemberPackages(dir);
    assert.ok(members.length > 20, `expected > 20 members, got ${members.length}`);
    for (const member of members) {
      assert.equal(parseCargoLockPackageVersion(lock, member.name), FIXTURE_TARGET_VERSION, member.name);
    }
    assert.equal(report.changed.at(-1).members, members.length);

    // Cargo, not a text edit, produced the lockfile: only the workspace member
    // version lines moved, and nothing was added or dropped.
    const before = lockBefore.split("\n");
    const after = lock.split("\n");
    assert.equal(after.length, before.length);
    const changedLines = after.filter((line, index) => line !== before[index]);
    assert.equal(changedLines.length, members.length);
    for (const line of changedLines) {
      assert.equal(line, `version = "${FIXTURE_TARGET_VERSION}"`);
    }

    // The standalone (non-workspace) module crate must not be dragged along.
    assert.equal(
      parseCargoLockPackageVersion(lock, "nexus-module-manifest"),
      parseCargoLockPackageVersion(lockBefore, "nexus-module-manifest"),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion refuses non-clean, equal, and decreasing versions", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    for (const target of [
      "9.8",
      `v${FIXTURE_TARGET_VERSION}`,
      `${FIXTURE_TARGET_VERSION}-rc.1`,
      FIXTURE_BASELINE_VERSION,
      "0.0.1",
    ]) {
      const report = bumpVersion({ repoRoot: dir, target });
      assert.equal(report.ok, false, target);
      assert.deepEqual(report.changed, [], target);
    }
    // Nothing was written by the refused attempts.
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_BASELINE_VERSION, surface.path);
    }
    assert.equal(readRepoFile(dir, CARGO_LOCK_PATH), lockBefore);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion reports a broken Cargo.lock instead of writing a partial bump", () => {
  const dir = createTempRepo();
  try {
    const lock = brokenLockfile(dir);
    writeRepoFile(dir, CARGO_LOCK_PATH, lock);
    const report = bumpVersion({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, false);
    assert.ok(
      report.problems.some((problem) => problem.includes("missing [[package]] entry for nexus42")),
      report.problems.join("; "),
    );
    // All-or-nothing: no version surface was written.
    assert.deepEqual(report.changed, []);
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_BASELINE_VERSION, surface.path);
    }
    assert.equal(readRepoFile(dir, CARGO_LOCK_PATH), lock);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI reports a broken Cargo.lock with a non-zero exit", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, CARGO_LOCK_PATH, brokenLockfile(dir));
    const run = runReleaseScript("bump-version.mjs", [FIXTURE_TARGET_VERSION], dir);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /missing \[\[package\]\] entry for nexus42/);
    assert.equal(
      JSON.parse(readRepoFile(dir, ROOT_PACKAGE_PATH)).version,
      FIXTURE_BASELINE_VERSION,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI fails loudly and restores the surfaces when cargo is unavailable", () => {
  const dir = createTempRepo();
  try {
    const before = LOCKSTEP_PATHS.map((path) => [path, readRepoFile(dir, path)]);
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    const run = runReleaseScript("bump-version.mjs", [FIXTURE_TARGET_VERSION], dir, {
      env: { PATH: "/nonexistent-cargo" },
    });
    assert.equal(run.status, 1);
    assert.match(run.stderr, /could not run/);
    assert.ok(run.stderr.includes(CARGO_LOCK_REGEN_COMMAND), run.stderr);
    // Rolled back: a failed regeneration never leaves a half-bumped repo.
    for (const [path, contents] of before) {
      assert.equal(readRepoFile(dir, path), contents, path);
    }
    assert.equal(readRepoFile(dir, CARGO_LOCK_PATH), lockBefore);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI bumps and prints next steps", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("bump-version.mjs", [FIXTURE_TARGET_VERSION], dir);
    assert.equal(run.status, 0, run.stderr);
    assert.ok(
      run.stdout.includes(`Bumped ${FIXTURE_BASELINE_VERSION} -> ${FIXTURE_TARGET_VERSION}`),
      run.stdout,
    );
    assert.ok(run.stdout.includes(`Lockstep OK: ${FIXTURE_TARGET_VERSION}`), run.stdout);
    assert.ok(
      run.stdout.includes(
        `git commit -m "chore(release): bump version to ${FIXTURE_TARGET_VERSION}"`,
      ),
      run.stdout,
    );
    assert.ok(
      run.stdout.includes(
        `generate-changelog.mjs --version ${FIXTURE_TARGET_VERSION} --prepend CHANGELOG.md`,
      ),
      run.stdout,
    );
    const surface = JSON.parse(readRepoFile(dir, PRODUCT_JSON_PATH));
    assert.equal(surface.version, FIXTURE_TARGET_VERSION);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI refuses a duplicate version with a clear error", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("bump-version.mjs", [FIXTURE_BASELINE_VERSION], dir);
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
