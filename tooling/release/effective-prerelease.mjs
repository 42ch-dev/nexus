#!/usr/bin/env node
/**
 * Effective prerelease decision for a Nexus Release.
 *
 * The `new-release` dispatch toggle travels to the release run as a
 * `Nexus-Prerelease: true|false` trailer on the bump commit; the tag job reads
 * it from the merge commit's second parent (`git show -s --format=%B`).
 *
 * Effective prerelease = hasSuffix(version) OR dispatch toggle OR
 * "signing not implemented": a version carrying an `-alpha.N` / `-rc.N` suffix
 * is always published as a prerelease, and a clean (unsuffixed) full release
 * additionally requires an explicit `prerelease=false` dispatch AND
 * `signingImplemented === true` (compass D5/D7).
 *
 * Fail-closed: an absent or unreadable trailer means prerelease, and an
 * unreadable `--version` also forces prerelease.
 *
 * CLI:
 *   node tooling/release/effective-prerelease.mjs --toggle true|false
 *   node tooling/release/effective-prerelease.mjs --message-file <path|-> [--version <release-version>]
 *   node tooling/release/effective-prerelease.mjs --version <release-version>
 *
 * Prints `true` or `false` (single line) to stdout.
 *
 * @module tooling/release/effective-prerelease
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { signingImplemented } from "./release-config.mjs";
import { parseSemVer } from "./semver.mjs";

/** Commit-message trailer read by the release pipeline. */
export const PRERELEASE_TRAILER = "Nexus-Prerelease";

/** Any line carrying the trailer key, well-formed or not. */
const TRAILER_KEY_PATTERN = /^\s*Nexus-Prerelease\s*:/i;
/** A line carrying the trailer key with exactly one readable value. */
const TRAILER_PATTERN = /^\s*Nexus-Prerelease\s*:\s*(\S+)\s*$/i;

/**
 * Parse the `Nexus-Prerelease` trailer out of a commit message.
 *
 * Every line carrying the key is collected before any value is trusted: a
 * message that also carries a garbled duplicate (empty value, extra tokens,
 * unexpected shape) fails closed instead of letting the readable line win.
 *
 * @param {string} message Full commit message (`git show -s --format=%B`).
 * @returns {boolean | null} Toggle value, or null when absent/ambiguous.
 */
export function parsePrereleaseTrailer(message) {
  if (typeof message !== "string") {
    return null;
  }
  /** @type {string[]} */
  const values = [];
  for (const line of message.split("\n")) {
    if (!TRAILER_KEY_PATTERN.test(line)) {
      continue;
    }
    const value = line.match(TRAILER_PATTERN)?.[1];
    if (value === undefined) {
      return null;
    }
    values.push(value.toLowerCase());
  }
  if (values.length === 0) {
    return null;
  }
  const unique = new Set(values);
  if (unique.size !== 1) {
    return null;
  }
  const [value] = unique;
  if (value === "true") {
    return true;
  }
  if (value === "false") {
    return false;
  }
  return null;
}

/**
 * A release version carrying an `-alpha.N` / `-rc.N` suffix is always a
 * prerelease. An absent version contributes nothing; a version the grammar
 * cannot read fails closed to prerelease, matching the trailer stance.
 *
 * @param {string | null} version Resolved release version, or null when absent.
 * @returns {boolean}
 */
function versionForcesPrerelease(version) {
  if (version === null) {
    return false;
  }
  const parsed = parseSemVer(version);
  return parsed?.prerelease !== null;
}

/**
 * @param {boolean | null} toggle Dispatch toggle from the bump-commit trailer;
 *   `null` when no explicit toggle was supplied. Only an explicit `false`
 *   (the `prerelease=false` opt-out) can open the full-release path, so a
 *   missing toggle fails closed to prerelease.
 * @param {boolean} [signing] `signingImplemented` (injectable for tests).
 * @param {string | null} [version] Resolved release version (`--version`).
 * @returns {boolean}
 */
export function effectivePrerelease(toggle, signing = signingImplemented, version = null) {
  return versionForcesPrerelease(version) || toggle !== false || !signing;
}

/**
 * @param {string} value
 * @returns {boolean}
 */
function parseToggleArgument(value) {
  const normalized = String(value).trim().toLowerCase();
  if (normalized === "true") {
    return true;
  }
  if (normalized === "false") {
    return false;
  }
  throw new Error(`Invalid --toggle value: ${value} (expected true or false)`);
}

/**
 * @returns {{ toggle: boolean | null; messageFile: string | null; version: string | null; help: boolean }}
 */
function parseArgs(argv) {
  /** @type {{ toggle: boolean | null; messageFile: string | null; version: string | null; help: boolean }} */
  const out = { toggle: null, messageFile: null, version: null, help: false };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--toggle") {
      const value = argv[++i];
      if (value === undefined) {
        throw new Error("--toggle requires a value");
      }
      out.toggle = parseToggleArgument(value);
    } else if (arg === "--message-file") {
      const value = argv[++i];
      if (value === undefined) {
        throw new Error("--message-file requires a path");
      }
      out.messageFile = value;
    } else if (arg === "--version") {
      const value = argv[++i];
      if (value === undefined) {
        throw new Error("--version requires a value");
      }
      out.version = value;
    } else if (arg === "--help" || arg === "-h") {
      out.help = true;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return out;
}

const USAGE = `Usage: node tooling/release/effective-prerelease.mjs --toggle <true|false>
       node tooling/release/effective-prerelease.mjs --message-file <path|-> [--version <release-version>]

Prints the effective prerelease value: a version with an -alpha.N / -rc.N
suffix OR the dispatch toggle OR "signing not implemented". A message file is
read for the Nexus-Prerelease: true|false trailer (absent or ambiguous trailer
fails closed to prerelease); --version takes the resolved release version and
an unreadable value fails closed to prerelease.`;

function main(argv) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log(USAGE);
    return 0;
  }
  if (options.toggle === null && options.messageFile === null && options.version === null) {
    console.error(USAGE);
    return 1;
  }

  let toggle = options.toggle;
  let source = options.toggle === null ? "unset" : "toggle";
  if (options.messageFile !== null) {
    const message =
      options.messageFile === "-"
        ? readFileSync(0, "utf8")
        : readFileSync(options.messageFile, "utf8");
    const trailer = parsePrereleaseTrailer(message);
    if (trailer === null) {
      console.error(
        `warning: no usable ${PRERELEASE_TRAILER} trailer in ${options.messageFile}; failing closed to prerelease`,
      );
      toggle = true;
      source = "trailer-missing";
    } else {
      toggle = trailer;
      source = "trailer";
    }
  }

  const effective = effectivePrerelease(
    /** @type {boolean | null} */ (toggle),
    undefined,
    options.version,
  );
  const versionNote = options.version === null ? "" : ` version=${options.version}`;
  console.error(
    `effective prerelease: ${effective} (${source}=${toggle}, signingImplemented=${signingImplemented})${versionNote}`,
  );
  console.log(String(effective));
  return 0;
}

const invokedDirectly =
  process.argv[1] !== undefined &&
  process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
