import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  CARGO_LOCK_PATH,
  ELECTRON_PACKAGE_PATH,
  LOCKSTEP_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  parseCargoPackageName,
  parseCargoWorkspaceMembers,
  parseJsonVersion,
  parseWorkspacePackageVersion,
  readSurfaceVersion,
  readSurfaceVersions,
  readWorkspaceMemberPackages,
  replaceJsonVersion,
  replaceWorkspacePackageVersion,
  writeReleaseVersion,
} from "./version-surfaces.mjs";
import {
  createTempRepo,
  cleanupTempRepo,
  FIXTURE_BASELINE_VERSION,
  readRepoFile,
  REPO_ROOT,
} from "./test-harness.mjs";

const currentVersion = JSON.parse(readRepoFile(REPO_ROOT, ROOT_PACKAGE_PATH)).version;

test("the four declared surfaces are equal in the real repository", () => {
  const surfaces = readSurfaceVersions(REPO_ROOT);
  assert.deepEqual(
    surfaces.map((surface) => surface.path),
    [...LOCKSTEP_PATHS],
  );
  for (const surface of surfaces) {
    assert.equal(surface.version, currentVersion, surface.path);
  }
  assert.equal(readSurfaceVersion(REPO_ROOT, PRODUCT_JSON_PATH), currentVersion);
});

test("parseWorkspacePackageVersion reads [workspace.package] only", () => {
  const contents = [
    "[package]",
    'name = "other"',
    'version = "9.9.9"',
    "",
    "[workspace.package]",
    'version = "0.1.0"',
    'edition = "2021"',
    "",
    "[workspace.dependencies]",
    'serde = "1"',
    "",
  ].join("\n");
  assert.equal(parseWorkspacePackageVersion(contents), "0.1.0");
  assert.throws(
    () => parseWorkspacePackageVersion("[package]\nname = \"x\"\n"),
    /missing \[workspace\.package\] section/,
  );
});

test("replaceWorkspacePackageVersion rewrites only the workspace version line", () => {
  const original = readRepoFile(REPO_ROOT, "Cargo.toml");
  const updated = replaceWorkspacePackageVersion(original, "9.9.9");
  assert.equal(
    updated,
    original.replace(/(\[workspace\.package\][\s\S]*?)version = "[^"]*"/, '$1version = "9.9.9"'),
  );
  assert.equal(parseWorkspacePackageVersion(updated), "9.9.9");
});

test("replaceJsonVersion changes one line and keeps canonical formatting", () => {
  for (const path of [ROOT_PACKAGE_PATH, ELECTRON_PACKAGE_PATH, PRODUCT_JSON_PATH]) {
    const original = readRepoFile(REPO_ROOT, path);
    const updated = replaceJsonVersion(original, "9.9.9", path);
    assert.equal(updated, original.replace(/"version": "[^"]*"/, '"version": "9.9.9"'), path);
    assert.equal(parseJsonVersion(updated, path), "9.9.9");
    assert.ok(updated.endsWith("\n"), path);
  }
});

test("replaceJsonVersion refuses non-canonical JSON instead of reformatting", () => {
  assert.throws(
    () => replaceJsonVersion('{"version":"0.1.0"}', "0.2.0", "package.json"),
    /not canonical 2-space JSON/,
  );
});

test("parseJsonVersion rejects files without a version string", () => {
  assert.throws(() => parseJsonVersion("{}", "package.json"), /missing "version"/);
  assert.throws(() => parseJsonVersion("{", "package.json"), /invalid JSON/);
});

test("workspace members parse from the real root manifest", () => {
  const members = parseCargoWorkspaceMembers(readRepoFile(REPO_ROOT, "Cargo.toml"));
  assert.ok(members.length > 20, `expected many members, got ${members.length}`);
  assert.ok(members.includes("apps/nexus42"));
  for (const member of members) {
    assert.ok(!/[*?[\]]/.test(member), member);
  }
});

test("package names parse and members resolve to lock entries", () => {
  assert.equal(
    parseCargoPackageName("[package]\nname = \"nexus-core\"\nversion.workspace = true\n"),
    "nexus-core",
  );
  assert.throws(() => parseCargoPackageName("[dependencies]\nfoo = \"1\"\n"), /missing \[package\]\.name/);

  const members = readWorkspaceMemberPackages(REPO_ROOT);
  const lock = readRepoFile(REPO_ROOT, CARGO_LOCK_PATH);
  for (const member of members) {
    assert.equal(parseCargoLockPackageVersion(lock, member.name), currentVersion, member.name);
  }
});

test("writeReleaseVersion regenerates Cargo.lock through Cargo", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8");
    const changed = writeReleaseVersion(dir, "9.9.9");
    assert.deepEqual(
      changed.map((entry) => entry.path),
      [...LOCKSTEP_PATHS, CARGO_LOCK_PATH],
    );
    assert.equal(changed.at(-1).members, readWorkspaceMemberPackages(dir).length);
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, "9.9.9", surface.path);
    }

    const lock = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8");
    const members = readWorkspaceMemberPackages(dir);
    for (const member of members) {
      assert.equal(parseCargoLockPackageVersion(lock, member.name), "9.9.9", member.name);
    }
    // Non-member packages keep their versions: `nexus-module-manifest` is a
    // standalone (excluded) crate, `leb128fmt` an unrelated dependency.
    for (const name of ["nexus-module-manifest", "leb128fmt"]) {
      assert.equal(
        parseCargoLockPackageVersion(lock, name),
        parseCargoLockPackageVersion(lockBefore, name),
        name,
      );
    }
    assert.equal(lock.split("\n").length, lockBefore.split("\n").length);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("writeReleaseVersion refuses a lockfile missing a workspace member entry", () => {
  const dir = createTempRepo();
  try {
    const lock = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8").replace(
      /\[\[package\]\]\nname = "nexus42"\nversion = "[^"]+"\n/,
      "",
    );
    writeFileSync(join(dir, CARGO_LOCK_PATH), lock, "utf8");

    assert.throws(
      () => writeReleaseVersion(dir, "9.9.9"),
      /missing \[\[package\]\] entry for nexus42/,
    );
    // All-or-nothing: the refused call wrote nothing.
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_BASELINE_VERSION, surface.path);
    }
    assert.equal(readFileSync(join(dir, CARGO_LOCK_PATH), "utf8"), lock);
  } finally {
    cleanupTempRepo(dir);
  }
});
