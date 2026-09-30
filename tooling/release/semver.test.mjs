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
  assert.equal(CLEAN_SEMVER_PATTERN.source, "^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)$");
});

test("isCleanSemVer accepts canonical X.Y.Z and rejects leading zeros", () => {
  for (const version of ["0.1.0", "0.2.0", "1.0.0", "10.20.30"]) {
    assert.equal(isCleanSemVer(version), true, version);
  }
  for (const version of [
    "01.2.3",
    "1.02.3",
    "1.2.03",
    "00.0.0",
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

test("parseSemVer returns canonical digit parts for clean versions and null otherwise", () => {
  assert.deepEqual(parseSemVer("1.20.300"), { major: "1", minor: "20", patch: "300" });
  assert.deepEqual(parseSemVer("0.0.0"), { major: "0", minor: "0", patch: "0" });
  assert.equal(parseSemVer("01.20.300"), null);
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

test("compareSemVer keeps components beyond Number.MAX_SAFE_INTEGER distinct", () => {
  const huge = "9007199254740993.0.0"; // MAX_SAFE_INTEGER + 2, lossy as a Number
  const hugeNeighbour = "9007199254740992.0.0"; // MAX_SAFE_INTEGER + 1
  assert.ok(compareSemVer(huge, hugeNeighbour) > 0);
  assert.ok(compareSemVer(hugeNeighbour, huge) < 0);
  assert.equal(compareSemVer(huge, huge), 0);
  assert.ok(compareSemVer("1.0.0", huge) < 0);
  const padded = `1${"0".repeat(30)}.0.0`;
  const shorter = `${"9".repeat(30)}.0.0`;
  assert.ok(compareSemVer(padded, shorter) > 0, "length decides before lexicographic order");
  assert.ok(compareSemVer("1.9007199254740993.0", "1.9007199254740992.0") > 0);
});

test("compareSemVer throws on non-clean versions", () => {
  assert.throws(() => compareSemVer("1.2", "1.2.0"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("1.2.0", "1.2.0-rc.1"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("01.2.3", "1.2.3"), /Invalid SemVer compare/);
});

test("isSemVerGreater is strict", () => {
  assert.equal(isSemVerGreater("0.2.0", "0.1.0"), true);
  assert.equal(isSemVerGreater("0.1.0", "0.1.0"), false);
  assert.equal(isSemVerGreater("0.1.0", "0.2.0"), false);
});
