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

/** @type {RegExp} */
export const CLEAN_SEMVER_PATTERN = /^\d+\.\d+\.\d+$/;

/**
 * @param {unknown} version
 * @returns {boolean}
 */
export function isCleanSemVer(version) {
  return typeof version === "string" && CLEAN_SEMVER_PATTERN.test(version);
}

/**
 * @typedef {{ major: number; minor: number; patch: number }} ParsedSemVer
 */

/**
 * @param {string} version
 * @returns {ParsedSemVer | null}
 */
export function parseSemVer(version) {
  if (!isCleanSemVer(version)) {
    return null;
  }
  const [major, minor, patch] = version.split(".").map((part) => Number(part));
  return { major, minor, patch };
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
  return left.major - right.major || left.minor - right.minor || left.patch - right.patch;
}

/**
 * @param {string} next
 * @param {string} current
 * @returns {boolean}
 */
export function isSemVerGreater(next, current) {
  return compareSemVer(next, current) > 0;
}
