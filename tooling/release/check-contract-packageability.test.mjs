import assert from "node:assert/strict";
import { test } from "node:test";
import { VERSION_BRACKETS_PATH } from "./version-surfaces.mjs";
import { checkContractPackageability } from "./check-contract-packageability.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  writeRepoFile,
} from "./test-harness.mjs";

/**
 * Dependency-free standalone crate: `cargo package --offline` needs no registry
 * for it, so the script's run loop is testable without a network.
 */
const TINY_CRATE_DIR = "tiny-contract";
const TINY_MANIFEST = [
  "[package]",
  'name = "nexus-tiny-contract"',
  'version = "0.1.0"',
  'edition = "2021"',
  "",
  "# Own workspace root, like the standalone `modules/*` contract crates.",
  "[workspace]",
  "",
].join("\n");

/**
 * Fixture repo whose carrier names only the tiny crate.
 *
 * @param {{ withSource?: boolean }} [options]
 * @returns {string}
 */
function tinyFixture(options = {}) {
  const files = {
    [`${TINY_CRATE_DIR}/Cargo.toml`]: TINY_MANIFEST,
    [VERSION_BRACKETS_PATH]: `${JSON.stringify(
      {
        contract: [
          { name: "nexus-tiny-contract", manifest: `${TINY_CRATE_DIR}/Cargo.toml` },
        ],
      },
      null,
      2,
    )}\n`,
  };
  if (options.withSource !== false) {
    files[`${TINY_CRATE_DIR}/src/lib.rs`] = "pub fn tiny() {}\n";
  }
  return createTempRepo({ files });
}

test("every carrier entry is packaged and reported", () => {
  const dir = tinyFixture();
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    assert.equal(report.firstFailure, null);
    assert.deepEqual(
      report.results.map((result) => [result.name, result.manifest, result.ok]),
      [["nexus-tiny-contract", `${TINY_CRATE_DIR}/Cargo.toml`, true]],
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a crate Cargo refuses to pack fails the report and names the first failure", () => {
  // No `src/lib.rs`: Cargo has no target to package.
  const dir = tinyFixture({ withSource: false });
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-tiny-contract");
    assert.equal(report.results[0].ok, false);
    assert.ok(report.results[0].output.length > 0);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a manifest that is not a crate fails instead of being skipped", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, "not-a-crate/Cargo.toml", "[workspace]\n");
    writeRepoFile(
      dir,
      VERSION_BRACKETS_PATH,
      `${JSON.stringify({
        contract: [{ name: "nexus-ghost", manifest: "not-a-crate/Cargo.toml" }],
      })}\n`,
    );
    assert.throws(
      () => checkContractPackageability(dir),
      /not-a-crate\/Cargo\.toml: missing \[package\]\.name/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});
