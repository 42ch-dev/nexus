import assert from "node:assert/strict";
import { test } from "node:test";
import { ROOT_PACKAGE_PATH } from "./version-surfaces.mjs";
import { checkVersionGreater } from "./assert-version-greater.mjs";
import {
  cleanupTempRepo,
  commitFile,
  createTempRepo,
  gitOk,
  initGitRepo,
  readRepoFile,
  REPO_ROOT,
  runReleaseScript,
  tagRelease,
} from "./test-harness.mjs";

const CURRENT_VERSION = JSON.parse(readRepoFile(REPO_ROOT, ROOT_PACKAGE_PATH)).version;

/** @param {(version: string) => string} replace */
function packageJsonWith(replace) {
  return readRepoFile(REPO_ROOT, ROOT_PACKAGE_PATH).replace(/"version": "[^"]*"/, replace);
}

test("checkVersionGreater accepts a strictly greater, untagged version", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const report = checkVersionGreater({ repoRoot: dir, target: "0.2.0" });
    assert.equal(report.ok, true, report.problems.join("; "));
    assert.equal(report.referenceVersion, CURRENT_VERSION);
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
    for (const target of ["0.2", "v0.2.0", "0.2.0-rc.1", "0.2.0+build.1", CURRENT_VERSION]) {
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
    tagRelease(dir, "v0.2.0");
    const report = checkVersionGreater({ repoRoot: dir, target: "0.2.0" });
    assert.equal(report.ok, false);
    assert.equal(report.tagExists, true);
    assert.ok(report.problems.some((problem) => problem.includes("already exists")));
    // An unrelated tag does not block the release (exact tag match only).
    tagRelease(dir, "v0.2.1");
    assert.equal(checkVersionGreater({ repoRoot: dir, target: "0.3.0" }).ok, true);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("checkVersionGreater compares against origin/<base-ref>, not the working tree", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    commitFile(dir, ROOT_PACKAGE_PATH, packageJsonWith('"version": "0.5.0"'), "release 0.5.0");
    gitOk(dir, ["update-ref", "refs/remotes/origin/main", "HEAD"]);
    // Working tree still advertises the older version; origin/main does not.
    gitOk(dir, ["checkout", "HEAD~1", "--", ROOT_PACKAGE_PATH]);
    assert.equal(JSON.parse(readRepoFile(dir, ROOT_PACKAGE_PATH)).version, CURRENT_VERSION);

    const report = checkVersionGreater({ repoRoot: dir, target: "0.2.0" });
    assert.equal(report.ok, false);
    assert.equal(report.referenceVersion, "0.5.0");
    assert.ok(report.problems.some((problem) => problem.includes("must be greater than 0.5.0")));
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
      checkVersionGreater({ repoRoot: dir, target: "0.2.0", baseRef: "release-line" }).ok,
      true,
    );
    const missing = checkVersionGreater({ repoRoot: dir, target: "0.2.0", baseRef: "nope" });
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
    const ok = runReleaseScript("assert-version-greater.mjs", ["0.2.0"], dir);
    assert.equal(ok.status, 0, ok.stderr);
    assert.match(ok.stdout, /Version OK: 0\.2\.0 > .* \(origin\/main\); v0\.2\.0 does not exist/);

    const duplicate = runReleaseScript("assert-version-greater.mjs", [CURRENT_VERSION], dir);
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
    const run = runReleaseScript("assert-version-greater.mjs", ["0.2.0"], dir);
    assert.equal(run.status, 1);
    assert.match(run.stderr, /cannot read origin\/main:package\.json/);
  } finally {
    cleanupTempRepo(dir);
  }
});
