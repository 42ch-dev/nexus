#!/usr/bin/env node
/**
 * Clean SemVer helpers for the Nexus release flow.
 *
 * Release versions are always clean `X.Y.Z`: prerelease-ness lives in the
 * GitHub Release metadata, never in the version string (compass D9), so this
 * module refuses prerelease/build metadata instead of parsing it.
 *
 * @module tooling/release/semver
 */

/**
 * Canonical clean SemVer: three numeric components with no leading zeros, so
 * `01.2.3` and `1.02.3` are rejected instead of silently normalised.
 *
 * @type {RegExp}
 */
export const CLEAN_SEMVER_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

/**
 * @param {unknown} version
 * @returns {boolean}
 */
export function isCleanSemVer(version) {
  return typeof version === "string" && CLEAN_SEMVER_PATTERN.test(version);
}

/**
 * Numeric components as their canonical digit strings. Because the pattern
 * forbids leading zeros, string length then lexicographic order is a lossless
 * numeric ordering: arbitrarily long components (beyond `Number.MAX_SAFE_INTEGER`)
 * stay distinct and correctly ordered.
 *
 * @typedef {{ major: string; minor: string; patch: string }} ParsedSemVer
 */

/**
 * @param {string} version
 * @returns {ParsedSemVer | null}
 */
export function parseSemVer(version) {
  if (!isCleanSemVer(version)) {
    return null;
  }
  const [major, minor, patch] = version.split(".");
  return { major, minor, patch };
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

/**
 * Compare clean SemVer strings: negative when a < b, 0 when equal, positive
 * when a > b. Throws on anything that is not a clean SemVer.
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
    compareComponents(left.patch, right.patch)
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
