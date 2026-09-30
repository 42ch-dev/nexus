#!/usr/bin/env node
/**
 * Release capability SSOT.
 *
 * `signingImplemented` is the single switch asserting whether Apple-signed /
 * notarized artifacts exist yet. While it is `false` the publish job forces
 * every Release to prerelease, no matter what the `new-release` dispatch
 * toggle asked for (compass D5/D7: unsigned artifacts are labeled unsigned and
 * are only ever published as prereleases).
 *
 * Signing arrival is this one flip — no other release-flow change:
 *   - set `signingImplemented` to `true`
 *   - publish non-prerelease by dispatching `prerelease: false`
 *
 * @module tooling/release/release-config
 */

/** @type {boolean} */
export const signingImplemented = false;
