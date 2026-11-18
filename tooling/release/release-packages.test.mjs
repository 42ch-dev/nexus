import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, posix } from "node:path";
import { test } from "node:test";
import {
  PACKAGE_RELEASE_LABEL,
  planPackageRelease,
  preparePackageVersions,
  publishSelected,
  readSourceProvenance,
  releaseTag,
} from "./release-packages.mjs";
import { verifyStagedSource } from "./check-contract-packageability.mjs";
import { readVersionBrackets } from "./version-surfaces.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  gitOk,
  initGitRepo,
  mutateRepoFile,
  readRepoFile,
  removeRepoFile,
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
 * A preload that denies the network at the JS module surface, so a CLI run can
 * be observed rather than string-scanned.
 */
const DENY_NETWORK_PRELOAD = `
import net from "node:net";
import tls from "node:tls";
import http from "node:http";
import https from "node:https";
import dns from "node:dns";
const denied = (name) => () => {
  throw new Error("network denied: " + name);
};
net.connect = net.createConnection = denied("net.connect");
tls.connect = denied("tls.connect");
http.request = http.get = denied("http.request");
https.request = https.get = denied("https.request");
dns.lookup = dns.resolve = denied("dns");
globalThis.fetch = async () => {
  throw new Error("network denied: fetch");
};
`;

/**
 * Temp repo with the release surfaces, a changelog per inventory package, the
 * caller's mutations, then one commit — so provenance is the fixture's own HEAD
 * and the tree is clean.
 *
 * @param {{ version?: string; files?: Record<string, string>; mutate?: (dir: string) => void }} [options]
 * @returns {{ dir: string; sha: string }}
 */
function fixture(options = {}) {
  const dir = createTempRepo({ version: options.version, files: options.files });
  for (const entry of readVersionBrackets(dir)) {
    writeRepoFile(dir, posix.join(posix.dirname(entry.manifest), "CHANGELOG.md"), CHANGELOG);
  }
  options.mutate?.(dir);
  return { dir, sha: initGitRepo(dir) };
}

/**
 * Packageability evidence in the shape `check-contract-packageability.mjs
 * --json` prints.
 *
 * @param {string} sha
 * @param {{ name: string; version: string; checksum?: string }[]} rows
 * @param {{ clean?: boolean }} [source]
 */
function evidence(sha, rows, source = {}) {
  return {
    ok: true,
    source: { sha, clean: source.clean ?? true },
    results: rows.map((row) => ({
      name: row.name,
      ok: true,
      archive: { sha256: row.checksum ?? "a".repeat(64) },
      package: { version: row.version },
    })),
    firstFailure: null,
  };
}

/**
 * Isolated publication protocol fixture: the only `publishSelected` client any
 * test uses. It records uploads, can refuse an upload or a confirmation read,
 * and deliberately has no withdraw/yank operation.
 */
function fixtureRegistry({ published = [], refuse, readFailure } = {}) {
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
      if (readFailure?.name === name) {
        if (readFailure.mode === "throw") {
          throw new Error("registry read timed out");
        }
        return { checksum: "f".repeat(64) };
      }
      return rows.find((row) => row.name === name && row.version === version) ?? null;
    },
  };
}

/**
 * Run one Node script directly, for the denial negative control.
 *
 * @param {string} scriptPath
 * @param {string} cwd
 * @param {Record<string, string>} env
 */
