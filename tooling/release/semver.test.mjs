import assert from "node:assert/strict";
import { test } from "node:test";
import {
  RELEASE_SEMVER_PATTERN,
  compareSemVer,
  isReleaseSemVer,
  isSemVerGreater,
  parseSemVer,
} from "./semver.mjs";

test("RELEASE_SEMVER_PATTERN matches the release version shape", () => {
  assert.equal(
    RELEASE_SEMVER_PATTERN.source,
    "^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)(?:-(alpha|rc)\\.([1-9]\\d*))?$",
  );
});

test("isReleaseSemVer accepts clean X.Y.Z and -alpha.N / -rc.N", () => {
  for (const version of ["0.2.0", "0.2.0-alpha.1", "0.2.0-alpha.10", "0.2.0-rc.1", "10.20.30-rc.12"]) {
    assert.equal(isReleaseSemVer(version), true, version);
  }
  for (const version of ["0.1.0", "1.0.0", "10.20.30", "0.0.0", "0.2.0-rc.2", "1.2.3-alpha.99"]) {
    assert.equal(isReleaseSemVer(version), true, version);
  }
});

test("isReleaseSemVer rejects malformed labels, numbers, build metadata and stray shapes", () => {
  for (const version of [
    // Item 8 reject table.
    "0.2.0-alpha",
    "0.2.0-alpha.0",
    "0.2.0-alpha.01",
    "0.2.0-Alpha.1",
    "0.2.0-RC.1",
    "0.2.0-beta.1",
    "0.2.0-alpha.1.2",
    "0.2.0+build",
    "0.2.0-alpha.1+build",
    "0.2.0-",
    "01.2.0",
    // Core-shape rejections retained from the clean-grammar table.
    "1.02.3",
    "1.2.03",
    "00.0.0",
    "0.2",
    "0.2.0.1",
    "v0.2.0",
    " 0.2.0",
    "",
    null,
    2,
  ]) {
    assert.equal(isReleaseSemVer(version), false, String(version));
  }
});

test("parseSemVer returns core digit parts plus parsed prerelease, null otherwise", () => {
  assert.deepEqual(parseSemVer("1.20.300"), {
    major: "1",
    minor: "20",
    patch: "300",
    prerelease: null,
  });
  assert.deepEqual(parseSemVer("0.0.0"), {
    major: "0",
    minor: "0",
    patch: "0",
    prerelease: null,
  });
  assert.deepEqual(parseSemVer("0.2.0-alpha.1"), {
    major: "0",
    minor: "2",
    patch: "0",
    prerelease: { label: "alpha", num: "1" },
  });
  assert.deepEqual(parseSemVer("10.20.30-rc.12"), {
    major: "10",
    minor: "20",
    patch: "30",
    prerelease: { label: "rc", num: "12" },
  });
  assert.equal(parseSemVer("01.20.300"), null);
  assert.equal(parseSemVer("1.20"), null);
  assert.equal(parseSemVer("1.20.300-beta.1"), null);
  assert.equal(parseSemVer("1.20.300-alpha.0"), null);
});

test("compareSemVer orders by major, then minor, then patch", () => {
  assert.ok(compareSemVer("0.2.0", "0.1.0") > 0);
  assert.ok(compareSemVer("0.1.0", "0.2.0") < 0);
  assert.equal(compareSemVer("0.1.0", "0.1.0"), 0);
  assert.ok(compareSemVer("1.0.0", "0.99.99") > 0);
  assert.ok(compareSemVer("0.1.10", "0.1.9") > 0);
});

test("compareSemVer orders the prerelease ladder within one core", () => {
  const ladder = [
    "0.2.0-alpha.1",
    "0.2.0-alpha.2",
    "0.2.0-alpha.10",
    "0.2.0-rc.1",
    "0.2.0-rc.2",
    "0.2.0",
  ];
  for (let i = 1; i < ladder.length; i += 1) {
    assert.ok(compareSemVer(ladder[i], ladder[i - 1]) > 0, `${ladder[i]} > ${ladder[i - 1]}`);
    assert.ok(compareSemVer(ladder[i - 1], ladder[i]) < 0, `${ladder[i - 1]} < ${ladder[i]}`);
  }
  assert.equal(compareSemVer("0.2.0-alpha.3", "0.2.0-alpha.3"), 0);
});

test("compareSemVer lets the core decide across cores", () => {
  assert.ok(compareSemVer("0.2.0-alpha.9", "0.1.9") > 0);
  assert.ok(compareSemVer("0.1.9", "0.2.0-alpha.9") < 0);
  assert.ok(compareSemVer("0.1.0", "0.2.0-alpha.1") < 0);
  assert.ok(compareSemVer("0.2.0-rc.1", "0.1.99") > 0);
  assert.ok(compareSemVer("1.0.0-alpha.1", "0.99.99-rc.9") > 0);
});

test("compareSemVer keeps components beyond Number.MAX_SAFE_INTEGER distinct, including N", () => {
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

  const hugeSuffix = "0.2.0-alpha.9007199254740993";
  const hugeSuffixNeighbour = "0.2.0-alpha.9007199254740992";
  assert.ok(compareSemVer(hugeSuffix, hugeSuffixNeighbour) > 0);
  assert.ok(compareSemVer(hugeSuffixNeighbour, hugeSuffix) < 0);
  assert.equal(compareSemVer(hugeSuffix, hugeSuffix), 0);
  const paddedSuffix = `0.2.0-rc.1${"0".repeat(30)}`;
  const shorterSuffix = `0.2.0-rc.${"9".repeat(30)}`;
  assert.ok(compareSemVer(paddedSuffix, shorterSuffix) > 0, "suffix length decides before lexicographic order");
});

test("compareSemVer throws on non-release versions", () => {
  assert.throws(() => compareSemVer("1.2", "1.2.0"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("1.2.0", "1.2.0-beta.1"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("01.2.3", "1.2.3"), /Invalid SemVer compare/);
  assert.throws(() => compareSemVer("1.2.0", "1.2.0-alpha.0"), /Invalid SemVer compare/);
});

test("isSemVerGreater is strict", () => {
  assert.equal(isSemVerGreater("0.2.0", "0.1.0"), true);
  assert.equal(isSemVerGreater("0.1.0", "0.1.0"), false);
  assert.equal(isSemVerGreater("0.1.0", "0.2.0"), false);
  assert.equal(isSemVerGreater("0.2.0-rc.1", "0.2.0-alpha.3"), true);
  assert.equal(isSemVerGreater("0.2.0-alpha.3", "0.2.0-rc.1"), false);
  assert.equal(isSemVerGreater("0.2.0", "0.2.0-rc.2"), true);
});
