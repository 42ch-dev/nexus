import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CLEAN_SEMVER_PATTERN,
  compareSemVer,
  isCleanSemVer,
  isSemVerGreater,
  parseSemVer,
} from "./semver.mjs";

test("CLEAN_SEMVER_PATTERN matches the release version shape", () => {
  assert.equal(CLEAN_SEMVER_PATTERN.source, "^\\d+\\.\\d+\\.\\d+$");
});

test("isCleanSemVer accepts X.Y.Z only", () => {
  for (const version of ["0.1.0", "0.2.0", "1.0.0", "10.20.30"]) {
    assert.equal(isCleanSemVer(version), true, version);
  }
  for (const version of [
    "0.2",
    "0.2.0.1",
    "v0.2.0",
    "0.2.0-rc.1",
    "0.2.0+build.5",
    " 0.2.0",
    "",
    null,
    2,
  ]) {
    assert.equal(isCleanSemVer(version), false, String(version));
  }
});

test("parseSemVer returns numeric parts for clean versions and null otherwise", () => {
  assert.deepEqual(parseSemVer("1.20.300"), { major: 1, minor: 20, patch: 300 });
  assert.equal(parseSemVer("1.20"), null);
  assert.equal(parseSemVer("1.20.300-rc.1"), null);
});

test("compareSemVer orders by major, then minor, then patch", () => {
  assert.ok(compareSemVer("0.2.0", "0.1.0") > 0);
  assert.ok(compareSemVer("0.1.0", "0.2.0") < 0);
  assert.equal(compareSemVer("0.1.0", "0.1.0"), 0);
  assert.ok(compareSemVer("1.0.0", "0.99.99") > 0);
  assert.ok(compareSemVer("0.1.10", "0.1.9") > 0);
});

test("compareSemVer throws on non-clean versions", () => {
  assert.throws(() => compareSemVer("1.2", "1.2.0"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("1.2.0", "1.2.0-rc.1"), /Invalid SemVer compare/);
});

test("isSemVerGreater is strict", () => {
  assert.equal(isSemVerGreater("0.2.0", "0.1.0"), true);
  assert.equal(isSemVerGreater("0.1.0", "0.1.0"), false);
  assert.equal(isSemVerGreater("0.1.0", "0.2.0"), false);
});
