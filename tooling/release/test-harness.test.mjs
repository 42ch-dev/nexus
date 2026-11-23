import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  readSurfaceVersions,
} from "./version-surfaces.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  expectedMemberLockVersions,
  FIXTURE_BASELINE_VERSION,
  readRepoFile,
} from "./test-harness.mjs";

function assertFixtureVersion(dir, version) {
  for (const surface of readSurfaceVersions(dir)) {
    assert.equal(surface.version, version, surface.path);
  }
  const lock = readRepoFile(dir, CARGO_LOCK_PATH);
  const expected = expectedMemberLockVersions(dir, version);
  assert.ok(expected.size > 20, `expected > 20 members, got ${expected.size}`);
  for (const [name, memberVersion] of expected) {
    assert.equal(parseCargoLockPackageVersion(lock, name), memberVersion, name);
  }
}

test("fixture repos pin the fixed baseline, not the repository version", () => {
  const dir = createTempRepo();
  try {
    assertFixtureVersion(dir, FIXTURE_BASELINE_VERSION);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("fixture repos ignore the source repository version", () => {
  const source = createTempRepo({ version: "1.2.3" });
  try {
    assertFixtureVersion(source, "1.2.3");

    const dir = createTempRepo({ sourceRoot: source });
    try {
      assertFixtureVersion(dir, FIXTURE_BASELINE_VERSION);
      assert.equal(
        JSON.parse(readRepoFile(dir, ROOT_PACKAGE_PATH)).version,
        FIXTURE_BASELINE_VERSION,
      );
    } finally {
      cleanupTempRepo(dir);
    }
  } finally {
    cleanupTempRepo(source);
  }
});
