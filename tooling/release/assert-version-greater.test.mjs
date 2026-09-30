import assert from "node:assert/strict";
import { test } from "node:test";
import { ROOT_PACKAGE_PATH } from "./version-surfaces.mjs";
import { checkVersionGreater } from "./assert-version-greater.mjs";
import {
  cleanupTempRepo,
  commitFile,
  createTempRepo,
  FIXTURE_BASELINE_VERSION,
  FIXTURE_TARGET_VERSION,
  gitOk,
  initGitRepo,
  readRepoFile,
  runReleaseScript,
  tagRelease,
} from "./test-harness.mjs";

/** `package.json` of a fixture repo with its version line rewritten. */
function packageJsonWith(dir, replace) {
  return readRepoFile(dir, ROOT_PACKAGE_PATH).replace(/"version": "[^"]*"/, replace);
}

test("checkVersionGreater accepts a strictly greater, untagged version", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const report = checkVersionGreater({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.referenceVersion, FIXTURE_BASELINE_VERSION);
    assert.equal(report.referenceRef, "origin/main");
    assert.equal(report.tagExists, false);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkVersionGreater rejects invalid, equal, and lower versions", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    for (const target of [
      "9.8",
      `v${FIXTURE_TARGET_VERSION}`,
      `${FIXTURE_TARGET_VERSION}-rc.1`,
      `${FIXTURE_TARGET_VERSION}+build.1`,
      FIXTURE_BASELINE_VERSION,
    ]) {
      const report = checkVersionGreater({ repoRoot: dir, target });
      assert.equal(report.ok, false, target);
    }
    const lower = checkVersionGreater({ repoRoot: dir, target: "0.0.1" });
    assert.equal(lower.ok, false);
    assert.ok(lower.problems.some((problem) => problem.includes("must be greater than")));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkVersionGreater rejects an existing v<version> tag", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    tagRelease(dir, `v${FIXTURE_TARGET_VERSION}`);
    const report = checkVersionGreater({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, false);
    assert.equal(report.tagExists, true);
    assert.ok(report.problems.some((problem) => problem.includes("already exists")));
    // An unrelated tag does not block the release (exact tag match only).
    tagRelease(dir, "v9.8.9");
    assert.equal(checkVersionGreater({ repoRoot: dir, target: "9.8.10" }).ok, true);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkVersionGreater compares against origin/<base-ref>, not the working tree", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    commitFile(dir, ROOT_PACKAGE_PATH, packageJsonWith(dir, '"version": "9.9.9"'), "release 9.9.9");
    gitOk(dir, ["update-ref", "refs/remotes/origin/main", "HEAD"]);
    // Working tree still advertises the baseline; origin/main does not.
    gitOk(dir, ["checkout", "HEAD~1", "--", ROOT_PACKAGE_PATH]);
    assert.equal(
      JSON.parse(readRepoFile(dir, ROOT_PACKAGE_PATH)).version,
      FIXTURE_BASELINE_VERSION,
    );

    const report = checkVersionGreater({ repoRoot: dir, target: FIXTURE_TARGET_VERSION });
    assert.equal(report.ok, false);
    assert.equal(report.referenceVersion, "9.9.9");
    assert.ok(report.problems.some((problem) => problem.includes("must be greater than 9.9.9")));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkVersionGreater honours --base-ref and fails without the ref", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    gitOk(dir, ["update-ref", "refs/remotes/origin/release-line", "HEAD"]);
    assert.equal(
      checkVersionGreater({ repoRoot: dir, target: FIXTURE_TARGET_VERSION, baseRef: "release-line" })
        .ok,
      true,
    );
    const missing = checkVersionGreater({
      repoRoot: dir,
      target: FIXTURE_TARGET_VERSION,
      baseRef: "nope",
    });
    assert.equal(missing.ok, false);
    assert.ok(missing.problems.some((problem) => problem.includes("cannot read origin/nope")));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI exit codes and messages", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const ok = runReleaseScript("assert-version-greater.mjs", [FIXTURE_TARGET_VERSION], dir);
    assert.equal(ok.status, 0, ok.stderr);
    assert.ok(
      ok.stdout.includes(
        `Version OK: ${FIXTURE_TARGET_VERSION} > ${FIXTURE_BASELINE_VERSION} (origin/main); ` +
          `v${FIXTURE_TARGET_VERSION} does not exist`,
      ),
      ok.stdout,
    );

    const duplicate = runReleaseScript(
      "assert-version-greater.mjs",
      [FIXTURE_BASELINE_VERSION],
      dir,
    );
    assert.equal(duplicate.status, 1);
    assert.match(duplicate.stderr, /equals the origin\/main version/);

    const invalid = runReleaseScript("assert-version-greater.mjs", ["not-a-version"], dir);
    assert.equal(invalid.status, 1);
    assert.match(invalid.stderr, /Invalid version "not-a-version"/);

    const usage = runReleaseScript("assert-version-greater.mjs", ["--help"], dir);
    assert.equal(usage.status, 0);
    assert.match(usage.stdout, /Usage: node tooling\/release\/assert-version-greater\.mjs/);

    const noArgs = runReleaseScript("assert-version-greater.mjs", [], dir);
    assert.equal(noArgs.status, 1);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI fails cleanly when origin/main is not fetched", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("assert-version-greater.mjs", [FIXTURE_TARGET_VERSION], dir);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /cannot read origin\/main:package\.json/);
  } finally {
    cleanupTempRepo(dir);
  }
});
