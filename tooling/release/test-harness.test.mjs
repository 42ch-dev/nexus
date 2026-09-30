import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CARGO_LOCK_PATH,
  ROOT_PACKAGE_PATH,
  parseCargoLockPackageVersion,
  readSurfaceVersions,
  readWorkspaceMemberPackages,
} from "./version-surfaces.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  FIXTURE_BASELINE_VERSION,
  readRepoFile,
} from "./test-harness.mjs";

/** Assert every version surface of a fixture repo carries `version`. */
function assertFixtureVersion(dir, version) {
  for (const surface of readSurfaceVersions(dir)) {
    assert.equal(surface.version, version, surface.path);
  }
  const lock = readRepoFile(dir, CARGO_LOCK_PATH);
  const members = readWorkspaceMemberPackages(dir);
  assert.ok(members.length > 20, `expected > 20 members, got ${members.length}`);
  for (const member of members) {
    assert.equal(parseCargoLockPackageVersion(lock, member.name), version, member.name);
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
  // A source whose surfaces carry neither the baseline nor an obvious value:
  // the fixture must still come out at the pinned baseline, which is what lets
  // every test hard-code FIXTURE_BASELINE_VERSION / FIXTURE_TARGET_VERSION
  // independently of the live release number.
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
