import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildChangelogSection,
  collectCommits,
  groupCommits,
  isReleaseBumpSubject,
  parseConventionalSubject,
  prependSection,
  resolveChangelogBase,
} from "./generate-changelog.mjs";
import {
  cleanupTempRepo,
  commitFile,
  createTempRepo,
  gitOk,
  initGitRepo,
  readRepoFile,
  runReleaseScript,
  tagRelease,
  writeRepoFile,
} from "./test-harness.mjs";

const sha = (char) => char.repeat(40);

/** @type {{ sha: string; subject: string }[]} */
const COMMITS = [
  { sha: sha("a"), subject: "feat(desktop): add native smoke" },
  { sha: sha("b"), subject: "fix: repair build" },
  { sha: sha("c"), subject: "docs: document release flow" },
  { sha: sha("d"), subject: "chore: tidy scripts" },
  { sha: sha("e"), subject: "chore(release): bump version to 0.2.0" },
  { sha: sha("f"), subject: "v1.201 — run observation (#352)" },
  { sha: sha("1"), subject: "feat!: drop legacy flag" },
];

test("parseConventionalSubject parses type, scope and breaking marker", () => {
  assert.deepEqual(parseConventionalSubject("feat(desktop)!: add x"), {
    type: "feat",
    scope: "desktop",
    breaking: true,
    description: "add x",
  });
  assert.deepEqual(parseConventionalSubject("fix: repair"), {
    type: "fix",
    scope: null,
    breaking: false,
    description: "repair",
  });
  assert.equal(parseConventionalSubject("v1.201 — run observation (#352)"), null);
  assert.equal(parseConventionalSubject("Merge branch 'main'"), null);
});

test("isReleaseBumpSubject excludes only chore(release) subjects", () => {
  assert.equal(isReleaseBumpSubject("chore(release): bump version to 0.2.0"), true);
  assert.equal(isReleaseBumpSubject("chore(release)!: bump version to 1.0.0"), true);
  assert.equal(isReleaseBumpSubject("chore(release-notes): keep"), false);
  assert.equal(isReleaseBumpSubject("chore: tidy"), false);
});

test("groupCommits buckets conventional commits and skips the rest", () => {
  const { groups, included, skipped } = groupCommits(COMMITS);
  assert.equal(included, 5);
  assert.deepEqual(skipped, [
    "chore(release): bump version to 0.2.0",
    "v1.201 — run observation (#352)",
  ]);
  assert.deepEqual(
    groups.map((group) => [group.heading, group.entries.length]),
    [
      ["Features", 2],
      ["Fixes", 1],
      ["Docs & chores", 2],
    ],
  );
});

test("buildChangelogSection renders the pinned heading and group order", () => {
  const built = buildChangelogSection({
    version: "0.2.0",
    date: "2026-09-30",
    commits: COMMITS,
  });
  assert.equal(
    built.section,
    [
      "## [0.2.0] - 2026-09-30",
      "",
      "### Features",
      "- **desktop**: add native smoke (aaaaaaa)",
      "- drop legacy flag (1111111)",
      "",
      "### Fixes",
      "- repair build (bbbbbbb)",
      "",
      "### Docs & chores",
      "- document release flow (ccccccc)",
      "- tidy scripts (ddddddd)",
    ].join("\n"),
  );
  assert.deepEqual(built.headings, ["Features", "Fixes", "Docs & chores"]);
});

test("buildChangelogSection inserts the human summary and omits empty groups", () => {
  const built = buildChangelogSection({
    version: "0.2.0",
    date: "2026-09-30",
    summary: "  First governed release.\n",
    commits: [{ sha: sha("b"), subject: "fix: repair build" }],
  });
  assert.equal(
    built.section,
    ["## [0.2.0] - 2026-09-30", "", "First governed release.", "", "### Fixes", "- repair build (bbbbbbb)"].join(
      "\n",
    ),
  );
  assert.deepEqual(built.headings, ["Fixes"]);
});

test("prependSection keeps the existing header and puts the new section first", () => {
  const existing = "# Changelog\n\nAll notable changes.\n\n## [Unreleased]\n\nnothing yet\n";
  const updated = prependSection(existing, "## [0.2.0] - 2026-09-30\n\n### Fixes\n- x (bbbbbbb)");
  assert.equal(
    updated,
    [
      "# Changelog",
      "",
      "All notable changes.",
      "",
      "## [0.2.0] - 2026-09-30",
      "",
      "### Fixes",
      "- x (bbbbbbb)",
      "",
      "## [Unreleased]",
      "",
      "nothing yet",
      "",
    ].join("\n"),
  );
});

test("resolveChangelogBase prefers the newest v* tag, then the CHANGELOG baseline", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const baseline = commitFile(dir, "CHANGELOG.md", "# Changelog\n", "docs: add changelog");
    tagRelease(dir, "v0.1.0", baseline);
    tagRelease(dir, "v0.0.9", baseline);
    assert.deepEqual(resolveChangelogBase(dir), { tag: "v0.1.0", source: "tag" });

    gitOk(dir, ["tag", "-d", "v0.1.0", "v0.0.9"]);
    assert.deepEqual(resolveChangelogBase(dir), {
      commit: baseline,
      source: "changelog-baseline",
    });
  } finally {
    cleanupTempRepo(dir);
  }
});

