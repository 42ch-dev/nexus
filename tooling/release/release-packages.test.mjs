import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { dirname, posix } from "node:path";
import {
  PACKAGE_RELEASE_LABEL,
  planPackageRelease,
  preparePackageVersions,
  publishSelected,
  readSourceProvenance,
  releaseTag,
} from "./release-packages.mjs";
import { readVersionBrackets } from "./version-surfaces.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  initGitRepo,
  readRepoFile,
  runReleaseScript,
  writeRepoFile,
} from "./test-harness.mjs";

/** Files a package release must never write, whatever it prepares. */
const PRODUCT_FILES = [
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "apps/desktop-electron/package.json",
  "apps/desktop-electron/resources/product.json",
  "apps/nexus-service/package.json",
  "packages/nexus-native/package.json",
  "tooling/release/version-brackets.json",
];

/** A prerequisite that already exists at the registry for every fixture. */
const CONTRACTS_PUBLISHED = { name: "nexus-contracts", version: "9.8.7", checksum: "a".repeat(64) };

const CHANGELOG = "# Changelog\n\n## [Unreleased]\n\n### Features\n- **test**: fixture entry\n";

/**
 * Temp repo with the release surfaces, a changelog per inventory package and a
 * commit, so provenance is the fixture's own HEAD.
 *
 * @param {{ version?: string; files?: Record<string, string> }} [options]
 * @returns {{ dir: string; sha: string }}
 */
function fixture(options = {}) {
  const dir = createTempRepo(options);
  for (const entry of readVersionBrackets(dir)) {
    writeRepoFile(dir, posix.join(posix.dirname(entry.manifest), "CHANGELOG.md"), CHANGELOG);
  }
  for (const [path, contents] of Object.entries(options.files ?? {})) {
    writeRepoFile(dir, path, contents);
  }
  return { dir, sha: initGitRepo(dir) };
}

/**
 * Verifier evidence for the packages named, all verified at `sha`.
 *
 * @param {string} sha
 * @param {{ name: string; version: string; checksum?: string }[]} rows
 */
function evidence(sha, rows) {
  return {
    sourceSha: sha,
    results: rows.map((row) => ({
      name: row.name,
      ok: true,
      archive: { sha256: row.checksum ?? "a".repeat(64) },
      package: { version: row.version },
    })),
  };
}

/**
 * Isolated publication protocol fixture: the only `publishSelected` client any
 * test uses. It records uploads, can refuse one name, and deliberately has no
 * withdraw/yank operation.
 */
function fixtureRegistry({ published = [], refuse } = {}) {
  const rows = [...published];
  const uploads = [];
  return {
    rows,
    uploads,
    async publish(entry) {
      if (entry.name === refuse) {
        throw new Error("registry refused the upload");
      }
      rows.push({ name: entry.name, version: entry.version, checksum: entry.archiveSha256 });
      uploads.push(`${entry.name}@${entry.version}`);
    },
    async read(name, version) {
      return rows.find((row) => row.name === name && row.version === version) ?? null;
    },
  };
}

/**
 * @param {ReturnType<typeof planPackageRelease>} plan
 * @returns {string[]}
 */
function refusalCodes(plan) {
  return plan.refusals.map((refusal) => refusal.code);
}

test("package tags and labels stay out of the product v* namespace", () => {
  assert.equal(releaseTag("nexus-pattern", "0.1.0"), "crate/nexus-pattern/v0.1.0");
  assert.ok(!releaseTag("nexus-pattern", "0.1.0").startsWith("v"));
  assert.notEqual(PACKAGE_RELEASE_LABEL, "release");
});

