import assert from "node:assert/strict";
import { execFile, spawn, spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { after, test } from "node:test";
import { pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { VERSION_BRACKETS_PATH } from "./version-surfaces.mjs";
import {
  checkContractPackageability,
  shareArchive,
  verifyStagedSource,
} from "./check-contract-packageability.mjs";
import {
  REPO_ROOT,
  cleanupTempRepo,
  createTempRepo,
  git,
  gitOk,
  initGitRepo,
  runReleaseScript,
  writeRepoFile,
} from "./test-harness.mjs";

/**
 * One compiled-artifact cache for the whole file: every case that ships the
 * same contract crates resolves them from the same directories, so Cargo builds
 * that graph once instead of once per case. Each case still stages, packs and
 * reports on its own inputs. See `shareRoot` in the checker.
 */
const SHARE_ROOT = mkdtempSync(join(tmpdir(), "nexus-release-share-"));

/** Wall-clock start of the file, the budget test below reads it. */
const FILE_STARTED = Date.now();

/**
 * Measured budget for this file, left well below `timeout-minutes` so a regression
 * fails here with a number instead of being killed with nothing to read. The runs
 * behind it are recorded next to the CI step: 720 s is 2.2x the slowest recorded
 * run (329 s in 45949440689) and 3.3x the fastest (219 s in 45955451001), which
 * spans the 1.5x runner swing; locally the file measures 188.2 s on a 10-core
 * darwin/arm64 host.
 */
const FILE_BUDGET_SECONDS = 720;

// Every blocking child (`cargo`, `git`, `tar`) may run only until this file's
// budget expires, so a case that outgrows it dies as a named failure inside the
// budget instead of being killed with the whole step.
process.env.NEXUS_RELEASE_DEADLINE_MS ??= String(
  FILE_STARTED + FILE_BUDGET_SECONDS * 1000,
);

/**
 * @param {string} dir
 * @returns {import("./check-contract-packageability.mjs").PackageabilityReport}
 */
function check(dir) {
  return checkContractPackageability(dir, { shareRoot: SHARE_ROOT });
}

after(() => cleanupTempRepo(SHARE_ROOT));

/** Repository-root-relative discovery path the checker reads. */
const CONSUMER_FIXTURES_PATH = "tooling/release/fixtures/package-consumers";
const CONSUMER_FIXTURES_DIR = join(REPO_ROOT, CONSUMER_FIXTURES_PATH);
/** Checked-in accounting every fixture repo must carry. */
const CONSUMER_ACCOUNTING_PATH = "tooling/release/fixtures/package-consumers.json";

/**
 * @param {Record<string, string | null>} entries
 * @returns {string}
 */
function consumerAccounting(entries) {
  return `${JSON.stringify(
    Object.fromEntries(
      Object.entries(entries).map(([name, fixture]) => [
        name,
        fixture === null
          ? {
              fixture: null,
              reason: "no runner of its own; another fixture resolves this archive",
            }
          : { fixture },
      ]),
    ),
    null,
    2,
  )}\n`;
}

/** Kept in sync with the checker's scratch prefix. */
const SCRATCH_PREFIX = "nexus-contract-package-";

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
  files[CONSUMER_ACCOUNTING_PATH] = consumerAccounting({
    "nexus-tiny-contract": null,
  });
  const dir = createTempRepo({ files });
  initGitRepo(dir);
  return dir;
}

/**
 * @param {string} caseDir Fixture case directory name.
 * @param {string} entryName Contract carrier and discovery name.
 * @param {{ initGit?: boolean }} [options] `initGit: false` lets a test add
 *   ignored inputs before the single initial commit.
 * @returns {string}
 */
function consumerFixtureRepo(caseDir, entryName, options = {}) {
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
  writeRepoFile(
    dir,
    CONSUMER_ACCOUNTING_PATH,
    consumerAccounting({ [entryName]: entryName }),
  );
  if (options.initGit !== false) {
    initGitRepo(dir);
  }
  return dir;
}

/**
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
    const report = check(dir);
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

    assert.equal(readFileSync(join(dir, "Cargo.lock"), "utf8"), lockBefore);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a crate Cargo refuses to pack fails the report and names the first failure", () => {
  const dir = tinyFixture({ withSource: false });
  try {
    const report = check(dir);
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
      () => check(dir),
      /not-a-crate\/Cargo\.toml: missing \[package\]\.name/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("staging fails closed when Git cannot list the working source", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, `${TINY_CRATE_DIR}/Cargo.toml`, TINY_MANIFEST);
    writeRepoFile(dir, `${TINY_CRATE_DIR}/src/lib.rs`, "pub fn tiny() {}\n");
    writeRepoFile(
      dir,
      VERSION_BRACKETS_PATH,
      `${JSON.stringify(
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
    );
    assert.throws(
      () => check(dir),
      /staging failed: git ls-files failed/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a consumer builds and runs the public API of the selected archive", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    const report = check(dir);
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

test("staging leaves gitignored working files out of the packed archive", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public", {
    initGit: false,
  });
  try {
    writeRepoFile(dir, ".gitignore", "build.rs\n");
    writeRepoFile(
      dir,
      "fixtures/nexus-fixture-public/build.rs",
      'fn main() {\n    panic!("ignored build script must not be packed");\n}\n',
    );
    initGitRepo(dir);
    assert.equal(
      git(dir, [
        "check-ignore",
        "-q",
        "fixtures/nexus-fixture-public/build.rs",
      ]).status,
      0,
      "the probe build script must be gitignored",
    );

    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    const [row] = report.results;
    assert.equal(row.archive.file, "nexus-fixture-public-0.1.0.crate");
    assert.equal(row.consumer.ok, true);
    assert.equal(row.consumer.cause, null);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an archive missing a packed module fails the consumer with a named cause and cleans up", () => {
  const before = scratchEntries();
  const dir = consumerFixtureRepo("missing-packed-input", "nexus-fixture-generated");
  try {
    const report = check(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-generated");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.archive.file, "nexus-fixture-generated-0.1.0.crate");
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /consumer run failed/);
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
    const report = check(dir);
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
    const report = check(dir);
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
    const report = check(dir);
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

test("a consumer-supplied Cargo config is refused before any resolution", () => {
  const dir = consumerFixtureRepo(
    "consumer-supplies-cargo-config",
    "nexus-fixture-config",
  );
  try {
    const report = check(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-config");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(
      row.consumer.cause,
      /consumer fixture supplies its own Cargo config/,
    );
    assert.deepEqual(row.consumer.declared, []);
    assert.deepEqual(row.consumer.patched, []);
    assert.deepEqual(row.consumer.resolved, []);
    assert.equal(row.consumer.output, "");
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }
});

const CONSUMER_CLOSURE = [
  "nexus-contracts",
  "nexus-workflow-ir",
  "nexus-pattern",
  "nexus-knowledge",
  "nexus-narrative",
];

/**
 * @param {string} entryName Consumer fixture and packed entry name.
 * @param {string[]} entries Entries the carrier packages for this case.
 * @returns {string}
 */
function realConsumerRepo(entryName, entries) {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-"));
  const members = CONSUMER_CLOSURE.map((name) => `    "crates/${name}",`).join("\n");
  writeRepoFile(
    dir,
    "Cargo.toml",
    readFileSync(join(REPO_ROOT, "Cargo.toml"), "utf8").replace(
      /^members = \[[\s\S]*?\n\]/m,
      `members = [\n${members}\n]`,
    ),
  );
  cpSync(join(REPO_ROOT, "Cargo.lock"), join(dir, "Cargo.lock"));
  for (const name of CONSUMER_CLOSURE) {
    cpSync(join(REPO_ROOT, "crates", name), join(dir, "crates", name), {
      recursive: true,
    });
  }
  writeRepoFile(
    dir,
    VERSION_BRACKETS_PATH,
    `${JSON.stringify(
      {
        contract: entries.map((name) => ({
          name,
          manifest: `crates/${name}/Cargo.toml`,
        })),
      },
      null,
      2,
    )}\n`,
  );
  cpSync(
    join(CONSUMER_FIXTURES_DIR, entryName),
    join(dir, CONSUMER_FIXTURES_PATH, entryName),
    { recursive: true },
  );
  writeRepoFile(
    dir,
    CONSUMER_ACCOUNTING_PATH,
    consumerAccounting(
      Object.fromEntries(
        entries.map((name) => [name, name === entryName ? entryName : null]),
      ),
    ),
  );
  initGitRepo(dir);
  return dir;
}

/**
 *
 * @param {import("./check-contract-packageability.mjs").PackageabilityReport} report
 * @param {string} name
 */
function rowFor(report, name) {
  const row = report.results.find((result) => result.name === name);
  assert.ok(row !== undefined, `${name} is in the report`);
  return row;
}

/**
 * @param {string} entryName Consumer fixture and packed entry name.
 * @param {string[]} modules `modules/<name>` crates the case needs.
 * @returns {string}
 */
function moduleConsumerRepo(entryName, modules) {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-"));
  writeRepoFile(dir, "Cargo.toml", '[workspace]\nmembers = []\nresolver = "2"\n');
  for (const name of modules) {
    cpSync(join(REPO_ROOT, "modules", name), join(dir, "modules", name), {
      recursive: true,
      filter: (source) => source.split("/").at(-1) !== "target",
    });
  }
  writeRepoFile(
    dir,
    VERSION_BRACKETS_PATH,
    `${JSON.stringify(
      {
        contract: modules.map((name) => ({
          name,
          manifest: `modules/${name}/Cargo.toml`,
        })),
      },
      null,
      2,
    )}\n`,
  );
  cpSync(
    join(CONSUMER_FIXTURES_DIR, entryName),
    join(dir, CONSUMER_FIXTURES_PATH, entryName),
    { recursive: true },
  );
  writeRepoFile(
    dir,
    CONSUMER_ACCOUNTING_PATH,
    consumerAccounting(
      Object.fromEntries(
        modules.map((name) => [name, name === entryName ? entryName : null]),
      ),
    ),
  );
  initGitRepo(dir);
  return dir;
}

test("the contracts consumer round-trips a generated object from its archive", () => {
  const dir = realConsumerRepo("nexus-contracts", ["nexus-contracts"]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    assert.equal(report.firstFailure, null);

    const row = rowFor(report, "nexus-contracts");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.match(row.archive.sha256, /^[0-9a-f]{64}$/);
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.equal(row.consumer.cause, null);
    assert.deepEqual(row.consumer.patched, [
      { name: "nexus-contracts", sha256: row.archive.sha256 },
    ]);
    const consumed = row.consumer.resolved.find(
      (resolved) => resolved.name === "nexus-contracts",
    );
    assert.equal(consumed.source, `archive:${row.archive.sha256}`);
    assert.ok(consumed.manifest.includes("archives"), consumed.manifest);
    assert.match(
      row.consumer.output,
      /nexus-contracts round-tripped wld_0123456789/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the workflow-ir consumer rejects a dangling reference from its archive", () => {
  const dir = realConsumerRepo("nexus-workflow-ir", [
    "nexus-contracts",
    "nexus-workflow-ir",
  ]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-workflow-ir");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-contracts", "nexus-workflow-ir"],
    );
    for (const patched of row.consumer.patched) {
      assert.equal(
        patched.sha256,
        /** @type {{sha256: string}} */ (rowFor(report, patched.name).archive)
          .sha256,
      );
    }
    assert.match(
      row.consumer.output,
      /nexus-workflow-ir validated prompt-step \([0-9a-f]{64}\)/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the pattern consumer scans every packed package and pins its template members", () => {
  const dir = realConsumerRepo("nexus-pattern", [
    "nexus-contracts",
    "nexus-workflow-ir",
    "nexus-pattern",
  ]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-pattern");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-contracts", "nexus-pattern", "nexus-workflow-ir"],
    );
    assert.match(
      row.consumer.output,
      /nexus-pattern scanned 3 packed packages \(2 available\) and pinned 3 template members to their archive IR hashes/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a pattern archive that drops an otherwise-unreferenced template fails the packed consumer", () => {
  const dir = realConsumerRepo("nexus-pattern", [
    "nexus-contracts",
    "nexus-workflow-ir",
    "nexus-pattern",
  ]);
  try {
    rmSync(
      join(
        dir,
        "crates/nexus-pattern/tests/fixtures/patterns/step-with-judge/templates/verdict.md",
      ),
    );
    assert.equal(git(dir, ["add", "-A"]).status, 0);
    assert.equal(git(dir, ["commit", "-qm", "drop packed template"]).status, 0);

    const report = check(dir);
    assert.equal(report.ok, false);
    assert.notEqual(report.firstFailure, null);

    const row = rowFor(report, "nexus-pattern");
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /consumer run failed/);
    assert.match(
      row.consumer.output,
      // The archive the consumer reads is extracted under the shared cache, one
      // directory per source digest (`archives/<sha256>/`).
      /archives\/[0-9a-f]{64}\/src\/\.\.\/tests\/fixtures\/patterns\/step-with-judge\/templates\/verdict\.md/,
      row.consumer.output,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the module-sdk consumer builds a wasm32 guest through nexus_entry and the host API separately", () => {
  const dir = moduleConsumerRepo("nexus-module-sdk", ["nexus-module-sdk"]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-module-sdk");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.equal(row.consumer.mode, "run");
    assert.deepEqual(row.consumer.patched, [
      { name: "nexus-module-sdk", sha256: row.archive.sha256 },
    ]);
    assert.match(
      row.consumer.output,
      /nexus-module-sdk host API validated consumer-probe/,
    );

    assert.match(row.consumer.guest.artifact, /\.wasm$/);
    assert.ok(row.consumer.guest.bytes > 8, String(row.consumer.guest.bytes));
    assert.match(row.consumer.guest.sha256, /^[0-9a-f]{64}$/);

    const guestManifest = readFileSync(
      join(CONSUMER_FIXTURES_DIR, "nexus-module-sdk/guest/Cargo.toml"),
      "utf8",
    );
    const dependencies = guestManifest
      .split(/^\[/m)
      .find((section) => section.startsWith("dependencies]"))
      ?.split("\n")
      .map((line) => line.match(/^([\w-]+)\s*=/)?.[1])
      .filter((name) => name !== undefined);
    assert.deepEqual(dependencies, ["nexus-module-sdk"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the module-manifest consumer validates, injects and verifies the digest and refuses the invalid corpus", () => {
  const dir = moduleConsumerRepo("nexus-module-manifest", [
    "nexus-module-manifest",
  ]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-module-manifest");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.equal(row.consumer.guest, null);
    assert.match(
      row.consumer.output,
      /nexus-module-manifest validated, injected 93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476, verified the pairing, and refused the invalid corpus entry/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the module-test consumer runs its ABI tests against the dev-dependency from the archive", () => {
  const dir = moduleConsumerRepo("nexus-module-test", [
    "nexus-module-sdk",
    "nexus-module-test",
  ]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-module-test");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-module-sdk", "nexus-module-test"],
    );
    assert.equal(row.consumer.mode, "test");
    assert.match(row.consumer.output, /running 2 tests/);
    assert.match(row.consumer.output, /test result: ok\. 2 passed/);
    assert.match(
      row.consumer.output,
      /nexus-module-test ran a tiny ABI-valid module and rejected a malformed output envelope/,
    );

    const fixtureManifest = readFileSync(
      join(CONSUMER_FIXTURES_DIR, "nexus-module-test/Cargo.toml"),
      "utf8",
    );
    const section = (header) =>
      fixtureManifest
        .split(/^\[/m)
        .find((part) => part.startsWith(`${header}]`)) ?? "";
    assert.match(section("dev-dependencies"), /^nexus-module-test\s*=/m);
    assert.doesNotMatch(section("dependencies"), /^nexus-module-test\s*=/m);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the narrative consumer reads a world and its KbStore entries from archives, not the original source escape", () => {
  const dir = realConsumerRepo("nexus-narrative", [
    "nexus-contracts",
    "nexus-knowledge",
    "nexus-narrative",
  ]);
  try {
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-narrative");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-contracts", "nexus-knowledge", "nexus-narrative"],
    );
    assert.match(
      row.consumer.output,
      /nexus-narrative read back world wld_consumer with entry kb_/,
    );

    for (const name of ["nexus-contracts", "nexus-knowledge", "nexus-narrative"]) {
      const resolved = row.consumer.resolved.find((entry) => entry.name === name);
      assert.equal(
        resolved.source,
        `archive:${/** @type {{sha256: string}} */ (rowFor(report, name).archive).sha256}`,
      );
      assert.ok(resolved.manifest.includes("archives"), resolved.manifest);
      assert.equal(
        resolved.manifest.startsWith(dir),
        false,
        `${name} resolved from the original checkout`,
      );
    }
    assert.equal(rowFor(report, "nexus-knowledge").consumer, null);
    assert.deepEqual(rowFor(report, "nexus-knowledge").coveredBy, [
      `${CONSUMER_FIXTURES_PATH}/nexus-narrative`,
    ]);
    assert.deepEqual(rowFor(report, "nexus-contracts").coveredBy, [
      `${CONSUMER_FIXTURES_PATH}/nexus-narrative`,
    ]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a tracked relative symlink stages verbatim, so a clean checkout is clean evidence", () => {
  const dir = tinyFixture();
  try {
    writeRepoFile(dir, ".agents/skills/probe/SKILL.md", "# probe\n");
    mkdirSync(join(dir, ".claude/skills"), { recursive: true });
    symlinkSync(
      "../../.agents/skills/probe",
      join(dir, ".claude/skills/probe"),
    );
    symlinkSync("../../.agents/skills/absent", join(dir, ".claude/skills/absent"));
    gitOk(dir, ["add", "-A"]);
    gitOk(dir, [
      "-c",
      "user.email=fixture@example.com",
      "-c",
      "user.name=Fixture",
      "commit",
      "-qm",
      "track a relative link",
    ]);

    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    assert.equal(report.source.clean, true, JSON.stringify(report.source));
    assert.equal(report.source.mismatch, undefined);
    const sha = report.source.sha;

    const stage = mkdtempSync(join(tmpdir(), "nexus-stage-link-"));
    try {
      cpSync(dir, stage, {
        recursive: true,
        verbatimSymlinks: true,
        filter: (source) => !source.split("/").includes(".git"),
      });
      assert.deepEqual(verifyStagedSource(dir, sha, stage), []);
      rmSync(join(stage, ".claude/skills/probe"));
      symlinkSync("../../.agents/skills/other", join(stage, ".claude/skills/probe"));
      assert.ok(
        verifyStagedSource(dir, sha, stage).includes(".claude/skills/probe"),
        "a retargeted link is a mismatch",
      );
    } finally {
      rmSync(stage, { recursive: true, force: true });
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an archive no passing consumer consumes fails as a pack-only result", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    writeRepoFile(dir, `${TINY_CRATE_DIR}/Cargo.toml`, TINY_MANIFEST);
    writeRepoFile(dir, `${TINY_CRATE_DIR}/src/lib.rs`, "pub fn tiny() {}\n");
    const carrier = JSON.parse(readFileSync(join(dir, VERSION_BRACKETS_PATH), "utf8"));
    writeRepoFile(
      dir,
      VERSION_BRACKETS_PATH,
      `${JSON.stringify(
        {
          contract: [
            ...carrier.contract,
            {
              name: "nexus-tiny-contract",
              manifest: `${TINY_CRATE_DIR}/Cargo.toml`,
            },
          ],
        },
        null,
        2,
      )}\n`,
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({
        "nexus-fixture-public": "nexus-fixture-public",
        "nexus-tiny-contract": null,
      }),
    );

    const report = check(dir);
    assert.equal(report.ok, false);
    const uncovered = rowFor(report, "nexus-tiny-contract");
    assert.equal(uncovered.ok, false);
    assert.equal(
      uncovered.cause,
      "pack-only: no passing consumer consumes nexus-tiny-contract",
    );
    const covered = rowFor(report, "nexus-fixture-public");
    assert.equal(covered.ok, true, covered.cause ?? "");
    assert.equal(covered.consumer.ok, true);
    assert.deepEqual(covered.coveredBy, [
      `${CONSUMER_FIXTURES_PATH}/nexus-fixture-public`,
    ]);
    assert.equal(uncovered.coveredBy, null);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("--require-clean refuses a dirty tree and emits no receipt", () => {
  const dir = tinyFixture();
  try {
    writeRepoFile(dir, `${TINY_CRATE_DIR}/src/late.rs`, "pub fn late() {}\n");
    const refused = runReleaseScript(
      "check-contract-packageability.mjs",
      ["--require-clean", "--json"],
      dir,
    );
    assert.equal(refused.status, 1, refused.stderr);
    assert.match(refused.stderr, /refused: unclean-evidence/);
    assert.equal(refused.stdout.trim(), "", "no receipt on an unclean tree");

    const plain = runReleaseScript(
      "check-contract-packageability.mjs",
      ["--json"],
      dir,
    );
    assert.equal(plain.status, 0, plain.stderr);
    assert.equal(JSON.parse(plain.stdout).source.clean, false);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a filtered path is compared by the blob Git stores, not its working-tree bytes", () => {
  const dir = tinyFixture();
  const stage = mkdtempSync(join(tmpdir(), "nexus-stage-filter-"));
  try {
    writeRepoFile(dir, ".gitattributes", "*.upper filter=upper\n");
    writeRepoFile(dir, "notes.upper", "hello\n");
    assert.equal(git(dir, ["config", "filter.upper.clean", "tr a-z A-Z"]).status, 0);
    gitOk(dir, ["add", "-A"]);
    gitOk(dir, [
      "-c",
      "user.email=fixture@example.com",
      "-c",
      "user.name=Fixture",
      "commit",
      "-qm",
      "track a filtered file",
    ]);
    const sha = gitOk(dir, ["rev-parse", "HEAD"]);
    assert.equal(
      git(dir, ["show", `${sha}:notes.upper`]).stdout,
      "HELLO\n",
      "the stored blob is the filtered content",
    );

    cpSync(dir, stage, {
      recursive: true,
      verbatimSymlinks: true,
      filter: (source) => !source.split("/").includes(".git"),
    });
    assert.equal(readFileSync(join(stage, "notes.upper"), "utf8"), "hello\n");
    assert.deepEqual(verifyStagedSource(dir, sha, stage), []);
  } finally {
    rmSync(stage, { recursive: true, force: true });
    cleanupTempRepo(dir);
  }
});

test("a patch a consumer declares but never resolves does not count as coverage", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, `${TINY_CRATE_DIR}/Cargo.toml`, TINY_MANIFEST);
    writeRepoFile(dir, `${TINY_CRATE_DIR}/src/lib.rs`, "pub fn tiny() {}\n");
    writeRepoFile(
      dir,
      VERSION_BRACKETS_PATH,
      `${JSON.stringify(
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
    );
    writeRepoFile(
      dir,
      `${CONSUMER_FIXTURES_PATH}/nexus-tiny-contract/Cargo.toml`,
      [
        "[package]",
        'name = "nexus-tiny-consumer"',
        'version = "0.0.0"',
        'edition = "2021"',
        "",
        "[dependencies]",
        "# Optional and enabled by no feature: declared, never resolved.",
        'nexus-tiny-contract = { version = "0.1.0", optional = true }',
        "",
        "[workspace]",
        "",
      ].join("\n"),
    );
    writeRepoFile(
      dir,
      `${CONSUMER_FIXTURES_PATH}/nexus-tiny-contract/src/main.rs`,
      'fn main() {\n    println!("declared, never resolved");\n}\n',
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({ "nexus-tiny-contract": "nexus-tiny-contract" }),
    );
    initGitRepo(dir);

    const report = check(dir);
    assert.equal(report.ok, false);
    const row = rowFor(report, "nexus-tiny-contract");
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-tiny-contract"],
    );
    assert.equal(
      row.consumer.resolved.some((entry) => entry.source.startsWith("archive:")),
      false,
      JSON.stringify(row.consumer.resolved),
    );
    assert.equal(
      row.cause,
      "unresolved-coverage: nexus-tiny-contract was patched into a consumer but resolution never used its archive",
    );
    assert.equal(row.coveredBy, null);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a repository without the consumer accounting fails instead of packing", () => {
  const dir = tinyFixture();
  try {
    rmSync(join(dir, CONSUMER_ACCOUNTING_PATH));
    assert.throws(
      () => check(dir),
      /consumer accounting failed: tooling\/release\/fixtures\/package-consumers\.json is missing/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("removing a declared fixture fails until the accounting is edited", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    // "fixtures were removed" must not read as "no fixtures by design".
    rmSync(join(dir, CONSUMER_FIXTURES_PATH, "nexus-fixture-public"), {
      recursive: true,
      force: true,
    });
    assert.throws(
      () => check(dir),
      /consumer accounting failed: .*nexus-fixture-public declares the fixture nexus-fixture-public, which is not in the tree/,
    );

    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({ "nexus-fixture-public": null }),
    );
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    assert.equal(report.consumerAccounting.fixtures.length, 0);
    assert.deepEqual(
      report.consumerAccounting.noFixture.map((row) => row.name),
      ["nexus-fixture-public"],
    );
    const [row] = report.results;
    assert.equal(row.consumer, null);
    assert.equal(row.coveredBy, null);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the declared fixture directory is the one that runs", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    renameSync(
      join(dir, CONSUMER_FIXTURES_PATH, "nexus-fixture-public"),
      join(dir, CONSUMER_FIXTURES_PATH, "fixture-renamed"),
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({ "nexus-fixture-public": "fixture-renamed" }),
    );
    const report = check(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    const row = rowFor(report, "nexus-fixture-public");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.equal(row.consumer.case, `${CONSUMER_FIXTURES_PATH}/fixture-renamed`);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("declaring another entry's fixture fails as a misdirected declaration", () => {
  const dir = realConsumerRepo("nexus-pattern", [
    "nexus-contracts",
    "nexus-workflow-ir",
    "nexus-pattern",
  ]);
  try {
    cpSync(
      join(CONSUMER_FIXTURES_DIR, "nexus-contracts"),
      join(dir, CONSUMER_FIXTURES_PATH, "nexus-contracts"),
      { recursive: true },
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({
        "nexus-contracts": "nexus-contracts",
        "nexus-workflow-ir": "nexus-contracts",
        "nexus-pattern": "nexus-pattern",
      }),
    );
    gitOk(dir, ["add", "-A"]);
    gitOk(dir, ["commit", "-qm", "declare the wrong fixture"]);

    const report = check(dir);
    assert.equal(report.ok, false);
    const misdirected = rowFor(report, "nexus-workflow-ir");
    assert.equal(misdirected.ok, false);
    assert.equal(misdirected.consumer.ok, true, misdirected.consumer.cause ?? "");
    assert.equal(
      misdirected.consumer.case,
      `${CONSUMER_FIXTURES_PATH}/nexus-contracts`,
    );
    assert.equal(
      misdirected.cause,
      `misdirected-fixture: nexus-workflow-ir declares ${CONSUMER_FIXTURES_PATH}/nexus-contracts, but only ${CONSUMER_FIXTURES_PATH}/nexus-pattern resolved its archive`,
    );
    assert.equal(misdirected.coveredBy, null);
    for (const name of ["nexus-contracts", "nexus-pattern"]) {
      const row = rowFor(report, name);
      assert.equal(row.ok, true, row.cause ?? "");
    }
    assert.deepEqual(rowFor(report, "nexus-contracts").coveredBy, [
      `${CONSUMER_FIXTURES_PATH}/nexus-contracts`,
      `${CONSUMER_FIXTURES_PATH}/nexus-pattern`,
    ]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the consumer accounting must name every carrier entry once", () => {
  const dir = tinyFixture();
  try {
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({ "nexus-other-contract": null }),
    );
    assert.throws(
      () => check(dir),
      /records nexus-other-contract, which is not a carrier entry/,
    );
    writeRepoFile(dir, CONSUMER_ACCOUNTING_PATH, "{}\n");
    assert.throws(
      () => check(dir),
      /nexus-tiny-contract is not recorded/,
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      `${JSON.stringify({ "nexus-tiny-contract": { fixture: null } })}\n`,
    );
    assert.throws(
      () => check(dir),
      /nexus-tiny-contract has no fixture, so it needs a reason/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

/**
 * An extracted archive's shape, minus Cargo: the same sources with only the
 * staging stamp differing is exactly how two cases of equal content arrive here,
 * so {@link treeDigest} has to call them one entry.
 *
 * @param {string} parent
 * @param {string} stamp
 * @returns {string}
 */
function extractedVariant(parent, stamp) {
  const root = join(parent, `extracted-${stamp}`);
  mkdirSync(join(root, "src"), { recursive: true });
  writeFileSync(
    join(root, "Cargo.toml"),
    '[package]\nname = "nexus-publish-race"\nversion = "0.1.0"\n',
  );
  writeFileSync(join(root, "src", "lib.rs"), "pub fn race() {}\n");
  writeFileSync(
    join(root, ".cargo_vcs_info.json"),
    `{"git":{"sha1":"${stamp}"}}\n`,
  );
  return root;
}

/** This checker, as a URL a child process can import. */
const CHECKER_URL = pathToFileURL(
  join(REPO_ROOT, "tooling/release/check-contract-packageability.mjs"),
).href;

/** Child publisher: one archive, one share root, printed result. */
const PUBLISH_CHILD = `
import { shareArchive } from ${JSON.stringify(CHECKER_URL)};
process.stdout.write(
  shareArchive(process.env.PUBLISH_ROOT, process.env.PUBLISH_SHARE),
);
`;

const execFileAsync = promisify(execFile);

test("two publishers of one digest keep one directory and drop the loser's copy", async () => {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-race-"));
  const share = join(dir, "share");
  const first = extractedVariant(dir, "aaaa");
  const second = extractedVariant(dir, "bbbb");
  try {
    const publish = (root) =>
      execFileAsync(process.execPath, ["--input-type=module", "-e", PUBLISH_CHILD], {
        encoding: "utf8",
        env: { ...process.env, PUBLISH_ROOT: root, PUBLISH_SHARE: share },
      });
    const results = await Promise.all([publish(first), publish(second)]);
    const published = results.map((result) => result.stdout.trim());

    assert.equal(
      published[0],
      published[1],
      "both publishers must resolve the same entry",
    );
    assert.ok(
      published[0].startsWith(join(share, "archives")),
      `published path ${published[0]} is under the share root`,
    );
    assert.deepEqual(
      readdirSync(join(share, "archives")),
      [basename(published[0])],
      "the digest owns exactly one directory",
    );
    assert.deepEqual(
      [first, second].filter((root) => existsSync(root)),
      [],
      "each publisher's own extraction is gone",
    );
    assert.match(
      readFileSync(join(published[0], "src", "lib.rs"), "utf8"),
      /pub fn race/,
      "the published entry carries the sources",
    );

    const late = extractedVariant(dir, "cccc");
    assert.equal(
      shareArchive(late, share),
      published[0],
      "a publish that loses to an existing entry reads the winner",
    );
    assert.equal(existsSync(late), false, "the loser drops its own extraction");
    assert.equal(
      readdirSync(join(share, "archives")).length,
      1,
      "the loser does not add a second directory",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a link in the sources hashes by its stored target", () => {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-link-"));
  const share = join(dir, "share");
  const variant = (stamp, target) => {
    const root = extractedVariant(dir, stamp);
    symlinkSync(target, join(root, "src", "linked.rs"));
    return root;
  };
  const first = variant("a1", "../real.rs");
  const second = variant("b2", "../real.rs");
  const third = variant("c3", "../other.rs");
  try {
    const entry = shareArchive(first, share);
    assert.ok(entry.startsWith(join(share, "archives")), entry);
    assert.equal(
      shareArchive(second, share),
      entry,
      "same bytes and same link target publish one entry",
    );
    assert.notEqual(
      shareArchive(third, share),
      entry,
      "a different link target is a different entry",
    );
    assert.equal(readdirSync(join(share, "archives")).length, 2);
    assert.equal(
      readlinkSync(join(entry, "src", "linked.rs")),
      "../real.rs",
      "the published entry keeps the link itself",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

/** Spawns the marker writer and then blocks, so only a group kill ends it. */
const GROUP_SPAWNER = `
const { spawn } = require("node:child_process");
spawn(process.execPath, ["-e", "setInterval(() => require('node:fs').appendFileSync(process.env.MARKER, '.'), 100)"], { stdio: "ignore" });
setInterval(() => {}, 1000);
`;

/**
 * Spawns the marker writer, then kills itself with SIGTERM once the writer has
 * been observed working (two writes by default), so the precondition is established
 * by the marker rather than by a wall clock a loaded runner may not meet. If the
 * writes never arrive within 5 s it reports that as its own failure (exit 4) instead
 * of killing itself. That bound is the shorter one on purpose: the test's race
 * guard is 12 s, so a genuinely unmet precondition surfaces as this exit, never as a
 * forwarding defect. `NEXUS_FIXTURE_MIN_WRITES` raises the bar for that control.
 */
const SUICIDE_SPAWNER = `
const { spawn } = require("node:child_process");
const { existsSync, readFileSync, writeSync } = require("node:fs");
const park = new Int32Array(new SharedArrayBuffer(4));
// fd 2 is a non-blocking pipe under the supervisor, so a full pipe surfaces as
// EAGAIN: retry synchronously for a few milliseconds. This never throws - a line
// that cannot be written must not cost the fixture the exit code its case asserts.
const say = (text) => {
  try {
    const bytes = Buffer.from(text + "\\n");
    let offset = 0;
    const until = Date.now() + 100;
    while (offset < bytes.length) {
      try {
        offset += writeSync(2, bytes, offset);
      } catch (error) {
        if (error.code !== "EAGAIN" || Date.now() > until) {
          return;
        }
        Atomics.wait(park, 0, 0, 2);
      }
    }
  } catch {
    // Best effort only: nothing else can carry the message out.
  }
};
const rawMinimum = (process.env.NEXUS_FIXTURE_MIN_WRITES ?? "").trim();
const minimumWrites = rawMinimum.length === 0 ? 2 : Number(rawMinimum);
if (!Number.isInteger(minimumWrites) || minimumWrites < 1) {
  say('fixture: NEXUS_FIXTURE_MIN_WRITES must be a positive integer, got "' + rawMinimum + '"');
  process.exit(5);
}
const writer = spawn(process.execPath, ["-e", "setInterval(() => require('node:fs').appendFileSync(process.env.MARKER, '.'), 100)"], { stdio: "ignore" });
const deadline = Date.now() + 5000;
const writes = (path) => (existsSync(path) ? readFileSync(path, "utf8").length : 0);
const poll = setInterval(() => {
  const seen = writes(process.env.MARKER);
  if (seen >= minimumWrites) {
    clearInterval(poll);
    process.kill(process.pid, "SIGTERM");
    return;
  }
  if (Date.now() > deadline) {
    clearInterval(poll);
    try {
      say("fixture: the marker writer never reached " + minimumWrites + " writes");
    } finally {
      // Nothing reaps this on the way out: the supervisor forwards an exit code
      // instead of killing a group, so the fixture owns its writer - even when the
      // line above could not be written.
      writer.kill("SIGKILL");
    }
    process.exit(4);
  }
}, 25);
`;

/** Child that spends its deadline inside {@link run}. */
const GROUP_CHILD = `
import { run } from ${JSON.stringify(CHECKER_URL)};
const result = run(process.execPath, ["-e", ${JSON.stringify(GROUP_SPAWNER)}], process.cwd());
process.stdout.write(JSON.stringify({ ok: result.ok, output: result.output }));
`;

/** Child that sits inside {@link run} until something kills it. */
const INTERRUPT_CHILD = `
import { run } from ${JSON.stringify(CHECKER_URL)};
const result = run(process.execPath, ["-e", ${JSON.stringify(GROUP_SPAWNER)}], process.cwd());
process.stdout.write(JSON.stringify({ ok: result.ok }));
`;

/**
 * Markers written so far, so a phase can wait for the writer to be live instead
 * of racing its startup against the deadline.
 *
 * @param {string} marker
 * @returns {number}
 */
function markerWrites(marker) {
  return existsSync(marker) ? readFileSync(marker, "utf8").length : 0;
}

/**
 * @param {() => boolean} predicate
 * @param {number} timeout
 */
async function waitFor(predicate, timeout) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (predicate()) {
      return;
    }
    await new Promise((done) => setTimeout(done, 50));
  }
  throw new Error(`condition not met within ${timeout}ms`);
}

/**
 * Waits (bounded) for the marker to stop changing. A supervisor notices its
 * parent's death on a 100 ms poll, so the writer can legitimately land one more
 * byte after the caller is gone; what must hold is that the content settles across
 * a 500 ms window and that a writer which never settles fails.
 *
 * @param {string} marker
 * @param {number} [timeout]
 */
async function assertMarkerSettles(marker, timeout = 3000) {
  const limit = Date.now() + timeout;
  let previous = readFileSync(marker, "utf8");
  let stableSince = Date.now();
  while (Date.now() < limit) {
    await new Promise((done) => setTimeout(done, 50));
    const current = readFileSync(marker, "utf8");
    if (current === previous) {
      if (Date.now() - stableSince >= 500) {
        return;
      }
      continue;
    }
    previous = current;
    stableSince = Date.now();
  }
  assert.fail(`the writer kept writing for ${timeout}ms (${previous.length} writes)`);
}

test(
  "a child over its deadline is killed with the grandchildren it started",
  { skip: process.platform === "win32" ? "no process groups on Windows" : false },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "nexus-release-group-"));
    const marker = join(dir, "marker.txt");
    try {
      // Phase 1: the writer is live (two writes observed) before any deadline.
      // Phase 2: the deadline fires with the writer still working.
      const pending = execFileAsync(
        process.execPath,
        ["--input-type=module", "-e", GROUP_CHILD],
        {
          encoding: "utf8",
          env: {
            ...process.env,
            MARKER: marker,
            NEXUS_RELEASE_DEADLINE_MS: String(Date.now() + 5000),
          },
        },
      );
      await waitFor(() => markerWrites(marker) >= 2, 4000);
      const { stdout } = await pending;

      const result = JSON.parse(stdout);
      assert.equal(result.ok, false);
      assert.match(
        result.output,
        /error: .* exceeded the release tooling deadline \(NEXUS_RELEASE_DEADLINE_MS\)/,
      );
      assert.notEqual(
        readFileSync(marker, "utf8"),
        "",
        "the grandchild wrote before the kill",
      );
      await assertMarkerSettles(marker);
    } finally {
      cleanupTempRepo(dir);
    }
  },
);

test(
  "interrupting the checker takes its running child down with it",
  { skip: process.platform === "win32" ? "no process groups on Windows" : false },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "nexus-release-interrupt-"));
    const marker = join(dir, "marker.txt");
    try {
      const checker = spawn(
        process.execPath,
        ["--input-type=module", "-e", INTERRUPT_CHILD],
        {
          stdio: "ignore",
          env: {
            ...process.env,
            MARKER: marker,
            // Far away: the signal, not the deadline, must end this child.
            NEXUS_RELEASE_DEADLINE_MS: String(Date.now() + 30000),
          },
        },
      );
      try {
        await waitFor(() => markerWrites(marker) >= 2, 4000);
        const closed = new Promise((done) =>
          checker.on("close", (code, signal) => done({ code, signal })),
        );
        checker.kill("SIGTERM");
        const exit = await Promise.race([
          closed,
          new Promise((done) => setTimeout(() => done("still-running"), 5000)),
        ]);
        assert.notEqual(exit, "still-running", "the checker died on SIGTERM");
        assert.notEqual(
          readFileSync(marker, "utf8"),
          "",
          "the grandchild wrote before the signal",
        );
        await assertMarkerSettles(marker);
      } finally {
        checker.kill("SIGKILL");
      }
    } finally {
      cleanupTempRepo(dir);
    }
  },
);

/**
 * Settles a marker that may never have been created: a signal delivered before the
 * command starts leaves nothing to reap, which is a valid outcome, but anything
 * that did start must stop.
 *
 * @param {string} marker
 * @param {number} [timeout]
 */
async function assertNothingWrites(marker, timeout = 3000) {
  const limit = Date.now() + timeout;
  while (!existsSync(marker) && Date.now() < limit) {
    await new Promise((done) => setTimeout(done, 50));
  }
  if (!existsSync(marker)) {
    return;
  }
  await assertMarkerSettles(marker, timeout);
}

/**
 * The supervisor's own pid, from the file it publishes at startup. A caller that
 * has to signal it outright cannot know that pid any other way, and this keeps the
 * test off external tools.
 *
 * @param {string} pidFile
 * @param {number} [timeout]
 * @returns {Promise<number>}
 */
async function supervisorPidFrom(pidFile, timeout = 4000) {
  const limit = Date.now() + timeout;
  while (Date.now() < limit) {
    if (existsSync(pidFile)) {
      const pid = Number(readFileSync(pidFile, "utf8").trim());
      if (Number.isInteger(pid) && pid > 0) {
        return pid;
      }
    }
    await new Promise((done) => setTimeout(done, 25));
  }
  assert.fail(`the supervisor published no pid in ${pidFile} within ${timeout}ms`);
}

test(
  "a supervisor killed outright leaves the caller a handle on its group",
  { skip: process.platform === "win32" ? "no process groups on Windows" : false },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "nexus-release-backstop-"));
    const marker = join(dir, "marker.txt");
    const supervisorPidFile = join(dir, "supervisor.pid");
    try {
      const checker = spawn(
        process.execPath,
        ["--input-type=module", "-e", GROUP_CHILD],
        {
          stdio: ["ignore", "pipe", "ignore"],
          env: {
            ...process.env,
            MARKER: marker,
            NEXUS_SUPERVISOR_PID_FILE: supervisorPidFile,
            // A killed supervisor keeps the caller's stdio open through the
            // command, so `spawnSync` cannot return at the kill: it returns when
            // this deadline fires, which is when the backstop has to act. It sits
            // above the two bounded preconditions below (pid file 4 s + marker
            // 4 s) with room to spare, so a slow precondition never reads as a
            // deadline.
            NEXUS_RELEASE_DEADLINE_MS: String(Date.now() + 15000),
          },
        },
      );
      let stdout = "";
      checker.stdout?.on("data", (chunk) => {
        stdout += chunk;
      });
      try {
        const supervisor = await supervisorPidFrom(supervisorPidFile);
        await waitFor(() => markerWrites(marker) >= 2, 4000);
        // Listen before killing: a checker that exits promptly must be observed
        // as an exit, not as a guard that never saw one.
        const closed = new Promise((done) =>
          checker.on("close", (code, signal) => done({ code, signal })),
        );
        process.kill(supervisor, "SIGKILL");

        const exit = await Promise.race([
          closed,
          new Promise((done) => setTimeout(() => done("still-running"), 20000)),
        ]);
        assert.notEqual(
          exit,
          "still-running",
          "the caller returned after its supervisor was killed",
        );
        const reported = JSON.parse(stdout);
        assert.equal(reported.ok, false, "a killed child is a failed run");
        assert.match(
          reported.output,
          /exceeded the release tooling deadline/,
          "the deadline is what ends the wait",
        );
        await assertMarkerSettles(marker);
      } finally {
        checker.kill("SIGKILL");
      }
    } finally {
      cleanupTempRepo(dir);
    }
  },
);

test(
  "a signal in the first milliseconds still ends the group it started",
  { skip: process.platform === "win32" ? "no process groups on Windows" : false },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "nexus-release-early-"));
    const marker = join(dir, "marker.txt");
    try {
      const supervisor = spawn(
        process.execPath,
        [SUPERVISOR_PATH, process.execPath, "-e", GROUP_SPAWNER],
        {
          detached: true,
          stdio: "ignore",
          env: { ...process.env, MARKER: marker },
        },
      );
      try {
        const closed = new Promise((done) =>
          supervisor.on("close", (code, signal) => done({ code, signal })),
        );
        await new Promise((done) => setTimeout(done, 50));
        supervisor.kill("SIGTERM");
        const exit = await Promise.race([
          closed,
          new Promise((done) => setTimeout(() => done("still-running"), 5000)),
        ]);
        assert.notEqual(exit, "still-running", "the supervisor died on the signal");
        await assertNothingWrites(marker);
      } finally {
        supervisor.kill("SIGKILL");
      }
    } finally {
      cleanupTempRepo(dir);
    }
  },
);

test("a link and a plain file with the same bytes are different entries", () => {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-linkfile-"));
  const share = join(dir, "share");
  const linked = extractedVariant(dir, "plain");
  symlinkSync("../real.rs", join(linked, "src", "linked.rs"));
  const plain = extractedVariant(dir, "twice");
  writeFileSync(join(plain, "src", "linked.rs"), "../real.rs");
  try {
    const linkEntry = shareArchive(linked, share);
    const plainEntry = shareArchive(plain, share);
    assert.notEqual(
      plainEntry,
      linkEntry,
      "a link hashes as its target, not as the bytes of a file that spells it",
    );
    assert.equal(readdirSync(join(share, "archives")).length, 2);
  } finally {
    cleanupTempRepo(dir);
  }
});

/**
 * Child that presents itself as Windows to {@link run}: it redefines the platform
 * before a dynamic import, so the module takes the non-supervised branch and the
 * timeout still has to bound the child. Its deadline starts *after* the import, so
 * a slow import can never expire it before the call; the sleeper is far longer
 * than that deadline, so an early return can only come from the timeout.
 */
const WIN32_SHAPE_PROBE = `
Object.defineProperty(process, "platform", { value: "win32" });
const { run } = await import(${JSON.stringify(CHECKER_URL)});
process.env.NEXUS_RELEASE_DEADLINE_MS = String(Date.now() + 2000);
const started = Date.now();
const result = run(process.execPath, ["-e", "setTimeout(() => {}, 20000)"], process.cwd());
process.stdout.write(
  JSON.stringify({ ok: result.ok, output: result.output, elapsed: Date.now() - started }),
);
`;

test("a child without process groups is still bounded by the deadline", async () => {
  const env = { ...process.env };
  delete env.NEXUS_RELEASE_DEADLINE_MS;
  const { stdout } = await execFileAsync(
    process.execPath,
    ["--input-type=module", "-e", WIN32_SHAPE_PROBE],
    { encoding: "utf8", env },
  );
  const result = JSON.parse(stdout);
  assert.equal(result.ok, false);
  assert.match(
    result.output,
    /error: .* exceeded the release tooling deadline \(NEXUS_RELEASE_DEADLINE_MS\)/,
  );
  assert.ok(
    result.elapsed > 1000 && result.elapsed < 6000,
    `the 20 s child had to die at the probe's own 2 s deadline, took ${result.elapsed}ms`,
  );
});

/** The supervisor, as the caller under test invokes it. */
const SUPERVISOR_PATH = join(
  REPO_ROOT,
  "tooling/release/supervise-child.mjs",
);

test(
  "the supervisor forwards the command's exit code and dies by its own signal",
  { skip: process.platform === "win32" ? "no process groups on Windows" : false },
  async () => {
    const dir = mkdtempSync(join(tmpdir(), "nexus-release-supervisor-"));
    const marker = join(dir, "marker.txt");
    try {
      const exiting = spawnSync(
        process.execPath,
        [SUPERVISOR_PATH, process.execPath, "-e", "process.exit(3)"],
        { detached: true, encoding: "utf8" },
      );
      assert.equal(exiting.status, 3, exiting.stderr);

      const missing = spawnSync(
        process.execPath,
        [SUPERVISOR_PATH, "/nonexistent-nexus-command"],
        { detached: true, encoding: "utf8" },
      );
      assert.equal(missing.status, 127);
      assert.match(missing.stderr, /could not run/);

      const supervisor = spawn(
        process.execPath,
        [SUPERVISOR_PATH, process.execPath, "-e", GROUP_SPAWNER],
        {
          detached: true,
          stdio: "ignore",
          env: { ...process.env, MARKER: marker },
        },
      );
      try {
        await waitFor(() => markerWrites(marker) >= 2, 4000);
        const closed = new Promise((done) =>
          supervisor.on("close", (code, signal) => done({ code, signal })),
        );
        supervisor.kill("SIGTERM");
        const exit = await Promise.race([
          closed,
          new Promise((done) => setTimeout(() => done("still-running"), 5000)),
        ]);
        assert.notEqual(exit, "still-running", "the supervisor died on SIGTERM");
        assert.equal(
          exit.signal,
          "SIGTERM",
          "the caller reads the signal, not exit 0",
        );
        await assertMarkerSettles(marker);
      } finally {
        supervisor.kill("SIGKILL");
      }

      // The command dying by a signal is the other re-raise path: the supervisor
      // has a listener for that signal installed, so it has to drop it first or
      // the process keeps running instead of dying the way the command did.
      const commandPidFile = join(dir, "command.pid");
      const forwarding = spawn(
        process.execPath,
        [
          SUPERVISOR_PATH,
          process.execPath,
          "-e",
          "require('node:fs').writeFileSync(process.env.PID_FILE, String(process.pid)); setInterval(() => {}, 1000)",
        ],
        {
          detached: true,
          stdio: "ignore",
          env: { ...process.env, PID_FILE: commandPidFile },
        },
      );
      try {
        await waitFor(() => existsSync(commandPidFile), 4000);
        const closed = new Promise((done) =>
          forwarding.on("close", (code, signal) => done({ code, signal })),
        );
        process.kill(Number(readFileSync(commandPidFile, "utf8")), "SIGTERM");
        const exit = await Promise.race([
          closed,
          new Promise((done) => setTimeout(() => done("still-running"), 5000)),
        ]);
        assert.notEqual(exit, "still-running", "the supervisor followed the command");
        assert.equal(
          exit.signal,
          "SIGTERM",
          "the command's signal reaches the caller, not a swallowed exit",
        );
      } finally {
        forwarding.kill("SIGKILL");
      }

      // A command that dies by a signal can leave its own group running; the
      // supervisor must reap that group before it re-raises and exits.
      const suicideMarker = join(dir, "suicide.txt");
      const suicide = spawn(
        process.execPath,
        [SUPERVISOR_PATH, process.execPath, "-e", SUICIDE_SPAWNER],
        {
          detached: true,
          stdio: "ignore",
          env: { ...process.env, MARKER: suicideMarker },
        },
      );
      try {
        const closed = new Promise((done) =>
          suicide.on("close", (code, signal) => done({ code, signal })),
        );
        const exit = await Promise.race([
          closed,
          // Above the fixture's own 5 s bound, so an unmet precondition exits 4
          // and is reported as that, not as a guard that fired first.
          new Promise((done) => setTimeout(() => done("still-running"), 12000)),
        ]);
        assert.notEqual(exit, "still-running", "the supervisor followed the command");
        assert.notEqual(
          exit.code,
          5,
          "fixture: NEXUS_FIXTURE_MIN_WRITES must be a positive integer",
        );
        assert.notEqual(
          exit.code,
          4,
          "precondition: the marker writer never reached two writes within 5 s",
        );
        assert.equal(exit.signal, "SIGTERM", "the command's signal reaches the caller");
        const writes = markerWrites(suicideMarker);
        assert.ok(
          writes >= 2,
          `the grandchild ran before the command died (${writes} writes)`,
        );
        await assertMarkerSettles(suicideMarker);
      } finally {
        suicide.kill("SIGKILL");
      }
    } finally {
      cleanupTempRepo(dir);
    }
  },
);

/**
 * The fixture's bound parses like `NEXUS_RELEASE_DEADLINE_MS`: empty means the
 * default, a positive integer is honoured, anything else is refused by name rather
 * than silently becoming a bound that never arms or one that fires at once.
 *
 * Each case runs under `supervise-child.mjs` and ends by asserting its marker
 * stopped changing. Who reaps the writer differs by case: the armed cases kill the
 * fixture itself with SIGTERM, and the supervisor's group kill then takes the
 * writer (`supervise-child.mjs` reaps only on a signalled command); the `999` case
 * exits 4 normally, where the supervisor forwards the code and the fixture's own
 * `writer.kill("SIGKILL")` is what stops it. Do not delete that explicit kill.
 */
test("the fixture's write bound parses like the deadline env", async () => {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-fixture-bound-"));
  /** @param {string | undefined} value */
  const runFixture = async (value) => {
    const marker = join(dir, `marker-${String(value)}.txt`);
    const env = { ...process.env, MARKER: marker };
    if (value === undefined) {
      delete env.NEXUS_FIXTURE_MIN_WRITES;
    } else {
      env.NEXUS_FIXTURE_MIN_WRITES = value;
    }
    const result = await new Promise((done) => {
      const supervisor = spawn(
        process.execPath,
        [SUPERVISOR_PATH, process.execPath, "-e", SUICIDE_SPAWNER],
        { detached: true, stdio: ["ignore", "ignore", "pipe"], env },
      );
      let stderr = "";
      supervisor.stderr.on("data", (chunk) => {
        stderr += chunk;
      });
      supervisor.on("close", (code, signal) => done({ code, signal, stderr }));
    });
    return { ...result, marker };
  };
  try {
    for (const value of [undefined, "", "  ", "2"]) {
      const result = await runFixture(value);
      assert.equal(
        result.signal,
        "SIGTERM",
        `NEXUS_FIXTURE_MIN_WRITES=${JSON.stringify(value)} arms and kills itself`,
      );
      await assertNothingWrites(result.marker);
    }

    const unreachable = await runFixture("999");
    assert.equal(unreachable.code, 4);
    assert.match(unreachable.stderr, /never reached 999 writes/);
    await assertNothingWrites(unreachable.marker);

    for (const value of ["x", "0", "-1"]) {
      const invalid = await runFixture(value);
      assert.equal(
        invalid.code,
        5,
        `NEXUS_FIXTURE_MIN_WRITES=${value} must be refused`,
      );
      assert.match(
        invalid.stderr,
        /NEXUS_FIXTURE_MIN_WRITES must be a positive integer/,
      );
      assert.equal(
        existsSync(invalid.marker),
        false,
        "a refused bound never starts the writer",
      );
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

/** Child that only reports whether {@link run} accepted the deadline. */
const DEADLINE_PROBE = `
import { run } from ${JSON.stringify(CHECKER_URL)};
const result = run(process.execPath, ["-e", ""], process.cwd());
process.stdout.write(JSON.stringify({ ok: result.ok }));
`;

test("a malformed or expired deadline is refused by name", async () => {
  for (const raw of ["abc", "0", String(Date.now() - 1000)]) {
    const failure = await execFileAsync(
      process.execPath,
      ["--input-type=module", "-e", DEADLINE_PROBE],
      {
        encoding: "utf8",
        env: { ...process.env, NEXUS_RELEASE_DEADLINE_MS: raw },
      },
    ).then(
      () => null,
      (error) => error,
    );
    assert.ok(failure !== null, `NEXUS_RELEASE_DEADLINE_MS=${raw} must fail`);
    assert.match(
      failure.stderr,
      new RegExp(
        `release tooling deadline \\(NEXUS_RELEASE_DEADLINE_MS\\) must be a future epoch-millisecond timestamp, got "${raw}"`,
      ),
    );
  }

  const { stdout } = await execFileAsync(
    process.execPath,
    ["--input-type=module", "-e", DEADLINE_PROBE],
    {
      encoding: "utf8",
      env: { ...process.env, NEXUS_RELEASE_DEADLINE_MS: String(Date.now() + 60000) },
    },
  );
  assert.deepEqual(JSON.parse(stdout), { ok: true });
});

/**
 * Guards the CI step's wall clock, not the checker's behaviour: the step stops
 * this file at `timeout-minutes`, and a job killed there says nothing about
 * which case grew. Failing on the file's own duration instead names the number
 * minutes earlier, and the per-test durations above name the case. The budget
 * is measured — the runs behind it are recorded next to the CI step — so it only
 * fires on a regression, never on an ordinary slow runner. A case that runs past
 * it is killed first: every child is spawned against
 * `NEXUS_RELEASE_DEADLINE_MS`, set above from this budget.
 */
test("the file stays inside the CI step's measured budget", () => {
  const seconds = (Date.now() - FILE_STARTED) / 1000;
  console.log(`check-contract-packageability suite: ${seconds.toFixed(1)}s`);
  assert.ok(
    seconds <= FILE_BUDGET_SECONDS,
    `this file took ${seconds.toFixed(1)}s, over its ${FILE_BUDGET_SECONDS}s budget; the per-test durations above name the case that grew`,
  );
});
