import assert from "node:assert/strict";
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  symlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { test } from "node:test";
import { VERSION_BRACKETS_PATH } from "./version-surfaces.mjs";
import {
  checkContractPackageability,
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

/** Repository-root-relative discovery path the checker reads. */
const CONSUMER_FIXTURES_PATH = "tooling/release/fixtures/package-consumers";
const CONSUMER_FIXTURES_DIR = join(REPO_ROOT, CONSUMER_FIXTURES_PATH);
/** Checked-in accounting every fixture repo must carry. */
const CONSUMER_ACCOUNTING_PATH = "tooling/release/fixtures/package-consumers.json";

/**
 * Accounting document for a fixture repo: every carrier entry, either with the
 * fixture that consumes it or explicitly without one and why.
 *
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
  // Declared fixture-less: the entry has no consumer and the accounting says so.
  files[CONSUMER_ACCOUNTING_PATH] = consumerAccounting({
    "nexus-tiny-contract": null,
  });
  const dir = createTempRepo({ files });
  // Staging derives its path list from Git, as it does in a real checkout.
  initGitRepo(dir);
  return dir;
}

/**
 * Temp repo whose carrier names only a standalone fixture subject copied from
 * `<caseDir>/subject`, with `<caseDir>/consumer` installed under the discovery
 * path the checker reads for that entry.
 *
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
    // Staging derives its path list from Git, as it does in a real checkout.
    initGitRepo(dir);
  }
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
    // No `initGitRepo`: a source tree Git cannot list must fail closed instead
    // of falling back to a verbatim, ignore-blind copy.
    assert.throws(
      () => checkContractPackageability(dir),
      /staging failed: git ls-files failed/,
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

test("staging leaves gitignored working files out of the packed archive", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public", {
    initGit: false,
  });
  try {
    // A build script the archive must not carry. If the gitignored file leaked
    // into staging, Cargo would pack it and the consumer build would run it.
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

    const report = checkContractPackageability(dir);
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

test("a consumer-supplied Cargo config is refused before any resolution", () => {
  const dir = consumerFixtureRepo(
    "consumer-supplies-cargo-config",
    "nexus-fixture-config",
  );
  try {
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    assert.equal(report.firstFailure, "nexus-fixture-config");

    const [row] = report.results;
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(
      row.consumer.cause,
      /consumer fixture supplies its own Cargo config/,
    );
    // The guard fires before any Cargo invocation: nothing was declared,
    // patched or resolved, and Cargo produced no output.
    assert.deepEqual(row.consumer.declared, []);
    assert.deepEqual(row.consumer.patched, []);
    assert.deepEqual(row.consumer.resolved, []);
    assert.equal(row.consumer.output, "");
    assert.equal(row.cause, row.consumer.cause);
  } finally {
    cleanupTempRepo(dir);
  }
});

/**
 * Contract crates the real consumer fixtures resolve. Every one is a member of
 * the temp workspace so workspace inheritance and `[workspace.dependencies]`
 * stay the product's own; only the entries a case names in its carrier pack.
 */
const CONSUMER_CLOSURE = [
  "nexus-contracts",
  "nexus-workflow-ir",
  "nexus-pattern",
  "nexus-knowledge",
  "nexus-narrative",
];

/**
 * Temp repo carrying the real contract crates, sources included, with one
 * consumer fixture installed under the discovery path the checker reads. The
 * workspace root is the real root manifest with its member list narrowed to
 * the closure, so the fixture inherits the product dependency table instead of
 * duplicating it; versions stay the real literals, so the fixture's caret
 * requirements mean what a registry consumer would mean.
 *
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
  // The product lock keeps every registry version resolvable offline.
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
 * The report row for one packed entry.
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
 * Temp repo carrying standalone `modules/*` contract crates, sources included,
 * with one consumer fixture installed under the discovery path the checker
 * reads. Standalone crates cannot be workspace members, so the root is an empty
 * virtual workspace and the carriers are the only entries that pack.
 *
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
      // A local build dir is not source: the real checkout leaves it untracked.
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
    const report = checkContractPackageability(dir);
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
    // The compiled program asserts every field; its own line is the proof it
    // ran to the end of its checks.
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
    const report = checkContractPackageability(dir);
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
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-pattern");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-contracts", "nexus-pattern", "nexus-workflow-ir"],
    );
    // The registry root the consumer scans is written from the archive's own
    // packed fixture members, so this line is archive evidence: all three
    // shipped directories load and all three templates match their IR hashes.
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
    // Every shipped member is embedded through `nexus_pattern::fixtures`, so
    // dropping one from the tracked source must break the consumer build rather
    // than leave a smaller archive that still passes. `verdict.md` is the member
    // the package's own tests never read, which is exactly the gap I-000533
    // named.
    rmSync(
      join(
        dir,
        "crates/nexus-pattern/tests/fixtures/patterns/step-with-judge/templates/verdict.md",
      ),
    );
    assert.equal(git(dir, ["add", "-A"]).status, 0);
    assert.equal(git(dir, ["commit", "-qm", "drop packed template"]).status, 0);

    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    // This fixture carries one consumer, so failing it also withdraws the
    // coverage proof of the archives it consumes; the failing consumer keeps
    // its own named cause below.
    assert.notEqual(report.firstFailure, null);

    const row = rowFor(report, "nexus-pattern");
    assert.equal(row.ok, false);
    assert.equal(row.consumer.ok, false);
    assert.match(row.consumer.cause, /consumer run failed/);
    // The failure must come from the extracted archive and name the omitted file.
    assert.match(
      row.consumer.output,
      /archives\/nexus-pattern\/nexus-pattern-[\d.]+\/src\/\.\.\/tests\/fixtures\/patterns\/step-with-judge\/templates\/verdict\.md/,
      row.consumer.output,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the module-sdk consumer builds a wasm32 guest through nexus_entry and the host API separately", () => {
  const dir = moduleConsumerRepo("nexus-module-sdk", ["nexus-module-sdk"]);
  try {
    const report = checkContractPackageability(dir);
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

    // The guest is real wasm32 output built from the same extracted archive.
    assert.match(row.consumer.guest.artifact, /\.wasm$/);
    assert.ok(row.consumer.guest.bytes > 8, String(row.consumer.guest.bytes));
    assert.match(row.consumer.guest.sha256, /^[0-9a-f]{64}$/);

    // And the guest declares the SDK only: no host-side crate can ride along.
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
    const report = checkContractPackageability(dir);
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
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));

    const row = rowFor(report, "nexus-module-test");
    assert.equal(row.ok, true, row.cause ?? "");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.deepEqual(
      row.consumer.patched.map((patched) => patched.name),
      ["nexus-module-sdk", "nexus-module-test"],
    );
    // The mini-host is a dev-dependency, so the fixture must ship a test target
    // and the checker must have run it with `cargo test`.
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
    const report = checkContractPackageability(dir);
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

    // Both packages are positively consumed from their own archives, and the
    // knowledge package needs no fixture of its own to earn that evidence.
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
    // Knowledge has no runner of its own: its coverage names the fixture whose
    // resolved graph pulled the archive in.
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
    // A link may point at a path that is not materialized in this checkout; it
    // is still one tree entry with its own bytes, and staging must copy it.
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

    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    // The whole point: an unchanged checkout of a revision that contains a
    // relative link must verify as that revision, or no plan can use the
    // receipt (`unclean-evidence`).
    assert.equal(report.source.clean, true, JSON.stringify(report.source));
    assert.equal(report.source.mismatch, undefined);
    const sha = report.source.sha;

    // Staging preserves link bytes; the comparison still catches a retarget,
    // so this is not a check that stopped checking.
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
    // A second entry that packs but that no consumer fixture names: its own
    // fixture is absent and nothing else consumes it.
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
    // The second entry is declared fixture-less; nothing consumes it.
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({
        "nexus-fixture-public": "nexus-fixture-public",
        "nexus-tiny-contract": null,
      }),
    );

    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    const uncovered = rowFor(report, "nexus-tiny-contract");
    assert.equal(uncovered.ok, false);
    assert.equal(
      uncovered.cause,
      "pack-only: no passing consumer consumes nexus-tiny-contract",
    );
    // The covered entry keeps its own consumer proof; coverage is a per-archive
    // requirement, not a global switch.
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

    // The plain check still reports the same tree and exits 0: the assertion is
    // opt-in for the step that produces evidence.
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
  // The repository tracks binary assets through an LFS clean filter, so a
  // stored blob is a pointer rather than the checkout's bytes. Comparing the
  // staged file raw reported every clean checkout as unclean; the comparison
  // must run Git's own conversion over the staged copy.
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
    // An optional dependency no feature enables: Cargo still lists it as a
    // declared dependency (so the checker supplies the patch), but it never
    // enters the resolved graph, and the fixture itself still builds and runs.
    // `patched` therefore says the archive was offered, not that anything used
    // it.
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

    const report = checkContractPackageability(dir);
    assert.equal(report.ok, false);
    const row = rowFor(report, "nexus-tiny-contract");
    assert.equal(row.ok, false);
    // The fixture ran and the archive was offered to it — the row still fails,
    // because nothing resolved that archive.
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
    // Nothing resolved the archive, so no fixture is named as its proof.
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
      () => checkContractPackageability(dir),
      /consumer accounting failed: tooling\/release\/fixtures\/package-consumers\.json is missing/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("removing a declared fixture fails until the accounting is edited", () => {
  const dir = consumerFixtureRepo("positive-api-call", "nexus-fixture-public");
  try {
    // The fixture directory is deleted and the accounting still declares it:
    // "fixtures were removed" must not read as "no fixtures by design".
    rmSync(join(dir, CONSUMER_FIXTURES_PATH, "nexus-fixture-public"), {
      recursive: true,
      force: true,
    });
    assert.throws(
      () => checkContractPackageability(dir),
      /consumer accounting failed: .*nexus-fixture-public declares the fixture nexus-fixture-public, which is not in the tree/,
    );

    // The deliberate edit — declare the entry fixture-less, with a reason —
    // makes the same tree a legitimate pack-only run.
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      consumerAccounting({ "nexus-fixture-public": null }),
    );
    const report = checkContractPackageability(dir);
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
  // A renamed fixture — directory and accounting record changed together — is
  // an accepted deliberate act. The record names the directory that runs, so
  // the evidence cannot describe a fixture the run never staged, and a declared
  // runner cannot be skipped while the row stays green through another
  // fixture's coverage.
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
    const report = checkContractPackageability(dir);
    assert.equal(report.ok, true, JSON.stringify(report.results, null, 2));
    const row = rowFor(report, "nexus-fixture-public");
    assert.equal(row.consumer.ok, true, row.consumer.cause ?? "");
    assert.equal(row.consumer.case, `${CONSUMER_FIXTURES_PATH}/fixture-renamed`);
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
      () => checkContractPackageability(dir),
      /records nexus-other-contract, which is not a carrier entry/,
    );
    writeRepoFile(dir, CONSUMER_ACCOUNTING_PATH, "{}\n");
    assert.throws(
      () => checkContractPackageability(dir),
      /nexus-tiny-contract is not recorded/,
    );
    writeRepoFile(
      dir,
      CONSUMER_ACCOUNTING_PATH,
      `${JSON.stringify({ "nexus-tiny-contract": { fixture: null } })}\n`,
    );
    assert.throws(
      () => checkContractPackageability(dir),
      /nexus-tiny-contract has no fixture, so it needs a reason/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});
