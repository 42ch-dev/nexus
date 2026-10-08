#!/usr/bin/env node
/**
 * Assert the Nexus version surfaces are in lockstep.
 *
 * Checks the four hand-written surfaces (`Cargo.toml` `[workspace.package]`,
 * root `package.json`, `apps/desktop-electron/package.json`,
 * `apps/desktop-electron/resources/product.json`), the five native npm
 * manifests (`packages/nexus-native/package.json` plus the four platform
 * packages — including the loader's `@42ch/nexus-native-*` pins) and every
 * workspace member entry in `Cargo.lock`.
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
  readSurfaceVersions,
  readWorkspaceMemberPackages,
  resolveRepoRoot,
  parseCargoLockPackageVersion,
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

  /** @type {{ name: string; version: string | null }[]} */
  const lockMembers = [];
  const lockPath = join(repoRoot, CARGO_LOCK_PATH);
  if (!existsSync(lockPath)) {
    problems.push(`${CARGO_LOCK_PATH}: missing`);
  } else {
    const lockContents = readFileSync(lockPath, "utf8");
    const expected = [...versions][0];
    for (const member of readWorkspaceMemberPackages(repoRoot)) {
      const version = parseCargoLockPackageVersion(lockContents, member.name);
      lockMembers.push({ name: member.name, version });
      if (version === null) {
        problems.push(`${CARGO_LOCK_PATH}: missing [[package]] entry for ${member.name}`);
      } else if (ok && version !== expected) {
        problems.push(
          `${CARGO_LOCK_PATH}: ${member.name} pinned at ${version}, expected ${expected}`,
        );
      }
    }
  }

  return {
    ok: ok && problems.length === 0,
    version: ok ? /** @type {string} */ ([...versions][0]) : null,
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