test("dependency order precedes consumers and ignores the caller's order", () => {
  const { dir, sha } = fixture();
  try {
    const plan = planPackageRelease(dir, {
      packages: [
        "nexus-narrative",
        "nexus-module-test",
        "nexus-pattern",
        "nexus-knowledge",
        "nexus-workflow-ir",
        "nexus-contracts",
        "nexus-module-manifest",
        "nexus-module-sdk",
      ],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.deepEqual(refusalCodes(plan), []);
    const index = (name) => plan.order.indexOf(name);
    assert.equal(plan.order.length, 8);
    assert.ok(index("nexus-contracts") < index("nexus-workflow-ir"));
    assert.ok(index("nexus-contracts") < index("nexus-knowledge"));
    assert.ok(index("nexus-workflow-ir") < index("nexus-pattern"));
    assert.ok(index("nexus-knowledge") < index("nexus-narrative"));
    assert.ok(index("nexus-module-sdk") < index("nexus-module-test"));
    // module-manifest has no inventory dependency; it is bound to no consumer.
    assert.ok(index("nexus-module-manifest") >= 0);
    for (const row of plan.packages) {
      assert.equal(row.tag, `crate/${row.name}/v${row.version}`);
      assert.equal(row.version, "9.8.7");
    }
  } finally {
    cleanupTempRepo(dir);
  }
});

test("unknown selection, wrong registry and malformed source refuse", () => {
  const { dir, sha } = fixture();
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-core", "not-a-package"],
      sourceSha: "c3ed",
      registry: "crates.io",
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan).sort(), [
      "invalid-source",
      "unclean-source",
      "unknown-registry",
      "unknown-selection",
      "unknown-selection",
    ]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a prerequisite that is neither selected nor published refuses", () => {
  const { dir, sha } = fixture();
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-pattern"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [{ name: "nexus-contracts", version: "9.8.7", checksum: "b".repeat(64) }],
      },
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan), ["unmet-prerequisite"]);
    assert.match(plan.refusals[0].message, /nexus-workflow-ir/);
    assert.ok(plan.packages[0].prerequisites.some((row) => row.resolved === "missing"));

    // An unknown registry view may not claim either way: the prerequisite
    // stays unverified instead of inventing absence.
    const unknown = planPackageRelease(dir, {
      packages: ["nexus-pattern"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(unknown.ok, true);
    assert.deepEqual(unknown.pending, ["nexus-pattern@9.8.7"]);
    assert.ok(unknown.packages[0].prerequisites.every((row) => row.resolved !== "missing"));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a selected version older than the registry refuses as out of order", () => {
  const { dir, sha } = fixture();
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [CONTRACTS_PUBLISHED, { name: "nexus-knowledge", version: "9.9.0", checksum: "c".repeat(64) }],
      },
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan), ["stale-version"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("unclean selected inputs and a source mismatch refuse", () => {
  const { dir, sha } = fixture();
  try {
    writeRepoFile(dir, "crates/nexus-knowledge/CHANGELOG.md", `${CHANGELOG}\n- **test**: dirty\n`);
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan), ["unclean-provenance"]);
    assert.match(plan.refusals[0].message, /crates\/nexus-knowledge\/CHANGELOG\.md/);

    const committed = initGitRepo(dir);
    const mismatched = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(mismatched.ok, false);
    assert.deepEqual(refusalCodes(mismatched), ["unclean-source"]);
    assert.equal(readSourceProvenance(dir, ["crates/nexus-knowledge/Cargo.toml"]).sha, committed);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("archive evidence from another source or version refuses", () => {
  const { dir, sha } = fixture();
  try {
    const otherSource = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
      evidence: evidence("0123456789abcdef0123456789abcdef01234567", [
        { name: "nexus-knowledge", version: "9.8.7" },
      ]),
    });
    assert.deepEqual(refusalCodes(otherSource), ["source-mismatch"]);

    const otherVersion = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
      evidence: evidence(sha, [{ name: "nexus-knowledge", version: "9.8.6" }]),
    });
    assert.deepEqual(refusalCodes(otherVersion), ["version-mismatch"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("content match skips, mismatched or unmatched content refuses", () => {
  const { dir, sha } = fixture();
  try {
    const checksum = "d".repeat(64);
    const accepted = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [CONTRACTS_PUBLISHED, { name: "nexus-knowledge", version: "9.8.7", checksum }],
        writable: true,
      },
      evidence: evidence(sha, [{ name: "nexus-knowledge", version: "9.8.7", checksum }]),
    });
    assert.deepEqual(refusalCodes(accepted), []);
    assert.deepEqual(accepted.completed, ["nexus-knowledge@9.8.7"]);
    assert.deepEqual(accepted.pending, []);

    const mismatched = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [CONTRACTS_PUBLISHED, { name: "nexus-knowledge", version: "9.8.7", checksum }],
      },
      evidence: evidence(sha, [{ name: "nexus-knowledge", version: "9.8.7", checksum: "e".repeat(64) }]),
    });
    assert.equal(mismatched.ok, false);
    assert.deepEqual(refusalCodes(mismatched), ["content-mismatch"]);

    const unverifiable = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [CONTRACTS_PUBLISHED, { name: "nexus-knowledge", version: "9.8.7", checksum }],
      },
    });
    assert.equal(unverifiable.ok, false);
    assert.deepEqual(refusalCodes(unverifiable), ["content-unverified"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a read-only registry refuses pending uploads as a permission failure", () => {
  const { dir, sha } = fixture();
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: { name: "crates-io", published: [CONTRACTS_PUBLISHED], writable: false },
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan), ["permissions"]);
    assert.equal(plan.credentials, "unconfigured");
  } finally {
    cleanupTempRepo(dir);
  }
});

