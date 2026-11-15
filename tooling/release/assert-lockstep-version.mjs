#!/usr/bin/env node
/**
 * Assert the Nexus version surfaces are in lockstep.
 *
 * Checks the five hand-written surfaces (`Cargo.toml` `[workspace.package]`,
 * root `package.json`, `apps/desktop-electron/package.json`,
 * `apps/desktop-electron/resources/product.json`,
 * `apps/nexus-service/package.json`), the five native npm
 * manifests (`packages/nexus-native/package.json` plus the four platform
 * packages — including the loader's `@42ch/nexus-native-*` pins) and the
 * `Cargo.lock` rows of two brackets:
 *
 * - Contract crates (`VERSION_BRACKETS_PATH`) version independently: their
 *   manifest declares a literal version and their lock row equals it. They are
 *   not compared to the product version.
 * - Every other workspace member keeps `version.workspace = true` and its lock
 *   row equals the product version; a literal there fails as an undeclared
 *   independent crate.
 *
 * CLI: node tooling/release/assert-lockstep-version.mjs
 *
 * Exit 0 when all surfaces agree; non-zero otherwise.
 *
 * @module tooling/release/assert-lockstep-version
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CARGO_LOCK_PATH,
  NATIVE_LOADER_PACKAGE_PATH,
  VERSION_BRACKETS_PATH,
  readSurfaceVersions,
  readVersionBrackets,
  readWorkspaceMemberPackages,
  resolveRepoRoot,
  parseCargoLockPackageVersion,
  parseCargoPackageVersion,
} from "./version-surfaces.mjs";

/**
 * @typedef {object} LockstepReport
 * @property {boolean} ok
 * @property {string | null} version Canonical version when the four agree.
 * @property {{ path: string; version: string }[]} surfaces
 * @property {{ name: string; version: string | null }[]} lockMembers
 * @property {string[]} problems
 */

/**
 * @param {string} repoRoot
 * @returns {LockstepReport}
 */
export function checkLockstep(repoRoot) {
  /** @type {string[]} */
  const problems = [];
  const surfaces = readSurfaceVersions(repoRoot);
  const versions = new Set(surfaces.map((surface) => surface.version));
  const ok = versions.size === 1;
  const productVersion = ok ? /** @type {string} */ ([...versions][0]) : null;
  if (!ok) {
    problems.push(
      `version surfaces disagree: ${surfaces
        .map((surface) => `${surface.path}=${surface.version}`)
        .join(", ")}`,
    );
  } else {
    const loaderManifest = JSON.parse(
      readFileSync(join(repoRoot, NATIVE_LOADER_PACKAGE_PATH), "utf8"),
    );
    const pins = loaderManifest?.optionalDependencies ?? {};
    for (const [name, pin] of Object.entries(pins)) {
      if (name.startsWith("@42ch/nexus-native") && pin !== "workspace:*") {
        problems.push(
          `${NATIVE_LOADER_PACKAGE_PATH}: ${name} pin ${pin}, expected workspace:* (pnpm pack replaces it with the exact platform version)`,
        );
      }
    }
  }

  const contractEntries = readVersionBrackets(repoRoot);
  const contractNames = new Set(contractEntries.map((entry) => entry.name));
  const members = readWorkspaceMemberPackages(repoRoot);
  const memberNames = new Set(members.map((member) => member.name));

  const lockPath = join(repoRoot, CARGO_LOCK_PATH);
  const lockContents = existsSync(lockPath) ? readFileSync(lockPath, "utf8") : null;
  if (lockContents === null) {
    problems.push(`${CARGO_LOCK_PATH}: missing`);
  }

  for (const entry of contractEntries) {
    const literal = parseCargoPackageVersion(
      readFileSync(join(repoRoot, entry.manifest), "utf8"),
      entry.manifest,
    );
    if (literal === null) {
      problems.push(
        `${entry.manifest}: contract crate ${entry.name} declares version.workspace = true, expected a literal version (contract crates version independently of the product)`,
      );
      continue;
    }
    if (lockContents === null) {
      continue;
    }
    const pinned = parseCargoLockPackageVersion(lockContents, entry.name);
    if (pinned === null) {
      if (memberNames.has(entry.name)) {
        problems.push(`${CARGO_LOCK_PATH}: missing [[package]] entry for ${entry.name}`);
      }
    } else if (pinned !== literal) {
      problems.push(
        `${CARGO_LOCK_PATH}: contract crate ${entry.name} pinned at ${pinned}, expected its manifest literal ${literal} (${entry.manifest})`,
      );
    }
  }

  /** @type {{ name: string; version: string | null }[]} */
  const lockMembers = [];
  for (const member of members) {
    const pinned =
      lockContents === null ? null : parseCargoLockPackageVersion(lockContents, member.name);
    lockMembers.push({ name: member.name, version: pinned });
    if (lockContents === null || contractNames.has(member.name)) {
      continue;
    }
    if (pinned === null) {
      problems.push(`${CARGO_LOCK_PATH}: missing [[package]] entry for ${member.name}`);
      continue;
    }
    const literal = parseCargoPackageVersion(
      readFileSync(join(repoRoot, member.manifestPath), "utf8"),
      member.manifestPath,
    );
    if (literal !== null) {
      problems.push(
        `${member.manifestPath}: non-contract member ${member.name} declares literal version ${literal}, expected version.workspace = true (a literal version makes it an undeclared independent crate; list it in ${VERSION_BRACKETS_PATH} if that is intended)`,
      );
    } else if (ok && pinned !== productVersion) {
      problems.push(
        `${CARGO_LOCK_PATH}: non-contract member ${member.name} pinned at ${pinned}, expected product version ${productVersion}`,
      );
    }
  }

  return {
    ok: ok && problems.length === 0,
    version: productVersion,
    surfaces,
    lockMembers,
    problems,
  };
}

/**
 * @param {LockstepReport} report
 */
function printReport(report) {
  for (const surface of report.surfaces) {
    console.log(`  ${surface.path.padEnd(48)} ${surface.version}`);
  }
  const lockVersions = new Set(report.lockMembers.map((member) => member.version));
  console.log(
    `  ${CARGO_LOCK_PATH.padEnd(48)} ${
      lockVersions.size === 1 ? [...lockVersions][0] : [...lockVersions].join(", ")
    } (${report.lockMembers.length} workspace members)`,
  );
}

function main(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log("Usage: node tooling/release/assert-lockstep-version.mjs");
    return 0;
  }

  const report = checkLockstep(resolveRepoRoot());
  if (report.ok) {
    console.log("Lockstep surfaces:");
    printReport(report);
    console.log(`Lockstep OK: ${report.version}`);
    return 0;
  }

  console.error("Lockstep surfaces:");
  printReport(report);
  console.error(`Lockstep mismatch:`);
  for (const problem of report.problems) {
    console.error(`  - ${problem}`);
  }
  return 1;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
