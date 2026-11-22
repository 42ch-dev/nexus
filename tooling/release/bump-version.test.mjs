import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  CARGO_LOCK_REGEN_COMMAND,
  LOCKSTEP_PATHS,
  NATIVE_SURFACE_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  readSurfaceVersions,
  readVersionBrackets,
  readWorkspaceMemberPackages,
} from "./version-surfaces.mjs";
import { bumpVersion } from "./bump-version.mjs";
import { checkLockstep } from "./assert-lockstep-version.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  expectedMemberLockVersions,
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

test("bumpVersion writes every version surface and reports the change", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    const report = bumpVersion({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.from, FIXTURE_BASELINE_VERSION);
    assert.deepEqual(
      report.changed.map((entry) => entry.path),
      [...LOCKSTEP_PATHS, ...NATIVE_SURFACE_PATHS, CARGO_LOCK_PATH],
    );
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_TARGET_VERSION, surface.path);
    }

    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    const members = readWorkspaceMemberPackages(dir);
    assert.ok(members.length > 20, `expected > 20 members, got ${members.length}`);
    // crates kept the literal their manifest still declares.
    const expected = expectedMemberLockVersions(dir, FIXTURE_TARGET_VERSION);
    for (const [name, version] of expected) {
      assert.equal(parseCargoLockPackageVersion(lock, name), version, name);
    }
    assert.equal(report.changed.at(-1).members, members.length);

    // version lines moved, and nothing was added or dropped.
    const before = lockBefore.split("\n");
    const after = lock.split("\n");
    assert.equal(after.length, before.length);
    const changedLines = after.filter((line, index) => line !== before[index]);
    const bumpedMembers = [...expected.values()].filter(
      (version) => version === FIXTURE_TARGET_VERSION,
    ).length;
    assert.equal(changedLines.length, bumpedMembers);
    for (const line of changedLines) {
      assert.equal(line, `version = "${FIXTURE_TARGET_VERSION}"`);
    }

    assert.equal(
      parseCargoLockPackageVersion(lock, "nexus-module-manifest"),
      parseCargoLockPackageVersion(lockBefore, "nexus-module-manifest"),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a product bump leaves every independent package literal and lock row alone", () => {
  const dir = createTempRepo();
  try {
    const entries = readVersionBrackets(dir);
    assert.equal(entries.length, 8);
    const manifestsBefore = new Map(
      entries.map((entry) => [entry.manifest, readRepoFile(dir, entry.manifest)]),
    );
    const report = bumpVersion({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, true, report.problems.join("; "));

    for (const entry of entries) {
      assert.equal(
        readRepoFile(dir, entry.manifest),
        manifestsBefore.get(entry.manifest),
        `${entry.name}: a product bump must not rewrite an independent package manifest`,
      );
    }
    // of the lockfile that owns the crate: the root workspace for a member, the
    // standalone crate's own lock otherwise.
    const lockstep = checkLockstep(dir);
    assert.equal(lockstep.ok, true, lockstep.problems.join("; "));
    assert.equal(lockstep.contract.length, entries.length);
    for (const row of lockstep.contract) {
      assert.equal(row.version, FIXTURE_BASELINE_VERSION, row.name);
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion refuses malformed, equal, and decreasing versions", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    for (const target of [
      "9.8",
      `v${FIXTURE_TARGET_VERSION}`,
      `${FIXTURE_TARGET_VERSION}-beta.1`,
      `${FIXTURE_TARGET_VERSION}-rc.0`,
      FIXTURE_BASELINE_VERSION,
      "0.0.1",
    ]) {
      const report = bumpVersion({ repoRoot: dir, target });
      assert.equal(report.ok, false, target);
      assert.deepEqual(report.changed, [], target);
    }
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_BASELINE_VERSION, surface.path);
    }
    assert.equal(readRepoFile(dir, CARGO_LOCK_PATH), lockBefore);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("bumpVersion walks the clean→alpha→rc→stable ladder and refuses a downgrade", () => {
  const dir = createTempRepo();
  /** Every surface, and every `Cargo.lock` row its bracket requires, at `target`. */
  const assertSurfacesAt = (target) => {
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, target, `${target} ${surface.path}`);
    }
    const lock = readRepoFile(dir, CARGO_LOCK_PATH);
    for (const [name, version] of expectedMemberLockVersions(dir, target)) {
      assert.equal(parseCargoLockPackageVersion(lock, name), version, `${target} ${name}`);
    }
  };
  try {
    for (const target of ["9.8.8-alpha.1", "9.8.8-alpha.2", "9.8.8-rc.1", "9.8.8-rc.2"]) {
      const report = bumpVersion({ repoRoot: dir, target });
      assert.equal(report.ok, true, `${target}: ${report.problems.join("; ")}`);
      assert.equal(report.lockstep?.ok, true, target);
      assertSurfacesAt(target);
    }

    const downgrade = bumpVersion({ repoRoot: dir, target: "9.8.8-alpha.4" });
    assert.equal(downgrade.ok, false);
    assert.deepEqual(downgrade.changed, []);
    assert.ok(
      downgrade.problems.some((problem) =>
        problem.includes("must be greater than the current 9.8.8-rc.2"),
      ),
      downgrade.problems.join("; "),
    );
    assertSurfacesAt("9.8.8-rc.2");

    const stable = bumpVersion({ repoRoot: dir, target: "9.8.8" });
    assert.equal(stable.ok, true, stable.problems.join("; "));
    assertSurfacesAt("9.8.8");
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
    const before = [...LOCKSTEP_PATHS, ...NATIVE_SURFACE_PATHS].map((path) => [
      path,
      readRepoFile(dir, path),
    ]);
    const lockBefore = readRepoFile(dir, CARGO_LOCK_PATH);
    const run = runReleaseScript("bump-version.mjs", [FIXTURE_TARGET_VERSION], dir, {
      env: { PATH: "/nonexistent-cargo" },
    });
    assert.equal(run.status, 1);
    assert.match(run.stderr, /could not run/);
    assert.ok(run.stderr.includes(CARGO_LOCK_REGEN_COMMAND), run.stderr);
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
