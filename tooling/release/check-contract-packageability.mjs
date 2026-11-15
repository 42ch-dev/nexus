#!/usr/bin/env node
/**
 * Contract bracket packageability — prove every contract crate can be packed.
 *
 * For each entry in `VERSION_BRACKETS_PATH` this runs
 *
 *   cargo package --manifest-path <manifest> --no-verify --allow-dirty --offline
 *
 * into a throwaway `--target-dir`, so no `target/package` state survives the
 * run and a second run always starts clean.
 *
 * Cargo strips `path` dependencies when it packs a manifest, so a contract
 * crate that depends on another contract crate cannot resolve it from the
 * registry while both are unpublished. Each run therefore receives the
 * contract crates its manifest depends on as `--config
 * patch.crates-io.<name>.path`. Unused patches are not passed: Cargo records
 * them as `[[patch.unused]]` rows and rewrites the checked-in `Cargo.lock`.
 *
 * CLI: node tooling/release/check-contract-packageability.mjs
 *
 * Exit 0 when all contract crates are packageable; non-zero otherwise.
 *
 * @module tooling/release/check-contract-packageability
 */

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readVersionBrackets, resolveRepoRoot } from "./version-surfaces.mjs";

const PACKAGE_ARGS = ["package", "--no-verify", "--allow-dirty", "--offline"];

/**
 * Contract entries the manifest names as a dependency (`name = ...` or
 * `name.path = ...`). Cargo package names are hyphen/underscore/alphanumeric,
 * so they need no regex escaping.
 *
 * @param {string} manifestContents
 * @param {import("./version-surfaces.mjs").ContractBracketEntry[]} entries
 * @returns {import("./version-surfaces.mjs").ContractBracketEntry[]}
 */
function contractDependencies(manifestContents, entries) {
  return entries.filter((entry) =>
    new RegExp(`^\\s*${entry.name}\\s*[.=]`, "m").test(manifestContents),
  );
}

/**
 * @typedef {object} PackageResult
 * @property {string} name Package name.
 * @property {string} manifest Manifest path, relative to the repository root.
 * @property {boolean} ok
 * @property {string} output Cargo's combined output, kept for the failure report.
 */

/**
 * @typedef {object} PackageabilityReport
 * @property {boolean} ok
 * @property {PackageResult[]} results One row per contract entry, in carrier order.
 * @property {string | null} firstFailure Package name of the first failing crate.
 */

/**
 * @param {string} repoRoot
 * @returns {PackageabilityReport}
 */
export function checkContractPackageability(repoRoot) {
  const entries = readVersionBrackets(repoRoot);
  const targetDir = mkdtempSync(join(tmpdir(), "nexus-contract-package-"));
  /** @type {PackageResult[]} */
  const results = [];
  try {
    for (const entry of entries) {
      const manifestContents = readFileSync(join(repoRoot, entry.manifest), "utf8");
      const patches = contractDependencies(manifestContents, entries)
        .filter((dependency) => dependency.name !== entry.name)
        .flatMap((dependency) => [
          "--config",
          `patch.crates-io.${dependency.name}.path = "${join(
            repoRoot,
            dirname(dependency.manifest),
          )}"`,
        ]);
      const run = spawnSync(
        "cargo",
        [
          ...PACKAGE_ARGS,
          "--manifest-path",
          entry.manifest,
          "--target-dir",
          targetDir,
          ...patches,
        ],
        { cwd: repoRoot, encoding: "utf8" },
      );
      results.push({
        name: entry.name,
        manifest: entry.manifest,
        ok: run.error === undefined && run.status === 0,
        output:
          run.error === undefined
            ? `${run.stdout ?? ""}${run.stderr ?? ""}`.trim()
            : `cargo could not run: ${run.error.message}`,
      });
    }
  } finally {
    rmSync(targetDir, { recursive: true, force: true });
  }

  const firstFailure = results.find((result) => !result.ok)?.name ?? null;
  return { ok: firstFailure === null, results, firstFailure };
}

/**
 * @param {string} text
 * @returns {string}
 */
function indent(text) {
  return text
    .split("\n")
    .map((line) => `  ${line}`)
    .join("\n");
}

/**
 * @param {PackageabilityReport} report
 */
function printRows(report) {
  for (const result of report.results) {
    console.log(
      `  ${result.name.padEnd(24)} ${result.manifest.padEnd(44)} ${
        result.ok ? "ok" : "FAILED"
      }`,
    );
  }
}

function main(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log("Usage: node tooling/release/check-contract-packageability.mjs");
    return 0;
  }

  const report = checkContractPackageability(resolveRepoRoot());
  const header = `Contract packageability (${report.results.length} crates):`;

  if (report.ok) {
    console.log(header);
    printRows(report);
    console.log(`Packageable: all ${report.results.length} contract crates`);
    return 0;
  }

  console.error(header);
  printRows(report);
  for (const failure of report.results) {
    if (failure.ok) {
      continue;
    }
    console.error(`\n${failure.name} (${failure.manifest}) cargo said:`);
    console.error(indent(failure.output));
  }
  console.error(`\nFirst failure: ${report.firstFailure}`);
  return 1;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
