import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
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
  replaceCargoLockMemberVersions,
  replaceJsonVersion,
  replaceWorkspacePackageVersion,
  writeReleaseVersion,
} from "./version-surfaces.mjs";
import {
  createTempRepo,
  cleanupTempRepo,
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

test("Cargo.lock rewrite touches workspace members only", () => {
  const lock = readRepoFile(REPO_ROOT, CARGO_LOCK_PATH);
  const members = readWorkspaceMemberPackages(REPO_ROOT).map((member) => member.name);
  const updated = replaceCargoLockMemberVersions(lock, "9.9.9", members);

  for (const name of members) {
    assert.equal(parseCargoLockPackageVersion(updated, name), "9.9.9", name);
  }
  // Non-member packages keep their versions: `nexus-module-manifest` is a
  // standalone (excluded) crate, `leb128fmt` an unrelated dependency.
  for (const name of ["nexus-module-manifest", "leb128fmt"]) {
    assert.equal(
      parseCargoLockPackageVersion(updated, name),
      parseCargoLockPackageVersion(lock, name),
      name,
    );
  }
  assert.equal(updated.split("\n").length, lock.split("\n").length);
  assert.equal(replaceCargoLockMemberVersions(updated, "9.9.9", members), updated);
});

test("Cargo.lock rewrite fails when a member entry is missing", () => {
  const lock = readRepoFile(REPO_ROOT, CARGO_LOCK_PATH);
  assert.throws(
    () => replaceCargoLockMemberVersions(lock, "9.9.9", ["nexus-not-a-member"]),
    /missing \[\[package\]\] entry for nexus-not-a-member/,
  );
});

test("writeReleaseVersion bumps all five surfaces", () => {
  const dir = createTempRepo();
  try {
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
    for (const member of readWorkspaceMemberPackages(dir)) {
      assert.equal(parseCargoLockPackageVersion(lock, member.name), "9.9.9", member.name);
    }
  } finally {
    cleanupTempRepo(dir);
  }
});
