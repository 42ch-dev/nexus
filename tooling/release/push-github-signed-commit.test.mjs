import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildCreateCommitInput,
  collectFileChanges,
  composeCommitMessage,
  parseArgs,
} from "./push-github-signed-commit.mjs";
import {
  cleanupTempRepo,
  createTempRepo,
  gitOk,
  initGitRepo,
  readRepoFile,
  removeRepoFile,
  writeRepoFile,
} from "./test-harness.mjs";

test("parseArgs requires a branch, a message and a repo", () => {
  const parsed = parseArgs(
    ["--branch", "release/0.2.0", "--message", "chore(release): bump version to 0.2.0"],
    { GITHUB_REPOSITORY: "42ch/nexus" },
  );
  assert.equal(parsed.branch, "release/0.2.0");
  assert.equal(parsed.headline, "chore(release): bump version to 0.2.0");
  assert.equal(parsed.baseRef, "main");
  assert.equal(parsed.repo, "42ch/nexus");
  assert.deepEqual(parsed.trailers, []);

  assert.throws(() => parseArgs(["--branch", "b"], {}), /Required: --branch and --message/);
  assert.throws(
    () => parseArgs(["--branch", "b", "--message", "m", "--nope"], { GITHUB_REPOSITORY: "42ch/nexus" }),
    /Unknown argument: --nope/,
  );
  assert.throws(
    () =>
      parseArgs(["--branch", "b", "--message", "m", "--repo", "nexus"], {
        GITHUB_REPOSITORY: "42ch/nexus",
      }),
    /Set --repo owner\/name or GITHUB_REPOSITORY/,
  );
});

test("parseArgs collects repeated --trailer flags and honours --base-ref", () => {
  const parsed = parseArgs(
    [
      "--branch",
      "b",
      "--message",
      "m",
      "--repo",
      "42ch/nexus",
      "--base-ref",
      "develop",
      "--trailer",
      "Nexus-Prerelease: true",
      "--trailer",
      "Co-authored-by: bot <bot@example.com>",
    ],
    {},
  );
  assert.deepEqual(parsed.trailers, [
    "Nexus-Prerelease: true",
    "Co-authored-by: bot <bot@example.com>",
  ]);
  assert.equal(parsed.baseRef, "develop");
  assert.throws(
    () => parseArgs(["--branch", "b", "--message", "m", "--repo", "42ch/nexus", "--trailer"], {}),
    /--trailer requires a value/,
  );
});

test("parseArgs reports --help without requiring the other flags", () => {
  assert.equal(parseArgs(["--help"], {}).help, true);
});

test("composeCommitMessage appends the trailer block and truncates long headlines", () => {
  assert.deepEqual(composeCommitMessage({ headline: "chore(release): bump version to 0.2.0" }), {
    headline: "chore(release): bump version to 0.2.0",
  });
  assert.deepEqual(
    composeCommitMessage({
      headline: "chore(release): bump version to 0.2.0",
      trailers: ["Nexus-Prerelease: true"],
    }),
    {
      headline: "chore(release): bump version to 0.2.0",
      body: "Nexus-Prerelease: true",
    },
  );
  const long = composeCommitMessage({ headline: "x".repeat(300) });
  assert.equal(long.headline.length, 256);
  assert.ok(long.headline.endsWith("..."));
});

test("buildCreateCommitInput maps the GraphQL createCommitOnBranch payload", () => {
  const fileChanges = { additions: [{ path: "package.json", contents: "e30=" }], deletions: [] };
  const input = buildCreateCommitInput({
    repo: "42ch/nexus",
    branch: "release/0.2.0",
    baseOid: "a".repeat(40),
    message: { headline: "h", body: "Nexus-Prerelease: true" },
    fileChanges,
  });
  assert.deepEqual(input, {
    branch: { repositoryNameWithOwner: "42ch/nexus", branchName: "release/0.2.0" },
    message: { headline: "h", body: "Nexus-Prerelease: true" },
    fileChanges,
    expectedHeadOid: "a".repeat(40),
  });
});

test("collectFileChanges reports additions and deletions against origin/<base>", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    writeRepoFile(dir, "package.json", readRepoFile(dir, "package.json").replace('"0.1.0"', '"0.2.0"'));
    writeRepoFile(dir, "docs/release.md", "notes\n");
    removeRepoFile(dir, "Cargo.lock");

    const { additions, deletions } = collectFileChanges(dir, "main");
    assert.deepEqual(
      deletions.map((entry) => entry.path),
      ["Cargo.lock"],
    );
    assert.deepEqual(
      additions.map((entry) => entry.path).sort(),
      ["docs/release.md", "package.json"],
    );
    const packageJson = additions.find((entry) => entry.path === "package.json");
    assert.equal(
      Buffer.from(packageJson.contents, "base64").toString("utf8"),
      readRepoFile(dir, "package.json"),
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("collectFileChanges reports nothing when the tree matches the base", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    assert.deepEqual(collectFileChanges(dir, "main"), { additions: [], deletions: [] });
  } finally {
    cleanupTempRepo(dir);
  }
});

test("collectFileChanges handles renames as delete plus add", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    gitOk(dir, ["mv", "package.json", "package-renamed.json"]);
    const { additions, deletions } = collectFileChanges(dir, "main");
    assert.deepEqual(
      deletions.map((entry) => entry.path),
      ["package.json"],
    );
    assert.deepEqual(
      additions.map((entry) => entry.path),
      ["package-renamed.json"],
    );
  } finally {
    cleanupTempRepo(dir);
  }
});