test("publication stops on the first failure and resume skips matched content", async () => {
  const { dir, sha } = fixture();
  try {
    const rows = [
      { name: "nexus-contracts", version: "9.8.7" },
      { name: "nexus-knowledge", version: "9.8.7" },
      { name: "nexus-narrative", version: "9.8.7" },
    ];
    const prepublished = [CONTRACTS_PUBLISHED];
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge", "nexus-narrative"],
      sourceSha: sha,
      registry: { name: "crates-io", published: prepublished, writable: true },
      evidence: evidence(sha, rows),
    });
    assert.deepEqual(refusalCodes(plan), []);
    assert.deepEqual(plan.pending, ["nexus-knowledge@9.8.7", "nexus-narrative@9.8.7"]);

    const client = fixtureRegistry({ published: prepublished, refuse: "nexus-narrative" });
    assert.equal("yank" in client, false);
    const partial = await publishSelected(plan, client);
    assert.equal(partial.ok, false);
    assert.deepEqual(partial.completed, ["nexus-knowledge@9.8.7"]);
    assert.deepEqual(partial.pending, ["nexus-narrative@9.8.7"]);
    assert.equal(partial.failed, "nexus-narrative@9.8.7");
    assert.equal(partial.refusals[0].code, "publish-failed");
    assert.deepEqual(client.uploads, ["nexus-knowledge@9.8.7"]);

    const resumed = planPackageRelease(dir, {
      packages: ["nexus-knowledge", "nexus-narrative"],
      sourceSha: sha,
      registry: { name: "crates-io", published: client.rows, writable: true },
      evidence: evidence(sha, rows),
    });
    assert.deepEqual(resumed.completed, ["nexus-knowledge@9.8.7"]);
    assert.deepEqual(resumed.pending, ["nexus-narrative@9.8.7"]);
    client.rows.splice(client.rows.findIndex((row) => row.name === "nexus-narrative"), 1);
    const retried = fixtureRegistry({ published: client.rows });
    const finished = await publishSelected(resumed, retried);
    assert.equal(finished.ok, true);
    assert.deepEqual(finished.completed, ["nexus-knowledge@9.8.7", "nexus-narrative@9.8.7"]);
    assert.deepEqual(retried.uploads, ["nexus-narrative@9.8.7"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("publication refuses a plan that did not pass and an entry without an archive", async () => {
  const { dir, sha } = fixture();
  try {
    const refused = planPackageRelease(dir, {
      packages: ["nexus-core"],
      sourceSha: sha,
      registry: "crates-io",
    });
    const blocked = await publishSelected(refused, fixtureRegistry());
    assert.equal(blocked.ok, false);
    assert.equal(blocked.refusals[0].code, "unknown-selection");

    const noArchive = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    const client = fixtureRegistry();
    const missing = await publishSelected(noArchive, client);
    assert.equal(missing.ok, false);
    assert.equal(missing.refusals[0].code, "missing-archive");
    assert.deepEqual(client.uploads, []);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("preparation writes only selected literals, dependency bounds and changelogs", () => {
  // Pre-1.0: `^0.4.2` excludes 0.5.0, so the dependent bound must follow.
  const { dir } = fixture({ version: "0.4.2" });
  try {
    const productBefore = PRODUCT_FILES.map((path) => readRepoFile(dir, path));
    const narrativeBefore = readRepoFile(dir, "crates/nexus-narrative/Cargo.toml");
    const contractsBefore = readRepoFile(dir, "crates/nexus-contracts/Cargo.toml");
    const result = preparePackageVersions(dir, {
      packages: ["nexus-workflow-ir", "nexus-pattern"],
      versions: { "nexus-workflow-ir": "0.5.0", "nexus-pattern": "0.4.3" },
      date: "2026-01-02",
    });
    assert.equal(result.ok, true, JSON.stringify(result.refusals));
    assert.deepEqual([...result.changed].sort(), [
      "crates/nexus-pattern/CHANGELOG.md",
      "crates/nexus-pattern/Cargo.toml",
      "crates/nexus-workflow-ir/CHANGELOG.md",
      "crates/nexus-workflow-ir/Cargo.toml",
    ]);

    const pattern = readRepoFile(dir, "crates/nexus-pattern/Cargo.toml");
    assert.match(pattern, /^version = "0\.4\.3"$/m);
    assert.match(pattern, /nexus-workflow-ir = \{ version = "0\.5\.0", path = "\.\.\/nexus-workflow-ir" \}/);
    assert.match(pattern, /nexus-contracts = \{ version = "0\.4\.2", path = "\.\.\/nexus-contracts" \}/);
    assert.match(
      readRepoFile(dir, "crates/nexus-workflow-ir/Cargo.toml"),
      /^version = "0\.5\.0"$/m,
    );
    assert.equal(readRepoFile(dir, "crates/nexus-narrative/Cargo.toml"), narrativeBefore);
    assert.equal(readRepoFile(dir, "crates/nexus-contracts/Cargo.toml"), contractsBefore);

    const changelog = readRepoFile(dir, "crates/nexus-pattern/CHANGELOG.md");
    assert.match(changelog, /## \[Unreleased\]\n\n## \[0\.4\.3\] - 2026-01-02\n\n### Features/);
    assert.equal(readRepoFile(dir, "crates/nexus-narrative/CHANGELOG.md"), CHANGELOG);

    PRODUCT_FILES.forEach((path, index) => {
      assert.equal(readRepoFile(dir, path), productBefore[index], `${path} must not change`);
    });
  } finally {
    cleanupTempRepo(dir);
  }
});

test("preparation refuses non-increasing versions and a package without a changelog", () => {
  const { dir } = fixture({ version: "0.4.2" });
  try {
    const notGreater = preparePackageVersions(dir, {
      packages: ["nexus-knowledge"],
      versions: { "nexus-knowledge": "0.4.2" },
    });
    assert.equal(notGreater.ok, false);
    assert.deepEqual(notGreater.refusals.map((row) => row.code), ["version-not-greater"]);
    assert.deepEqual(notGreater.changed, []);

    const unknown = preparePackageVersions(dir, {
      packages: ["nexus-core"],
      versions: { "nexus-core": "0.5.0" },
    });
    assert.deepEqual(unknown.refusals.map((row) => row.code), ["unknown-selection"]);

    const missing = preparePackageVersions(dir, {
      packages: ["nexus-knowledge"],
      versions: { "nexus-knowledge": "0.5.0" },
    });
    assert.equal(missing.ok, true);
    assert.match(readRepoFile(dir, "crates/nexus-knowledge/CHANGELOG.md"), /## \[0\.5\.0\]/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("check-only CLI plans offline from the reviewed source", () => {
  const { dir, sha } = fixture();
  try {
    const run = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-pattern,nexus-knowledge,nexus-narrative",
        "--registry",
        "crates-io",
      ],
      dir,
      { env: { CARGO_REGISTRY_TOKEN: "" } },
    );
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, /archive evidence absent/);
    assert.match(run.stdout, /order {5}nexus-pattern, nexus-knowledge, nexus-narrative/);
    assert.match(run.stdout, /pending {3}nexus-pattern@9\.8\.7, nexus-knowledge@9\.8\.7, nexus-narrative@9\.8\.7/);
    assert.match(run.stdout, /credentials unconfigured/);

    const unknown = runReleaseScript(
      "release-packages.mjs",
      ["--check-only", "--source", sha, "--packages", "nexus-core", "--registry", "crates-io"],
      dir,
    );
    assert.equal(unknown.status, 1);
    assert.match(unknown.stderr, /refused: unknown-selection/);

    const usage = runReleaseScript(
      "release-packages.mjs",
      ["--check-only", "--packages", "nexus-knowledge", "--registry", "crates-io"],
      dir,
    );
    assert.equal(usage.status, 2);
    assert.match(usage.stderr, /--source is required/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("prepare CLI applies the selection and leaves product files alone", () => {
  const { dir } = fixture({ version: "0.4.2" });
  try {
    const productBefore = PRODUCT_FILES.map((path) => readRepoFile(dir, path));
    const run = runReleaseScript(
      "release-packages.mjs",
      ["--prepare", "--packages", "nexus-module-sdk", "--versions", "nexus-module-sdk=0.4.3"],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, /modules\/nexus-module-sdk\/Cargo\.toml/);
    assert.equal(readRepoFile(dir, "modules/nexus-module-sdk/CHANGELOG.md").includes("## [0.4.3] - "), true);
    PRODUCT_FILES.forEach((path, index) => {
      assert.equal(readRepoFile(dir, path), productBefore[index], `${path} must not change`);
    });
  } finally {
    cleanupTempRepo(dir);
  }
});

test("the helper holds no registry client, credential or network path", () => {
  const source = readFileSync(join(dirname(new URL(import.meta.url).pathname), "release-packages.mjs"), "utf8");
  for (const token of [
    "node:http",
    "node:https",
    "node:net",
    "fetch(",
    "CARGO_REGISTRY_TOKEN",
    "cargo",
    "git push",
  ]) {
    assert.equal(source.includes(token), false, `release-packages.mjs must not contain "${token}"`);
  }
});
