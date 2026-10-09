#!/usr/bin/env node
/**
 * Assert a release version is a valid, unused, strictly increasing version.
 *
 * Checks, in order:
 *   1. the target is a release SemVer (X.Y.Z, X.Y.Z-alpha.N, or X.Y.Z-rc.N)
 *   2. it is strictly greater than the root `package.json` version on
 *      `origin/<base-ref>` (the release lands on top of main; the reference may
 *      itself be suffixed once an alpha/rc bump has merged)
 *   3. no `v<version>` tag exists yet
 *
 * CLI:
 *   node tooling/release/assert-version-greater.mjs <release-version> [--base-ref main] [--repo-root <path>]
 *
 * Exit 0 when all three hold; non-zero otherwise.
 *
 * @module tooling/release/assert-version-greater
 */

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { ROOT_PACKAGE_PATH, resolveRepoRoot } from "./version-surfaces.mjs";
import { isReleaseSemVer, isSemVerGreater } from "./semver.mjs";

/**
 * @param {string} repoRoot
 * @param {string[]} args
 * @returns {{ status: number; stdout: string; stderr: string }}
 */
export function runGit(repoRoot, args) {
  const result = spawnSync("git", args, { cwd: repoRoot, encoding: "utf8" });
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? result.error?.message ?? "",
  };
}

/**
 * @typedef {object} VersionGreaterReport
 * @property {boolean} ok
 * @property {string} target
 * @property {string | null} referenceVersion
 * @property {string} referenceRef
 * @property {string | null} tag
 * @property {boolean} tagExists
 * @property {string[]} problems
 */

/**
 * @param {{ repoRoot: string; target: string; baseRef?: string }} options
 * @returns {VersionGreaterReport}
 */
export function checkVersionGreater({ repoRoot, target, baseRef = "main" }) {
  const referenceRef = `origin/${baseRef}`;
  const tag = `v${target}`;
  /** @type {string[]} */
  const problems = [];
  /** @type {string | null} */
  let referenceVersion = null;
  let tagExists = false;

  if (!isReleaseSemVer(target)) {
    return {
      ok: false,
      target,
      referenceVersion,
      referenceRef,
      tag,
      tagExists,
      problems: [
        `Invalid version "${target}": expected a release SemVer (X.Y.Z, X.Y.Z-alpha.N, or X.Y.Z-rc.N)`,
      ],
    };
  }

  const show = runGit(repoRoot, ["show", `${referenceRef}:${ROOT_PACKAGE_PATH}`]);
  if (show.status !== 0) {
    problems.push(
      `cannot read ${referenceRef}:${ROOT_PACKAGE_PATH} (${show.stderr.trim() || "git show failed"})`,
    );
  } else {
    try {
      const parsed = JSON.parse(show.stdout);
      referenceVersion = typeof parsed?.version === "string" ? parsed.version : null;
    } catch (error) {
      problems.push(
        `${referenceRef}:${ROOT_PACKAGE_PATH} is not valid JSON (${error instanceof Error ? error.message : error})`,
      );
    }
    if (referenceVersion === null) {
      problems.push(`${referenceRef}:${ROOT_PACKAGE_PATH}: missing "version" string field`);
    } else if (referenceVersion === target) {
      problems.push(
        `Version ${target} equals the ${referenceRef} version; a release must advance it`,
      );
    } else if (!isReleaseSemVer(referenceVersion)) {
      problems.push(
        `${referenceRef}:${ROOT_PACKAGE_PATH} version "${referenceVersion}" is not a release SemVer`,
      );
    } else if (!isSemVerGreater(target, referenceVersion)) {
      problems.push(`Version ${target} must be greater than ${referenceVersion} (${referenceRef})`);
    }
  }

  const tagList = runGit(repoRoot, ["tag", "--list", tag]);
  if (tagList.status !== 0) {
    problems.push(`cannot list tags (${tagList.stderr.trim() || "git tag failed"})`);
  } else {
    tagExists = tagList.stdout.trim().length > 0;
    if (tagExists) {
      problems.push(`Git tag ${tag} already exists; choose a higher version`);
    }
  }

  return {
    ok: problems.length === 0,
    target,
    referenceVersion,
    referenceRef,
    tag,
    tagExists,
    problems,
  };
}

const USAGE = `Usage: node tooling/release/assert-version-greater.mjs <release-version> [--base-ref main] [--repo-root <path>]

<release-version> is X.Y.Z, X.Y.Z-alpha.N, or X.Y.Z-rc.N. Asserts it is strictly
greater than ${ROOT_PACKAGE_PATH} on origin/<base-ref>, with no v<version> tag present.`;

function main(argv, env = process.env) {
  /** @type {{ target: string | null; baseRef: string; repoRoot: string | null; help: boolean }} */
  const options = { target: null, baseRef: "main", repoRoot: null, help: false };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--base-ref") {
      options.baseRef = argv[++i] ?? options.baseRef;
    } else if (arg === "--repo-root") {
      options.repoRoot = argv[++i] ?? null;
    } else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else if (arg.startsWith("-")) {
      console.error(`Unknown argument: ${arg}`);
      console.error(USAGE);
      return 1;
    } else if (options.target === null) {
      options.target = arg.trim();
    } else {
      console.error(`Unexpected argument: ${arg}`);
      console.error(USAGE);
      return 1;
    }
  }

  if (options.help) {
    console.log(USAGE);
    return 0;
  }
  if (options.target === null || options.target.length === 0) {
    console.error(USAGE);
    return 1;
  }

  const report = checkVersionGreater({
    repoRoot: options.repoRoot ?? env.NEXUS_REPO_ROOT?.trim() ?? resolveRepoRoot(),
    target: options.target,
    baseRef: options.baseRef,
  });

  if (report.ok) {
    console.log(
      `Version OK: ${report.target} > ${report.referenceVersion} (${report.referenceRef}); ${report.tag} does not exist`,
    );
    return 0;
  }

  for (const problem of report.problems) {
    console.error(problem);
  }
  return 1;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