function spawnNode(scriptPath, cwd, env) {
  const result = spawnSync(process.execPath, [scriptPath], {
    cwd,
    env: { ...process.env, ...env },
    encoding: "utf8",
  });
  return {
    status: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

/**
 * @param {ReturnType<typeof planPackageRelease>} plan
 * @returns {string[]}
 */
function refusalCodes(plan) {
  return plan.refusals.map((refusal) => refusal.code);
}

/**
 * @param {ReturnType<typeof planPackageRelease>} plan
 * @returns {string[]}
 */
function blockerCodes(plan) {
  return plan.blockers.map((blocker) => blocker.code);
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
    assert.ok(index("nexus-module-manifest") >= 0);
    for (const row of plan.packages) {
      assert.equal(row.tag, `crate/${row.name}/v${row.version}`);
      assert.equal(row.version, "9.8.7");
    }
    // A valid plan is not a publishable one: no registry view and no archive
    // evidence leave every row blocked, and the reason says so.
    assert.equal(plan.ok, true);
    assert.equal(plan.ready, false);
    assert.ok(blockerCodes(plan).includes("archive-unverified"));
    assert.match(plan.packages[0].reason, /registry state unknown/);
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
    assert.equal(plan.ready, false);
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

test("an incompatible selected prerequisite refuses instead of passing", () => {
  const { dir, sha } = fixture({
    mutate: (root) =>
      mutateRepoFile(root, "crates/nexus-narrative/Cargo.toml", (contents) =>
        contents.replace('nexus-knowledge = { version = "9.8.7"', 'nexus-knowledge = { version = "9.9.0"'),
      ),
  });
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge", "nexus-narrative"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(plan.ok, false);
    assert.equal(plan.ready, false);
    assert.ok(refusalCodes(plan).includes("incompatible-prerequisite"));
    assert.match(
      plan.refusals.find((refusal) => refusal.code === "incompatible-prerequisite").message,
      /requires nexus-knowledge 9\.9\.0, but the selected nexus-knowledge is 9\.8\.7/,
    );
    const narrative = plan.packages.find((row) => row.name === "nexus-narrative");
    assert.equal(
      narrative.prerequisites.find((row) => row.name === "nexus-knowledge").resolved,
      "incompatible",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an unverified or absent prerequisite blocks publication", async () => {
  const { dir, sha } = fixture();
  try {
    const onlyPattern = planPackageRelease(dir, {
      packages: ["nexus-pattern"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(onlyPattern.ok, true);
    assert.equal(onlyPattern.ready, false);
    assert.ok(blockerCodes(onlyPattern).includes("prerequisite-unverified"));
    assert.ok(
      onlyPattern.packages[0].prerequisites.every((row) => row.resolved === "unverified"),
    );
    const blocked = await publishSelected(onlyPattern, fixtureRegistry());
    assert.equal(blocked.ok, false);
    assert.equal(blocked.refusals[0].code, "not-ready");
    assert.deepEqual(blocked.pending, ["nexus-pattern@9.8.7"]);

    const known = planPackageRelease(dir, {
      packages: ["nexus-pattern"],
      sourceSha: sha,
      registry: {
        name: "crates-io",
        published: [{ name: "nexus-contracts", version: "9.8.7", checksum: "b".repeat(64) }],
      },
    });
    assert.equal(known.ok, false);
    assert.deepEqual(refusalCodes(known), ["unmet-prerequisite"]);
    assert.match(known.refusals[0].message, /nexus-workflow-ir/);
    assert.ok(known.packages[0].prerequisites.some((row) => row.resolved === "missing"));
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

test("any uncommitted source, an unreadable checkout and a wrong source refuse", () => {
  const { dir, sha } = fixture();
  try {
    // A crate outside the selection still feeds the packed archives.
    mutateRepoFile(dir, "crates/nexus-core/src/lib.rs", (contents) => `${contents}\n// local edit\n`);
    const dirty = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.equal(dirty.ok, false);
    assert.deepEqual(refusalCodes(dirty), ["unclean-provenance"]);
    assert.match(dirty.refusals[0].message, /crates\/nexus-core\/src\/lib\.rs/);

    const committed = initGitRepo(dir);
    const wrongSource = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    assert.deepEqual(refusalCodes(wrongSource), ["unclean-source"]);
    assert.equal(readSourceProvenance(dir).sha, committed);
    assert.deepEqual(readSourceProvenance(dir).dirty, []);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a checkout whose status cannot be read refuses instead of failing open", () => {
  const dir = createTempRepo();
  try {
    const plan = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: "0".repeat(40),
      registry: "crates-io",
    });
    assert.equal(plan.ok, false);
    assert.deepEqual(refusalCodes(plan), ["source-unreadable"]);
    assert.match(plan.refusals[0].message, /cannot read HEAD/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("archive evidence must come from the reviewed clean revision", () => {
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

    const unclean = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
      evidence: evidence(sha, [{ name: "nexus-knowledge", version: "9.8.7" }], { clean: false }),
    });
    assert.deepEqual(refusalCodes(unclean), ["unclean-evidence"]);

    const unreadable = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
      evidence: { ok: true, source: { sha, clean: true } },
    });
    assert.deepEqual(refusalCodes(unreadable), ["evidence-unreadable"]);

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
    assert.equal(accepted.ready, true);

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
      evidence: evidence(sha, [{ name: "nexus-knowledge", version: "9.8.7" }]),
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
    assert.equal(plan.ready, true);
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
    assert.equal(resumed.ready, true);
    const retried = fixtureRegistry({ published: client.rows });
    const finished = await publishSelected(resumed, retried);
    assert.equal(finished.ok, true);
    assert.deepEqual(finished.completed, ["nexus-knowledge@9.8.7", "nexus-narrative@9.8.7"]);
    assert.deepEqual(retried.uploads, ["nexus-narrative@9.8.7"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an unconfirmed upload stays pending and keeps the completed report", async () => {
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
    assert.equal(plan.ready, true);

    // The confirmation read throws after the upload landed.
    const throwing = fixtureRegistry({ published: prepublished, readFailure: { name: "nexus-knowledge", mode: "throw" } });
    const thrown = await publishSelected(plan, throwing);
    assert.equal(thrown.ok, false);
    assert.deepEqual(thrown.completed, []);
    assert.deepEqual(thrown.pending, ["nexus-knowledge@9.8.7", "nexus-narrative@9.8.7"]);
    assert.equal(thrown.failed, "nexus-knowledge@9.8.7");
    assert.equal(thrown.refusals[0].code, "upload-unconfirmed");
    assert.match(thrown.refusals[0].message, /confirmation read failed/);

    // The read answers, but not with the reviewed content.
    const mismatched = fixtureRegistry({ published: prepublished, readFailure: { name: "nexus-narrative", mode: "mismatch" } });
    const wrong = await publishSelected(plan, mismatched);
    assert.equal(wrong.ok, false);
    assert.deepEqual(wrong.completed, ["nexus-knowledge@9.8.7"]);
    assert.deepEqual(wrong.pending, ["nexus-narrative@9.8.7"]);
    assert.equal(wrong.failed, "nexus-narrative@9.8.7");
    assert.equal(wrong.refusals[0].code, "upload-unconfirmed");
    assert.match(wrong.refusals[0].message, /expected a{64}/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("publication refuses a refused plan and a plan that is not ready", async () => {
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

    const notReady = planPackageRelease(dir, {
      packages: ["nexus-knowledge"],
      sourceSha: sha,
      registry: "crates-io",
    });
    const client = fixtureRegistry();
    const unready = await publishSelected(notReady, client);
    assert.equal(unready.ok, false);
    assert.equal(unready.refusals[0].code, "not-ready");
    assert.deepEqual(client.uploads, []);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("an unknown registry view cannot make a pending selection publishable", async () => {
  const { dir, sha } = fixture();
  try {
    // `nexus-module-manifest` has no prerequisite, so only the registry view
    // stands between verified evidence and a publishable plan.
    const evidenceRows = [{ name: "nexus-module-manifest", version: "9.8.7" }];
    const unknown = planPackageRelease(dir, {
      packages: ["nexus-module-manifest"],
      sourceSha: sha,
      registry: "crates-io",
      evidence: evidence(sha, evidenceRows),
    });
    assert.deepEqual(refusalCodes(unknown), []);
    assert.equal(unknown.ready, false);
    assert.deepEqual(blockerCodes(unknown), ["registry-unverified"]);
    const client = fixtureRegistry();
    const refused = await publishSelected(unknown, client);
    assert.equal(refused.ok, false);
    assert.equal(refused.refusals[0].code, "not-ready");
    assert.deepEqual(client.uploads, []);

    // A known, empty view for the same selection is publishable.
    const known = planPackageRelease(dir, {
      packages: ["nexus-module-manifest"],
      sourceSha: sha,
      registry: { name: "crates-io", published: [], writable: true },
      evidence: evidence(sha, evidenceRows),
    });
    assert.deepEqual(blockerCodes(known), []);
    assert.equal(known.ready, true);
    assert.deepEqual(known.pending, ["nexus-module-manifest@9.8.7"]);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("a staged copy is verified against the revision in both directions", () => {
  const { dir, sha } = fixture();
  const stage = mkdtempSync(join(tmpdir(), "nexus-stage-"));
  try {
    cpSync(dir, stage, {
      recursive: true,
      filter: (source) => !source.split("/").includes(".git"),
    });
    assert.deepEqual(verifyStagedSource(dir, sha, stage), []);

    // Content drift after the status sample.
    mutateRepoFile(stage, "crates/nexus-knowledge/src/lib.rs", (contents) => `${contents}\n// late edit\n`);
    const drifted = verifyStagedSource(dir, sha, stage);
    assert.ok(
      drifted.includes("crates/nexus-knowledge/src/lib.rs"),
      drifted.slice(0, 5).join(","),
    );

    // A file added after the revision: the field is where the old
    // commit-blob-only walk went blind, because the addition is not in the
    // commit at all.
    writeRepoFile(stage, "crates/nexus-knowledge/src/added.rs", "pub fn added() {}\n");
    const added = verifyStagedSource(dir, sha, stage);
    assert.ok(
      added.includes(`crates/nexus-knowledge/src/added.rs (staged, absent from ${sha})`),
      added.slice(0, 5).join(","),
    );

    // A revision path the copy lacks is a mismatch, never a silent skip.
    removeRepoFile(stage, "crates/nexus-knowledge/Cargo.toml");
    const missing = verifyStagedSource(dir, sha, stage);
    assert.ok(
      missing.some((path) => path.startsWith("crates/nexus-knowledge/Cargo.toml (in ")),
      missing.slice(0, 5).join(","),
    );
  } finally {
    rmSync(stage, { recursive: true, force: true });
    cleanupTempRepo(dir);
  }
});

test("a gitlink is one tree entry, not a staged extra", () => {
  // An initialized submodule: the revision records a single commit entry, while
  // the worktree contains its files, which staging copies.
  const upstream = mkdtempSync(join(tmpdir(), "nexus-submodule-"));
  const { dir } = fixture();
  const stage = mkdtempSync(join(tmpdir(), "nexus-stage-gitlink-"));
  try {
    mkdirSync(join(upstream, "skill"), { recursive: true });
    writeFileSync(join(upstream, "skill/SKILL.md"), "# probe\n", "utf8");
    gitOk(upstream, ["init", "-q", "-b", "main"]);
    gitOk(upstream, ["add", "-A"]);
    gitOk(upstream, ["-c", "user.email=fixture@example.com", "-c", "user.name=Fixture", "commit", "-qm", "init"]);
    gitOk(dir, [
      "-c",
      "protocol.file.allow=always",
      "submodule",
      "add",
      "--quiet",
      upstream,
      ".agents/skills/probe",
    ]);
    gitOk(dir, ["add", "-A"]);
    gitOk(dir, ["-c", "user.email=fixture@example.com", "-c", "user.name=Fixture", "commit", "-qm", "add submodule"]);
    const sha = gitOk(dir, ["rev-parse", "HEAD"]);
    const gitlinkRows = spawnSync("git", ["ls-tree", "-r", sha], { cwd: dir, encoding: "utf8" })
      .stdout.split("\n")
      .filter((line) => line.startsWith("160000"));
    assert.equal(gitlinkRows.length, 1, gitlinkRows.join("|"));

    cpSync(dir, stage, {
      recursive: true,
      filter: (source) => !source.split("/").includes(".git"),
    });
    // The stage really carries the submodule contents the walk must ignore.
    assert.equal(readFileSync(join(stage, ".agents/skills/probe/skill/SKILL.md"), "utf8"), "# probe\n");
    assert.deepEqual(verifyStagedSource(dir, sha, stage), []);
  } finally {
    rmSync(stage, { recursive: true, force: true });
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

test("preparation rewrites every declaration of a bumped prerequisite", () => {
  // `nexus-knowledge` declares the contracts edge twice — a normal and a dev
  // dependency — so one non-global rewrite would leave the other bound at the
  // old minor while reporting success.
  const { dir } = fixture({ version: "0.4.2" });
  try {
    const before = readRepoFile(dir, "crates/nexus-knowledge/Cargo.toml");
    assert.equal(
      before.match(/nexus-contracts = \{ version = "0\.4\.2"/g)?.length,
      2,
      before,
    );
    const result = preparePackageVersions(dir, {
      packages: ["nexus-contracts"],
      versions: { "nexus-contracts": "0.5.0" },
    });
    assert.equal(result.ok, true, JSON.stringify(result.refusals));
    const after = readRepoFile(dir, "crates/nexus-knowledge/Cargo.toml");
    assert.equal(after.match(/nexus-contracts = \{ version = "0\.4\.2"/g), null);
    assert.equal(
      after.match(/nexus-contracts = \{ version = "0\.5\.0"/g)?.length,
      2,
      after,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("preparation refuses a non-increasing version and a missing changelog", () => {
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

    const before = readRepoFile(dir, "crates/nexus-knowledge/Cargo.toml");
    removeRepoFile(dir, "crates/nexus-knowledge/CHANGELOG.md");
    const missing = preparePackageVersions(dir, {
      packages: ["nexus-knowledge"],
      versions: { "nexus-knowledge": "0.5.0" },
    });
    assert.equal(missing.ok, false);
    assert.deepEqual(missing.refusals.map((row) => row.code), ["changelog-missing"]);
    assert.deepEqual(missing.changed, []);
    assert.equal(readRepoFile(dir, "crates/nexus-knowledge/Cargo.toml"), before);
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
    assert.equal(
      readRepoFile(dir, "modules/nexus-module-sdk/CHANGELOG.md").includes("## [0.4.3] - "),
      true,
    );
    PRODUCT_FILES.forEach((path, index) => {
      assert.equal(readRepoFile(dir, path), productBefore[index], `${path} must not change`);
    });
  } finally {
    cleanupTempRepo(dir);
  }
});

test("check-only CLI plans from the reviewed source and exits on readiness", () => {
  const { dir, sha } = fixture();
  const scratch = mkdtempSync(join(tmpdir(), "nexus-release-cli-"));
  try {
    const planned = runReleaseScript(
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
    assert.equal(planned.status, 0, planned.stderr);
    assert.match(planned.stdout, /plan {6}ok/);
    assert.match(planned.stdout, /archive evidence absent/);
    assert.match(planned.stdout, /order {5}nexus-pattern, nexus-knowledge, nexus-narrative/);
    assert.match(planned.stdout, /ready {5}no/);
    assert.match(planned.stdout, /prerequisite-unverified/);
    assert.match(planned.stdout, /registry-unverified/);
    assert.match(planned.stdout, /credentials unconfigured/);

    // The gate refuses the same plan: a green planning run is not a release.
    const gated = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--require-publishable",
        "--source",
        sha,
        "--packages",
        "nexus-pattern,nexus-knowledge,nexus-narrative",
        "--registry",
        "crates-io",
      ],
      dir,
    );
    assert.equal(gated.status, 1, gated.stdout);
    assert.match(gated.stdout, /ready {5}no/);

    // The operator route for exact-match resume: registry state plus the
    // checker's JSON report, both outside the packed tree.
    const statePath = join(scratch, "registry-state.json");
    writeFileSync(
      statePath,
      JSON.stringify({ published: [CONTRACTS_PUBLISHED], writable: true }),
      "utf8",
    );
    const evidencePath = join(scratch, "evidence.json");
    writeFileSync(
      evidencePath,
      JSON.stringify(
        evidence(sha, [
          { name: "nexus-knowledge", version: "9.8.7" },
          { name: "nexus-narrative", version: "9.8.7" },
        ]),
      ),
      "utf8",
    );
    const ready = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge,nexus-narrative",
        "--registry",
        "crates-io",
        "--registry-state",
        statePath,
        "--archive-evidence",
        evidencePath,
      ],
      dir,
    );
    assert.equal(ready.status, 0, ready.stderr);
    assert.match(ready.stdout, /ready {5}yes/);
    assert.match(ready.stdout, /pending {3}nexus-knowledge@9\.8\.7, nexus-narrative@9\.8\.7/);

    // The same inputs satisfy the publishability gate.
    const readyGated = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--require-publishable",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge,nexus-narrative",
        "--registry",
        "crates-io",
        "--registry-state",
        statePath,
        "--archive-evidence",
        evidencePath,
      ],
      dir,
    );
    assert.equal(readyGated.status, 0, readyGated.stderr);

    const prepareGated = runReleaseScript(
      "release-packages.mjs",
      ["--prepare", "--require-publishable", "--packages", "nexus-knowledge"],
      dir,
    );
    assert.equal(prepareGated.status, 2);
    assert.match(prepareGated.stderr, /--require-publishable belongs to --check-only/);

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

    const brokenEvidence = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge",
        "--registry",
        "crates-io",
        "--archive-evidence",
        join(scratch, "missing.json"),
      ],
      dir,
    );
    assert.equal(brokenEvidence.status, 2);
    assert.match(
      brokenEvidence.stderr,
      /refused: input-unusable: --archive-evidence .*missing\.json/,
    );

    // A well-formed JSON document of the wrong shape is a named refusal, never
    // a TypeError from whatever reads it next.
    const wrongShape = join(scratch, "wrong-shape.json");
    writeFileSync(wrongShape, JSON.stringify({ results: 5 }), "utf8");
    const badEvidence = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge",
        "--registry",
        "crates-io",
        "--archive-evidence",
        wrongShape,
      ],
      dir,
    );
    assert.equal(badEvidence.status, 2);
    assert.match(badEvidence.stderr, /refused: evidence-unreadable: --archive-evidence/);

    const badState = join(scratch, "bad-state.json");
    writeFileSync(badState, JSON.stringify({ published: 5, writable: true }), "utf8");
    const refusedState = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge",
        "--registry",
        "crates-io",
        "--registry-state",
        badState,
      ],
      dir,
    );
    assert.equal(refusedState.status, 2);
    assert.match(refusedState.stderr, /refused: registry-state-unusable: --registry-state/);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
    cleanupTempRepo(dir);
  }
});

test("the supported CLI paths run with the network denied, and the denial is real", () => {
  const { dir, sha } = fixture();
  const scratch = mkdtempSync(join(tmpdir(), "nexus-release-net-"));
  try {
    const preload = join(scratch, "deny-network.mjs");
    writeFileSync(preload, DENY_NETWORK_PRELOAD, "utf8");
    const env = { NODE_OPTIONS: `--import ${preload}` };

    // Negative control: without this, a passing denied run would prove nothing.
    const probe = join(scratch, "probe.mjs");
    writeFileSync(
      probe,
      'try {\n  await fetch("http://127.0.0.1:9/");\n  console.log("NETWORK-REACHABLE");\n} catch (error) {\n  console.log(`denied: ${error.message}`);\n  process.exit(1);\n}\n',
      "utf8",
    );
    const probeRun = spawnNode(probe, dir, env);
    assert.equal(probeRun.status, 1);
    assert.match(probeRun.stdout, /denied: network denied: fetch/);

    const statePath = join(scratch, "registry-state.json");
    writeFileSync(statePath, JSON.stringify({ published: [CONTRACTS_PUBLISHED], writable: true }), "utf8");
    const evidencePath = join(scratch, "evidence.json");
    writeFileSync(
      evidencePath,
      JSON.stringify(evidence(sha, [{ name: "nexus-knowledge", version: "9.8.7" }])),
      "utf8",
    );
    const planned = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge",
        "--registry",
        "crates-io",
        "--registry-state",
        statePath,
        "--archive-evidence",
        evidencePath,
      ],
      dir,
      { env },
    );
    assert.equal(planned.status, 0, planned.stderr);
    assert.match(planned.stdout, /ready {5}yes/);

    const withoutState = runReleaseScript(
      "release-packages.mjs",
      ["--check-only", "--source", sha, "--packages", "nexus-knowledge", "--registry", "crates-io"],
      dir,
      { env },
    );
    assert.equal(withoutState.status, 0, withoutState.stderr);
    assert.match(withoutState.stdout, /plan {6}ok/);
    assert.match(withoutState.stdout, /ready {5}no/);

    const gatedDenied = runReleaseScript(
      "release-packages.mjs",
      [
        "--check-only",
        "--require-publishable",
        "--source",
        sha,
        "--packages",
        "nexus-knowledge",
        "--registry",
        "crates-io",
      ],
      dir,
      { env },
    );
    assert.equal(gatedDenied.status, 1, gatedDenied.stdout);
    assert.match(gatedDenied.stdout, /ready {5}no/);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
    cleanupTempRepo(dir);
  }
});