test("resolveChangelogBase returns null without tags or a CHANGELOG baseline", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    assert.equal(resolveChangelogBase(dir), null);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("collectCommits ranges over the requested refs", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const baseline = commitFile(dir, "CHANGELOG.md", "# Changelog\n", "docs: add changelog");
    commitFile(dir, "notes-a.md", "a\n", "feat: alpha");
    commitFile(dir, "notes-b.md", "b\n", "fix: beta");
    const commits = collectCommits(dir, baseline, "HEAD");
    assert.deepEqual(
      commits.map((commit) => commit.subject),
      ["fix: beta", "feat: alpha"],
    );
    assert.match(commits[0].sha, /^[0-9a-f]{40}$/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI uses the CHANGELOG baseline for the first release", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    commitFile(dir, "CHANGELOG.md", "# Changelog\n", "docs: bootstrap changelog");
    commitFile(dir, "notes-a.md", "a\n", "feat(desktop): first feature");
    commitFile(dir, "notes-b.md", "b\n", "fix: first fix");

    const run = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2.0", "--date", "2026-09-30"],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stderr, /first-release fallback/);
    assert.ok(run.stdout.startsWith("## [0.2.0] - 2026-09-30\n"));
    assert.match(run.stdout, /### Features\n- \*\*desktop\*\*: first feature \([0-9a-f]{7}\)\n/);
    assert.match(run.stdout, /### Fixes\n- first fix \([0-9a-f]{7}\)\n$/);
    assert.ok(!run.stdout.includes("bootstrap changelog"), run.stdout);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI ranges from the newest tag and prepends into CHANGELOG.md", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const baseline = commitFile(
      dir,
      "CHANGELOG.md",
      "# Changelog\n\nAll notable changes.\n\n## [Unreleased]\n",
      "docs: bootstrap changelog",
    );
    tagRelease(dir, "v0.1.0", baseline);
    commitFile(dir, "notes-a.md", "a\n", "feat: alpha");
    commitFile(dir, "notes-b.md", "b\n", "chore(release): bump version to 0.2.0");

    const run = runReleaseScript(
      "generate-changelog.mjs",
      [
        "--version",
        "0.2.0",
        "--date",
        "2026-09-30",
        "--summary",
        "First governed release.",
        "--prepend",
        "CHANGELOG.md",
      ],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stderr, /changelog base: v0\.1\.0 \(newest v\* tag\)/);

    const changelog = readRepoFile(dir, "CHANGELOG.md");
    assert.ok(changelog.startsWith("# Changelog\n\nAll notable changes.\n\n## [0.2.0] - 2026-09-30"));
    assert.ok(changelog.includes("First governed release."));
    assert.match(changelog, /### Features\n- alpha \([0-9a-f]{7}\)\n/);
    assert.ok(changelog.includes("## [Unreleased]"));
    assert.ok(changelog.indexOf("## [0.2.0]") < changelog.indexOf("## [Unreleased]"));
    assert.ok(!changelog.includes("bump version to 0.2.0"));
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI emits an empty-bodied section when the tag sits at HEAD", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    tagRelease(dir, "v0.1.0", "HEAD");
    const run = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2.0", "--date", "2026-09-30"],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout, "## [0.2.0] - 2026-09-30\n");
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI fails without a defensible base and on invalid arguments", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    const noBase = runReleaseScript("generate-changelog.mjs", ["--version", "0.2.0"], dir);
    assert.equal(noBase.status, 1);
    assert.match(noBase.stderr, /cannot determine the changelog base/);

    writeRepoFile(dir, "CHANGELOG.md", "# Changelog\n");
    commitFile(dir, "notes.md", "x\n", "feat: x");
    const badVersion = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2", "--date", "2026-09-30"],
      dir,
    );
    assert.equal(badVersion.status, 1);
    assert.match(badVersion.stderr, /Invalid or missing --version/);

    const badDate = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2.0", "--date", "30-09-2026"],
      dir,
    );
    assert.equal(badDate.status, 1);
    assert.match(badDate.stderr, /Invalid --date/);

    const missingSummary = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2.0", "--summary-file", "nope.md"],
      dir,
    );
    assert.equal(missingSummary.status, 1);

    const bothSummaries = runReleaseScript(
      "generate-changelog.mjs",
      ["--version", "0.2.0", "--summary", "a", "--summary-file", "b"],
      dir,
    );
    assert.equal(bothSummaries.status, 1);

    const help = runReleaseScript("generate-changelog.mjs", ["--help"], dir);
    assert.equal(help.status, 0);
    assert.match(help.stdout, /Usage: node tooling\/release\/generate-changelog\.mjs/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI --summary-file and --base override the defaults", () => {
  const dir = createTempRepo();
  try {
    initGitRepo(dir);
    commitFile(dir, "notes-a.md", "a\n", "feat: alpha");
    const tip = commitFile(dir, "notes-b.md", "b\n", "fix: beta");
    writeRepoFile(dir, "summary.md", "Written by the release manager.\n");

    const run = runReleaseScript(
      "generate-changelog.mjs",
      [
        "--version",
        "0.2.0",
        "--date",
        "2026-09-30",
        "--base",
        `${tip}~1`,
        "--summary-file",
        "summary.md",
      ],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.match(run.stdout, /Written by the release manager\./);
    assert.match(run.stdout, /### Fixes\n- beta \(/);
    assert.ok(!run.stdout.includes("alpha"));
  } finally {
    cleanupTempRepo(dir);
  }
});
