#!/usr/bin/env node
/**
 * Bump the Nexus release version across the five-file SSOT.
 *
 * Files written (all in one step, never committed by this script):
 *   - `Cargo.toml` `[workspace.package].version`
 *   - root `package.json`
 *   - `apps/desktop-electron/package.json`
 *   - `apps/desktop-electron/resources/product.json`
 *   - `Cargo.lock` (workspace member `[[package]]` versions)
 *
 * The changelog entry is a separate step — `generate-changelog.mjs`.
 *
 * CLI: node tooling/release/bump-version.mjs <X.Y.Z> [--repo-root <path>]
 *
 * @module tooling/release/bump-version
 */

import { fileURLToPath } from "node:url";
import {
  CARGO_LOCK_PATH,
  LOCKSTEP_PATHS,
  ROOT_PACKAGE_PATH,
  readSurfaceVersion,
  resolveRepoRoot,
  writeReleaseVersion,
} from "./version-surfaces.mjs";
import { checkLockstep } from "./assert-lockstep-version.mjs";
import { isCleanSemVer, isSemVerGreater } from "./semver.mjs";

/**
 * @typedef {object} BumpReport
 * @property {boolean} ok
 * @property {string} from
 * @property {string} to
 * @property {{ path: string; from: string; members: number }[]} changed
 * @property {string[]} problems
 * @property {import("./assert-lockstep-version.mjs").LockstepReport | null} lockstep
 */

/**
 * @param {{ repoRoot: string; target: string }} options
 * @returns {BumpReport}
 */
export function bumpVersion({ repoRoot, target }) {
  /** @type {string[]} */
  const problems = [];

  if (!isCleanSemVer(target)) {
    return {
      ok: false,
      from: "",
      to: target,
      changed: [],
      problems: [
        `Invalid version "${target}": expected a clean SemVer (X.Y.Z, no prerelease suffix)`,
      ],
      lockstep: null,
    };
  }

  const current = readSurfaceVersion(repoRoot, ROOT_PACKAGE_PATH);
  if (!isCleanSemVer(current)) {
    return {
      ok: false,
      from: current,
      to: target,
      changed: [],
      problems: [`${ROOT_PACKAGE_PATH}: version "${current}" is not a clean SemVer`],
      lockstep: null,
    };
  }
  if (!isSemVerGreater(target, current)) {
    return {
      ok: false,
      from: current,
      to: target,
      changed: [],
      problems: [
        `Target version ${target} must be greater than the current ${current} (${ROOT_PACKAGE_PATH})`,
      ],
      lockstep: null,
    };
  }

  let changed;
  let lockstep;
  try {
    changed = writeReleaseVersion(repoRoot, target);
    lockstep = checkLockstep(repoRoot);
  } catch (error) {
    return {
      ok: false,
      from: current,
      to: target,
      changed: [],
      problems: [error instanceof Error ? error.message : String(error)],
      lockstep: null,
    };
  }
  if (!lockstep.ok) {
    problems.push(...lockstep.problems);
  }

  return { ok: problems.length === 0, from: current, to: target, changed, problems, lockstep };
}

const USAGE = `Usage: node tooling/release/bump-version.mjs <X.Y.Z> [--repo-root <path>]

Bumps ${LOCKSTEP_PATHS.join(", ")} and ${CARGO_LOCK_PATH}, then re-checks lockstep.
Does not commit, tag, or touch CHANGELOG.md.`;

function main(argv, env = process.env) {
  /** @type {{ target: string | null; repoRoot: string | null; help: boolean }} */
  const options = { target: null, repoRoot: null, help: false };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--repo-root") {
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

  const repoRoot = options.repoRoot ?? env.NEXUS_REPO_ROOT?.trim() ?? resolveRepoRoot();
  const report = bumpVersion({ repoRoot, target: options.target });

  if (!report.ok) {
    for (const problem of report.problems) {
      console.error(problem);
    }
    return 1;
  }

  console.log(`Bumped ${report.from} -> ${report.to}`);
  for (const file of report.changed) {
    const detail = file.members > 0 ? `${file.members} workspace members` : file.from;
    console.log(`  ${file.path.padEnd(48)} ${detail}`);
  }
  console.log(`Lockstep OK: ${report.to}`);
  console.log("");
  console.log("Next steps:");
  console.log("  git add -A");
  console.log(`  git commit -m "chore(release): bump version to ${report.to}"`);
  console.log("");
  console.log("CHANGELOG entry (append before committing):");
  console.log(
    `  node tooling/release/generate-changelog.mjs --version ${report.to} --prepend CHANGELOG.md`,
  );
  return 0;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
