import assert from "node:assert/strict";
import { cpSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { VERSION_BRACKETS_PATH } from "./version-surfaces.mjs";
import { checkContractPackageability } from "./check-contract-packageability.mjs";
import {
  REPO_ROOT,
  cleanupTempRepo,
  createTempRepo,
  writeRepoFile,
} from "./test-harness.mjs";

/** Repository-root-relative discovery path the checker reads. */
const CONSUMER_FIXTURES_PATH = "tooling/release/fixtures/package-consumers";
const CONSUMER_FIXTURES_DIR = join(REPO_ROOT, CONSUMER_FIXTURES_PATH);

/** Kept in sync with the checker's scratch prefix. */
const SCRATCH_PREFIX = "nexus-contract-package-";

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
          {
            name: "nexus-tiny-contract",
            manifest: `${TINY_CRATE_DIR}/Cargo.toml`,
          },
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

/**
 * Temp repo whose carrier names only a standalone fixture subject copied from
 * `<caseDir>/subject`, with `<caseDir>/consumer` installed under the discovery
 * path the checker reads for that entry.
 *
 * @param {string} caseDir Fixture case directory name.
 * @param {string} entryName Contract carrier and discovery name.
 * @returns {string}
 */
function consumerFixtureRepo(caseDir, entryName) {
  const dir = createTempRepo();
  const subjectDir = `fixtures/${entryName}`;
  cpSync(join(CONSUMER_FIXTURES_DIR, caseDir, "subject"), join(dir, subjectDir), {
    recursive: true,
  });
  writeRepoFile(
    dir,
    VERSION_BRACKETS_PATH,
    `${JSON.stringify(
      {
        contract: [{ name: entryName, manifest: `${subjectDir}/Cargo.toml` }],
      },
      null,
      2,
    )}\n`,
  );
  cpSync(
    join(CONSUMER_FIXTURES_DIR, caseDir, "consumer"),
    join(dir, CONSUMER_FIXTURES_PATH, entryName),
    { recursive: true },
  );
  return dir;
}

/**
 * Disposable scratch dirs currently present in the checker's scratch parent.
 *
 * @returns {string[]}
 */
function scratchEntries() {
  const targetDir = process.env.CARGO_TARGET_DIR?.trim();
  const parent =
    targetDir !== undefined && targetDir.length > 0
      ? resolve(targetDir)
      : tmpdir();
  mkdirSync(parent, { recursive: true });
  return readdirSync(parent).filter((name) => name.startsWith(SCRATCH_PREFIX));
}

test("every carrier entry is packaged and reported with archive evidence", () => {
  const dir = tinyFixture();
  try {
    const lockBefore = readFileSync(join(dir, "Cargo.lock"), "utf8");
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    assert.equal(report.firstFailure, null);
    assert.deepEqual(
      report.results.map((result) => [
        result.name,
        result.manifest,
        result.ok,
      ]),
      [["nexus-tiny-contract", `${TINY_CRATE_DIR}/Cargo.toml`, true]],
    );

    const [row] = report.results;
    assert.equal(row.cause, null);
    assert.equal(row.archive.file, "nexus-tiny-contract-0.1.0.crate");
    assert.match(row.archive.sha256, /^[0-9a-f]{64}$/);
    assert.ok(row.archive.bytes > 0);
    assert.equal(row.package.version, "0.1.0");
    assert.deepEqual(row.dependencies, []);
    assert.equal(row.consumer, null);

    // Staging never runs Cargo against the source checkout.
    assert.equal(readFileSync(join(dir, "Cargo.lock"), "utf8"), lockBefore);
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
    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.match(row.cause, /cargo package failed/);
    assert.ok(row.output.length > 0);
    assert.equal(row.archive, null);
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

test("a consumer builds and runs the public API of the selected archive", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const [row] = report.results;
    assert.equal(row.archive.file, "nexus-fixture-public-0.1.0.crate");
    assert.match(row.archive.sha256, /^[0-9a-f]{64}$/);
    assert.deepEqual(row.dependencies, []);
    assert.equal(row.cause, null);
    assert.equal(row.consumer.ok, true);
    assert.equal(row.consumer.cause, null);
    assert.deepEqual(row.consumer.patched, [
      { name: "nexus-fixture-public", sha256: row.archive.sha256 },
    ]);
    const consumed = row.consumer.resolved.find(
      (resolved) => resolved.name === "nexus-fixture-public",
    );
    assert.equal(consumed.source, `archive:${row.archive.sha256}`);
    assert.ok(consumed.manifest.includes("archives"));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an archive missing a packed module fails the consumer with a named cause and cleans up", () => {
  const before = scratchEntries();
  const dir = consumerFixtureRepo("missing-packed-input", "nexus-fixture-generated");
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-generated");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.archive.file, "nexus-fixture-generated-0.1.0.crate");
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /consumer run failed/);
    // The generated module the lib needs was never packed.
    assert.match(row.consumer.output, /generated/);
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }

  assert.deepEqual(
    scratchEntries().filter((name) => !before.includes(name)),
    [],
  );
});

test("an incompatible sibling requirement fails locked offline resolution", () => {
  const dir = consumerFixtureRepo(
    "incompatible-sibling-requirement",
    "nexus-fixture-sibling",
  );
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-sibling");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /consumer lock resolution failed/);
    assert.match(row.consumer.output, /0\.2/);
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a private dependency fails before any resolution", () => {
  const dir = consumerFixtureRepo(
    "forbidden-private-dependency",
    "nexus-fixture-anchor",
  );
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-anchor");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /private package nexus-core/);
    assert.deepEqual(row.consumer.patched, []);
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a path dependency is refused even when its source exists", () => {
  const dir = consumerFixtureRepo(
    "original-source-path-escape",
    "nexus-fixture-escape",
  );
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-escape");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(
      row.consumer.cause,
      /path dependency nexus-fixture-escape/,
    );
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }
});
