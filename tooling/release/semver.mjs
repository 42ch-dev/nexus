#!/usr/bin/env node
/**
 * Release SemVer helpers for the Nexus release flow.
 *
 * A release version is clean `X.Y.Z` or carries an `-alpha.N` / `-rc.N`
 * prerelease suffix: `alpha` marks an explicitly unstable daily/iteration
 * build, `rc` a release candidate put in front of testers before the stable
 * of the same core. Labels are lowercase only; `N` is a canonical positive
 * integer starting at 1; build metadata (`+…`) stays rejected. The module
 * refuses malformed versions instead of normalising them.
 *
 * Cutover note (Design decision item 7): the former clean-only predicate and
 * pattern were removed. Every consumer (bump-version, assert-version-greater,
 * generate-changelog, effective-prerelease) takes release versions; this
 * module no longer exposes a clean-only rule.
 *
 * @module tooling/release/semver
 */

/**
 * Canonical release SemVer: three numeric core components with no leading
 * zeros, optionally followed by `-alpha.N` or `-rc.N`. `N` starts at 1, so
 * `0.2.0-alpha.0` and `0.2.0-alpha.01` are rejected instead of normalised.
 *
 * @type {RegExp}
 */
export const RELEASE_SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|rc)\.([1-9]\d*))?$/;

/**
 * @param {unknown} version
 * @returns {boolean}
 */
export function isReleaseSemVer(version) {
  return typeof version === "string" && RELEASE_SEMVER_PATTERN.test(version);
}

/**
 * Numeric components as their canonical digit strings. Because the pattern
 * forbids leading zeros, string length then lexicographic order is a lossless
 * numeric ordering: arbitrarily long components (beyond `Number.MAX_SAFE_INTEGER`)
 * stay distinct and correctly ordered. The same rule orders the suffix `N`.
 *
 * @typedef {{ label: "alpha" | "rc"; num: string }} ParsedPrerelease
 * @typedef {{ major: string; minor: string; patch: string; prerelease: ParsedPrerelease | null }} ParsedSemVer
 */

/**
 * @param {string} version
 * @returns {ParsedSemVer | null}
 */
export function parseSemVer(version) {
  if (!isReleaseSemVer(version)) {
    return null;
  }
  const match = RELEASE_SEMVER_PATTERN.exec(version);
  const [, major, minor, patch, label, num] = match;
  return {
    major,
    minor,
    patch,
    prerelease: label ? { label, num } : null,
  };
}

/**
 * @param {string} a
 * @param {string} b
 * @returns {number}
 */
function compareComponents(a, b) {
  if (a.length !== b.length) {
    return a.length < b.length ? -1 : 1;
  }
  if (a === b) {
    return 0;
  }
  return a < b ? -1 : 1;
}

/** Fixed two-label precedence: `alpha` is always less than `rc`. */
const LABEL_ORDER = { alpha: 0, rc: 1 };

/**
 * @param {ParsedPrerelease | null} a
 * @param {ParsedPrerelease | null} b
 * @returns {number}
 */
function comparePrerelease(a, b) {
  if (a === null && b === null) {
    return 0;
  }
  if (a === null) {
    return 1;
  }
  if (b === null) {
    return -1;
  }
  if (a.label !== b.label) {
    return LABEL_ORDER[a.label] < LABEL_ORDER[b.label] ? -1 : 1;
  }
  return compareComponents(a.num, b.num);
}

/**
 * Compare release SemVer strings (§11 as one rule set): core components
 * numerically first; when the cores are equal a suffixed version is less than
 * the same core without a suffix, `alpha < rc`, and same-label `N` compares
 * numerically. Returns negative when a < b, 0 when equal, positive when a > b.
 * Throws on anything that is not a release SemVer.
 *
 * @param {string} a
 * @param {string} b
 * @returns {number}
 */
export function compareSemVer(a, b) {
  const left = parseSemVer(a);
  const right = parseSemVer(b);
  if (!left || !right) {
    throw new Error(`Invalid SemVer compare: "${a}" vs "${b}"`);
  }
  return (
    compareComponents(left.major, right.major) ||
    compareComponents(left.minor, right.minor) ||
    compareComponents(left.patch, right.patch) ||
    comparePrerelease(left.prerelease, right.prerelease)
  );
}

/**
 * @param {string} next
 * @param {string} current
 * @returns {boolean}
 */
export function isSemVerGreater(next, current) {
  return compareSemVer(next, current) > 0;
}
