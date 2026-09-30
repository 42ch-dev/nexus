import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  buildCreateCommitInput,
  collectFileChanges,
  composeCommitMessage,
  main,
  parseArgs,
  verifyPinnedBase,
} from "./push-github-signed-commit.mjs";
import {
  cleanupTempRepo,
  commitFile,
  createTempRepo,
  FIXTURE_BASELINE_VERSION,
  FIXTURE_TARGET_VERSION,
  git,
  gitOk,
  initGitRepo,
  readRepoFile,
  removeRepoFile,
  runReleaseScript,
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

test("parseArgs accepts an optional --body-file and rejects a missing value", () => {
  const parsed = parseArgs(
    ["--branch", "b", "--message", "m", "--repo", "42ch/nexus", "--body-file", "summary.md"],
    {},
  );
  assert.equal(parsed.bodyFile, "summary.md");
  assert.equal(parsed.trailers.length, 0);
  assert.throws(
    () => parseArgs(["--branch", "b", "--message", "m", "--repo", "42ch/nexus", "--body-file"], {}),
    /--body-file requires a path/,
  );
});

test("parseArgs reports --help without requiring the other flags", () => {
  assert.equal(parseArgs(["--help"], {}).help, true);
});

test("parseArgs takes the expected base OID from --expected-base-oid or RELEASE_BASE_OID", () => {
  const oid = "a".repeat(40);
  assert.equal(parseArgs(["--branch", "b", "--message", "m", "--repo", "o/r"], {}).expectedBaseOid, null);
  assert.equal(
    parseArgs(["--branch", "b", "--message", "m", "--repo", "o/r"], { RELEASE_BASE_OID: oid })
      .expectedBaseOid,
    oid,
  );
  assert.equal(
    parseArgs(
      ["--branch", "b", "--message", "m", "--repo", "o/r", "--expected-base-oid", oid],
      { RELEASE_BASE_OID: "b".repeat(40) },
    ).expectedBaseOid,
    oid,
  );
  // A blank flag/env value must not disable the check with an empty string.
  assert.equal(
    parseArgs(
      ["--branch", "b", "--message", "m", "--repo", "o/r", "--expected-base-oid", "  "],
      { RELEASE_BASE_OID: "   " },
    ).expectedBaseOid,
    null,
  );
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
  assert.deepEqual(
    composeCommitMessage({
      headline: "chore(release): bump version to 0.2.0",
      body: "First release.\n\nHighlights here.",
      trailers: ["Nexus-Prerelease: true"],
    }),
    {
      headline: "chore(release): bump version to 0.2.0",
      body: "First release.\n\nHighlights here.\nNexus-Prerelease: true",
    },
  );
  assert.deepEqual(composeCommitMessage({ headline: "h", body: "Summary line" }), {
    headline: "h",
    body: "Summary line",
  });
  assert.deepEqual(
    composeCommitMessage({ headline: "h", body: "  \n ", trailers: ["Nexus-Prerelease: true"] }),
    { headline: "h", body: "Nexus-Prerelease: true" },
  );
  const long = composeCommitMessage({ headline: "x".repeat(300) });
  assert.equal(long.headline.length, 256);
  assert.ok(long.headline.endsWith("..."));
});

test("the release tag annotation reads the summary line, not the trailer", () => {
  // Mirrors `.github/workflows/release.yml`: the tag job annotates with the
  // first usable line of the bump commit body, read as
  // `git show -s --format=%B <commit> | awk 'NR == 1 { next } NF { print; exit }'`.
  // `%B` renders `headline\n\nbody`, so line 1 is the subject.
  const firstUsableBodyLine = (message) =>
    message
      .split("\n")
      .slice(1)
      .find((line) => line.trim().length > 0) ?? "";
  const rawMessage = (message) => `${message.headline}\n\n${message.body ?? ""}`;

  const withSummary = composeCommitMessage({
    headline: "chore(release): bump version to 0.2.0",
    body: "Nexus 0.2.0 opens the pre-baseline history.\n\nMore detail.",
    trailers: ["Nexus-Prerelease: true"],
  });
  assert.equal(
    firstUsableBodyLine(rawMessage(withSummary)),
    "Nexus 0.2.0 opens the pre-baseline history.",
  );

  const withoutSummary = composeCommitMessage({
    headline: "chore(release): bump version to 0.2.0",
    trailers: ["Nexus-Prerelease: true"],
  });
  assert.equal(firstUsableBodyLine(rawMessage(withoutSummary)), "Nexus-Prerelease: true");
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
    writeRepoFile(
      dir,
      "package.json",
      readRepoFile(dir, "package.json").replace(
        `"version": "${FIXTURE_BASELINE_VERSION}"`,
        `"version": "${FIXTURE_TARGET_VERSION}"`,
      ),
    );
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

test("verifyPinnedBase passes on a match, is skipped without an expected OID, and refuses a stale base", () => {
  const pinned = "a".repeat(40);
  assert.equal(verifyPinnedBase(pinned, pinned, "main"), null);
  assert.equal(verifyPinnedBase(null, pinned, "main"), null);
  assert.equal(verifyPinnedBase(undefined, pinned, "main"), null);

  assert.throws(() => verifyPinnedBase(pinned, "b".repeat(40), "main"), (error) => {
    assert.match(error.message, /Stale base/);
    assert.ok(error.message.includes(pinned));
    assert.ok(error.message.includes("b".repeat(40)));
    assert.match(error.message, /Re-dispatch/);
    return true;
  });
});

/**
 * Scratch repo whose `origin` is a local bare repo, with `main` advanced past
 * the commit the working tree was prepared from — the QC2-001 race shape.
 *
 * @returns {{ root: string; origin: string; work: string; preparedOid: string; advancedOid: string }}
 */
function createAdvancedOriginFixture() {
  const root = mkdtempSync(join(tmpdir(), "nexus-release-origin-"));
  const origin = join(root, "origin.git");
  const work = join(root, "work");
  gitOk(root, ["init", "-q", "--bare", "-b", "main", origin]);
  mkdirSync(work, { recursive: true });
  gitOk(work, ["init", "-q", "-b", "main"]);
  gitOk(work, ["config", "user.email", "release-test@example.com"]);
  gitOk(work, ["config", "user.name", "Release Test"]);
  gitOk(work, ["config", "commit.gpgsign", "false"]);
  writeRepoFile(work, "package.json", '{"name":"scratch","version":"9.8.7"}\n');
  gitOk(work, ["add", "-A"]);
  gitOk(work, ["commit", "-q", "-m", "init"]);
  gitOk(work, ["remote", "add", "origin", origin]);
  gitOk(work, ["push", "-q", "origin", "main"]);
  const preparedOid = gitOk(work, ["rev-parse", "HEAD"]);

  const advancedOid = commitFile(work, "main-only.txt", "main advanced\n", "main advances");
  gitOk(work, ["push", "-q", "origin", "main"]);
  return { root, origin, work, preparedOid, advancedOid };
}

test("the helper refuses a stale prepared base instead of reverting the newer main", () => {
  const { root, origin, work, preparedOid, advancedOid } = createAdvancedOriginFixture();
  try {
    // Faithful race: the bump was prepared on the old base, then main advanced.
    gitOk(work, ["reset", "-q", "--hard", preparedOid]);
    writeRepoFile(work, "package.json", '{"name":"scratch","version":"9.8.8"}\n');

    // What the unguarded helper would have committed: main's added file shows
    // up as a deletion against the newer origin/main.
    gitOk(work, ["fetch", "-q", "origin", "main"]);
    assert.deepEqual(
      collectFileChanges(work, "main").deletions.map((entry) => entry.path),
      ["main-only.txt"],
    );

    const result = runReleaseScript(
      "push-github-signed-commit.mjs",
      ["--branch", "release/0.2.0", "--message", "chore(release): bump version to 0.2.0", "--repo", "o/r"],
      work,
      { env: { RELEASE_BASE_OID: preparedOid, GITHUB_TOKEN: "", GH_TOKEN: "" } },
    );

    assert.equal(result.status, 1);
    assert.match(result.stderr, /Stale base/);
    assert.ok(result.stderr.includes(preparedOid));
    assert.ok(result.stderr.includes(advancedOid));
    assert.match(result.stderr, /Re-dispatch/);
    // No mutation attempted: the API boundary (token check) was never reached.
    assert.doesNotMatch(result.stderr, /GITHUB_TOKEN/);
    assert.doesNotMatch(result.stdout, /COMMIT_OID/);
    assert.equal(git(origin, ["show-ref", "--verify", "--quiet", "refs/heads/release/0.2.0"]).status, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the helper proceeds past the base pin when the fetched main still matches it", () => {
  const { root, work, advancedOid } = createAdvancedOriginFixture();
  try {
    const result = runReleaseScript(
      "push-github-signed-commit.mjs",
      ["--branch", "release/0.2.0", "--message", "chore(release): bump version to 0.2.0", "--repo", "o/r"],
      work,
      { env: { RELEASE_BASE_OID: advancedOid, GITHUB_TOKEN: "", GH_TOKEN: "" } },
    );

    // Reached the next guard, not the stale-base refusal.
    assert.equal(result.status, 1);
    assert.doesNotMatch(result.stderr, /Stale base/);
    assert.match(result.stderr, /No file changes vs origin\/main/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

/**
 * Scratch repo whose `origin` is a local bare repo, with `main` advanced past
 * the commit the previous release branch was published from and the working
 * tree prepared exactly like the workflow (fresh branch off `origin/main` plus
 * the version bump) — the QC3-002 re-dispatch shape.
 *
 * @param {{ existingRelease?: boolean }} [options]
 * @returns {{ root: string; origin: string; work: string; previousReleaseOid: string | null; advancedMainOid: string }}
 */
function createRedispatchFixture({ existingRelease = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "nexus-release-redispatch-"));
  const origin = join(root, "origin.git");
  const work = join(root, "work");
  gitOk(root, ["init", "-q", "--bare", "-b", "main", origin]);
  mkdirSync(work, { recursive: true });
  gitOk(work, ["init", "-q", "-b", "main"]);
  gitOk(work, ["config", "user.email", "release-test@example.com"]);
  gitOk(work, ["config", "user.name", "Release Test"]);
  gitOk(work, ["config", "commit.gpgsign", "false"]);
  writeRepoFile(work, "package.json", '{"name":"scratch","version":"9.8.7"}\n');
  gitOk(work, ["add", "-A"]);
  gitOk(work, ["commit", "-q", "-m", "init"]);
  gitOk(work, ["remote", "add", "origin", origin]);
  gitOk(work, ["push", "-q", "origin", "main"]);

  let previousReleaseOid = null;
  if (existingRelease) {
    // A previously published release head: the bump on the older `main`.
    gitOk(work, ["switch", "-q", "-c", "release/0.2.0"]);
    writeRepoFile(work, "package.json", '{"name":"scratch","version":"9.8.8"}\n');
    gitOk(work, ["add", "-A"]);
    gitOk(work, ["commit", "-q", "-m", "chore(release): bump version to 0.2.0"]);
    previousReleaseOid = gitOk(work, ["rev-parse", "HEAD"]);
    gitOk(work, ["push", "-q", "origin", "release/0.2.0"]);
    gitOk(work, ["switch", "-q", "main"]);
  }

  const advancedMainOid = commitFile(work, "main-only.txt", "main advanced\n", "main advances");
  gitOk(work, ["push", "-q", "origin", "main"]);

  gitOk(work, ["switch", "-q", "--force-create", "release/0.2.0", "origin/main"]);
  writeRepoFile(work, "package.json", '{"name":"scratch","version":"9.8.9"}\n');

  return { root, origin, work, previousReleaseOid, advancedMainOid };
}

/**
 * Minimal stand-in for the GitHub REST + GraphQL endpoints the helper calls.
 *
 * @param {{ refOid: string | null; commitOid?: string; commitError?: string }} options
 * @returns {Promise<{ url: string; requests: { method: string; url: string; body: any }[]; refMutations: () => { method: string; url: string; body: any }[]; close: () => Promise<void> }>}
 */
async function startGitHubStub({ refOid, commitOid = "c".repeat(40), commitError = null }) {
  /** @type {{ method: string; url: string; body: any }[]} */
  const requests = [];
  const server = createServer((req, res) => {
    let raw = "";
    req.on("data", (chunk) => {
      raw += chunk;
    });
    req.on("end", () => {
      requests.push({ method: req.method ?? "", url: req.url ?? "", body: raw ? JSON.parse(raw) : null });
      const send = (status, payload) => {
        res.writeHead(status, { "Content-Type": "application/json" });
        res.end(JSON.stringify(payload));
      };
      if (req.method === "GET" && req.url?.startsWith("/repos/o/r/git/ref/heads/")) {
        return refOid === null
          ? send(404, { message: "Not Found" })
          : send(200, { ref: "refs/heads/release/0.2.0", object: { sha: refOid } });
      }
      if (req.url === "/graphql") {
        return commitError
          ? send(200, { errors: [{ message: commitError }] })
          : send(200, {
              data: { createCommitOnBranch: { commit: { oid: commitOid, url: `https://example.test/${commitOid}` } } },
            });
      }
      if (req.method === "POST" && req.url === "/repos/o/r/git/refs") {
        return send(201, {});
      }
      if (req.method === "PATCH" && req.url?.startsWith("/repos/o/r/git/refs/heads/")) {
        return send(200, {});
      }
      return send(404, { message: `unexpected ${req.method} ${req.url}` });
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  const port = typeof address === "object" && address !== null ? address.port : 0;
  return {
    url: `http://127.0.0.1:${port}`,
    requests,
    // Any REST call that would move a ref; the commit mutation itself is
    // GraphQL and counted separately.
    refMutations: () => requests.filter((entry) => entry.method !== "GET" && entry.url.startsWith("/repos/o/r/git/")),
    close: () =>
      new Promise((resolve) => {
        server.close(() => resolve());
      }),
  };
}

/**
 * Run the helper in-process (the stub server shares this event loop) with the
 * GitHub environment pointed at it.
 *
 * @param {{ url: string }} stub
 * @param {string} work
 * @param {string[]} args
 * @param {Record<string, string>} [env]
 * @returns {Promise<number>}
 */
async function runHelper(stub, work, args, env = {}) {
  const applied = {
    NEXUS_REPO_ROOT: work,
    GITHUB_TOKEN: "stub-token",
    GH_TOKEN: "",
    GITHUB_API_URL: stub.url,
    ...env,
  };
  /** @type {Record<string, string | undefined>} */
  const saved = {};
  for (const [key, value] of Object.entries(applied)) {
    saved[key] = process.env[key];
    process.env[key] = value;
  }
  try {
    return await main(args);
  } finally {
    for (const [key, value] of Object.entries(saved)) {
      if (value === undefined) {
        delete process.env[key];
      } else {
        process.env[key] = value;
      }
    }
  }
}

/** The re-dispatch CLI invocation shared by the three publication tests. */
const REDISPATCH_ARGS = [
  "--branch",
  "release/0.2.0",
  "--message",
  "chore(release): bump version to 0.2.0",
  "--repo",
  "o/r",
];

test("the helper keeps an existing release head when the replacement commit fails", async () => {
  const fixture = createRedispatchFixture();
  const stub = await startGitHubStub({ refOid: fixture.previousReleaseOid, commitError: "commit denied" });
  try {
    await assert.rejects(
      runHelper(stub, fixture.work, REDISPATCH_ARGS, { RELEASE_BASE_OID: fixture.advancedMainOid }),
      /GraphQL errors/,
    );

    // The failing run never moved the published ref: no force-reset to main.
    assert.deepEqual(stub.refMutations(), []);
    assert.equal(
      gitOk(fixture.origin, ["rev-parse", "refs/heads/release/0.2.0"]),
      fixture.previousReleaseOid,
    );
  } finally {
    await stub.close();
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("the helper replaces an existing release head with a commit parented on it", async () => {
  const fixture = createRedispatchFixture();
  const stub = await startGitHubStub({ refOid: fixture.previousReleaseOid });
  try {
    assert.equal(
      await runHelper(stub, fixture.work, REDISPATCH_ARGS, { RELEASE_BASE_OID: fixture.advancedMainOid }),
      0,
    );

    const mutation = stub.requests.find((entry) => entry.url === "/graphql");
    assert.ok(mutation, "expected a createCommitOnBranch mutation");
    const input = mutation.body.variables.input;
    // Compare-and-swap on the existing head, not on main.
    assert.equal(input.expectedHeadOid, fixture.previousReleaseOid);
    assert.notEqual(input.expectedHeadOid, fixture.advancedMainOid);
    assert.deepEqual(stub.refMutations(), []);
    // Delta taken against the previous release head, so the resulting tree is
    // the prepared working tree (main's later changes included) and the PR
    // does not revert them.
    assert.deepEqual(
      input.fileChanges.additions.map((entry) => entry.path).sort(),
      ["main-only.txt", "package.json"],
    );
    assert.deepEqual(input.fileChanges.deletions, []);
  } finally {
    await stub.close();
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("the helper creates the release branch from the base when it is absent", async () => {
  const fixture = createRedispatchFixture({ existingRelease: false });
  const stub = await startGitHubStub({ refOid: null, commitOid: "d".repeat(40) });
  try {
    assert.equal(
      await runHelper(stub, fixture.work, REDISPATCH_ARGS, { RELEASE_BASE_OID: fixture.advancedMainOid }),
      0,
    );

    const created = stub.requests.find(
      (entry) => entry.method === "POST" && entry.url === "/repos/o/r/git/refs",
    );
    assert.deepEqual(created?.body, { ref: "refs/heads/release/0.2.0", sha: fixture.advancedMainOid });

    const input = stub.requests.find((entry) => entry.url === "/graphql").body.variables.input;
    assert.equal(input.expectedHeadOid, fixture.advancedMainOid);
    // Fresh path unchanged: the delta is still taken against the base ref.
    assert.deepEqual(
      input.fileChanges.additions.map((entry) => entry.path).sort(),
      ["package.json"],
    );
  } finally {
    await stub.close();
    rmSync(fixture.root, { recursive: true, force: true });
  }
});
