#!/usr/bin/env node
/**
 * Generate the CHANGELOG section (also the Release body source) for a release.
 *
 * Groups conventional commits over `<base>..<head>` into Features / Fixes /
 * Docs & chores. `chore(release):` bump commits are excluded — the release
 * notes never list the release machinery itself.
 *
 * Base ref resolution:
 *   1. the newest `v*` tag (`git tag --list 'v*' --sort=-v:refname`)
 *   2. first-release fallback (compass D9): the commit that introduced
 *      `CHANGELOG.md` (`git log --diff-filter=A -1 --format=%H -- CHANGELOG.md`),
 *      so the first governed release covers everything merged after the
 *      changelog mechanism landed
 *   3. otherwise the command fails — there is no defensible range
 *
 * CLI:
 *   node tooling/release/generate-changelog.mjs --version <X.Y.Z> [--base <ref>]
 *     [--head <ref>] [--summary <text> | --summary-file <path>]
 *     [--date YYYY-MM-DD] [--prepend <CHANGELOG.md>] [--repo-root <path>]
 *
 * Writes the section to stdout, or prepends it to the file named by
 * `--prepend`.
 *
 * @module tooling/release/generate-changelog
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveRepoRoot } from "./version-surfaces.mjs";
import { isCleanSemVer } from "./semver.mjs";
import { runGit } from "./assert-version-greater.mjs";

/** Changelog file bootstrapped by the release-flow work (baseline marker). */
export const CHANGELOG_PATH = "CHANGELOG.md";

/** Conventional-commit groups, in rendering order. */
export const GROUPS = [
  { heading: "Features", types: ["feat"] },
  { heading: "Fixes", types: ["fix"] },
  {
    heading: "Docs & chores",
    types: ["build", "chore", "ci", "docs", "perf", "refactor", "revert", "style", "test"],
  },
];

const CONVENTIONAL_PATTERN = /^([a-zA-Z]+)(?:\(([^()]*)\))?(!?):\s+(.+)$/;
const RELEASE_BUMP_PATTERN = /^chore\(release\)!?:/;

/**
 * @param {string} subject
 * @returns {{ type: string; scope: string | null; breaking: boolean; description: string } | null}
 */
export function parseConventionalSubject(subject) {
  const match = subject.match(CONVENTIONAL_PATTERN);
  if (!match) {
    return null;
  }
  return {
    type: match[1].toLowerCase(),
    scope: match[2] ?? null,
    breaking: match[3] === "!",
    description: match[4].trim(),
  };
}

/**
 * @param {string} subject
 * @returns {boolean}
 */
export function isReleaseBumpSubject(subject) {
  return RELEASE_BUMP_PATTERN.test(subject);
}

/**
 * @param {{ sha: string; subject: string }} commit
 * @returns {string}
 */
function renderEntry(commit) {
  const parsed = parseConventionalSubject(commit.subject);
  const description = parsed ? parsed.description : commit.subject;
  const scope = parsed?.scope ? `**${parsed.scope}**: ` : "";
  return `- ${scope}${description} (${commit.sha.slice(0, 7)})`;
}

/**
 * Group commits into the section groups, dropping the release bump commits and
 * any subject that is not conventional.
 *
 * @param {{ sha: string; subject: string }[]} commits
 * @returns {{ groups: { heading: string; entries: string[] }[]; included: number; skipped: string[] }}
 */
export function groupCommits(commits) {
  /** @type {Map<string, string[]>} */
  const buckets = new Map(GROUPS.map((group) => [group.heading, []]));
  /** @type {string[]} */
  const skipped = [];
  let included = 0;

  for (const commit of commits) {
    if (isReleaseBumpSubject(commit.subject)) {
      skipped.push(commit.subject);
      continue;
    }
    const parsed = parseConventionalSubject(commit.subject);
    if (!parsed) {
      skipped.push(commit.subject);
      continue;
    }
    const group = GROUPS.find((candidate) => candidate.types.includes(parsed.type));
    if (!group) {
      skipped.push(commit.subject);
      continue;
    }
    /** @type {string[]} */
    const bucket = buckets.get(group.heading) ?? [];
    bucket.push(renderEntry(commit));
    buckets.set(group.heading, bucket);
    included += 1;
  }

  return {
    groups: GROUPS.map((group) => ({
      heading: group.heading,
      entries: buckets.get(group.heading) ?? [],
    })),
    included,
    skipped,
  };
}

/**
 * Build the markdown section for one release.
 *
 * @param {{ version: string; date: string; summary?: string | null; commits: { sha: string; subject: string }[] }} options
 * @returns {{ section: string; included: number; skipped: string[]; headings: string[] }}
 */
export function buildChangelogSection({ version, date, summary = null, commits }) {
  const { groups, included, skipped } = groupCommits(commits);
  /** @type {string[]} */
  const lines = [`## [${version}] - ${date}`];

  const trimmedSummary = typeof summary === "string" ? summary.trim() : "";
  if (trimmedSummary.length > 0) {
    lines.push("", trimmedSummary);
  }

  /** @type {string[]} */
  const headings = [];
  for (const group of groups) {
    if (group.entries.length === 0) {
      continue;
    }
    headings.push(group.heading);
    lines.push("", `### ${group.heading}`, ...group.entries);
  }

  return { section: lines.join("\n"), included, skipped, headings };
}

/**
 * @param {string} repoRoot
 * @param {string} baseRef
 * @param {string} headRef
 * @returns {{ sha: string; subject: string }[]}
 */
export function collectCommits(repoRoot, baseRef, headRef = "HEAD") {
  const result = runGit(repoRoot, ["log", "--format=%H%x1f%s", `${baseRef}..${headRef}`]);
  if (result.status !== 0) {
    throw new Error(
      `git log ${baseRef}..${headRef} failed: ${result.stderr.trim() || "unknown error"}`,
    );
  }
  return result.stdout
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => {
      const [sha, subject = ""] = line.split("\x1f");
      return { sha, subject };
    });
}

/**
 * @param {string} repoRoot
 * @returns {{ tag: string; source: "tag" } | { commit: string; source: "changelog-baseline" } | null}
 */
export function resolveChangelogBase(repoRoot) {
  const tags = runGit(repoRoot, ["tag", "--list", "v*", "--sort=-v:refname"]);
  if (tags.status === 0) {
    const [newest] = tags.stdout.split("\n").filter((line) => line.trim().length > 0);
    if (newest !== undefined) {
      return { tag: newest.trim(), source: "tag" };
    }
  }

  const baseline = runGit(repoRoot, [
    "log",
    "--diff-filter=A",
    "-1",
    "--format=%H",
    "--",
    CHANGELOG_PATH,
  ]);
  const sha = baseline.stdout.trim();
  if (baseline.status === 0 && sha.length > 0) {
    return { commit: sha, source: "changelog-baseline" };
  }
  return null;
}

/**
 * Insert a section above the first `## ` heading (or at EOF when the file has
 * no sections yet).
 *
 * @param {string} contents
 * @param {string} section
 * @returns {string}
 */
export function prependSection(contents, section) {
  const match = contents.match(/^## /m);
  if (!match || match.index === undefined) {
    return `${contents.replace(/\s*$/, "")}\n\n${section}\n`;
  }
  const before = contents.slice(0, match.index).replace(/\s*$/, "");
  const after = contents.slice(match.index);
  return `${before}\n\n${section}\n\n${after}`;
}

const USAGE = `Usage: node tooling/release/generate-changelog.mjs --version <X.Y.Z> [--base <ref>] [--head <ref>]
         [--summary <text> | --summary-file <path>] [--date YYYY-MM-DD]
         [--prepend <CHANGELOG.md>] [--repo-root <path>]

Generates the release section (Features / Fixes / Docs & chores) from
conventional commits over <base>..<head>. Base defaults to the newest v* tag,
falling back to the commit that introduced ${CHANGELOG_PATH}.`;

function main(argv, env = process.env) {
  /** @type {{ version: string | null; base: string | null; head: string; summary: string | null; summaryFile: string | null; date: string | null; prepend: string | null; repoRoot: string | null; help: boolean }} */
  const options = {
    version: null,
    base: null,
    head: "HEAD",
    summary: null,
    summaryFile: null,
    date: null,
    prepend: null,
    repoRoot: null,
    help: false,
  };

  const valueFlags = new Set([
    "--version",
    "--base",
    "--head",
    "--summary",
    "--summary-file",
    "--date",
    "--prepend",
    "--repo-root",
  ]);

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      options.help = true;
      continue;
    }
    if (!valueFlags.has(arg)) {
      console.error(`Unknown argument: ${arg}`);
      console.error(USAGE);
      return 1;
    }
    const value = argv[++i];
    if (value === undefined) {
      console.error(`${arg} requires a value`);
      console.error(USAGE);
      return 1;
    }
    if (arg === "--version") options.version = value.trim();
    else if (arg === "--base") options.base = value.trim();
    else if (arg === "--head") options.head = value.trim();
    else if (arg === "--summary") options.summary = value;
    else if (arg === "--summary-file") options.summaryFile = value;
    else if (arg === "--date") options.date = value.trim();
    else if (arg === "--prepend") options.prepend = value;
    else if (arg === "--repo-root") options.repoRoot = value;
  }

  if (options.help) {
    console.log(USAGE);
    return 0;
  }
  if (options.version === null || !isCleanSemVer(options.version)) {
    console.error(`Invalid or missing --version: expected a clean SemVer (X.Y.Z)`);
    console.error(USAGE);
    return 1;
  }
  if (options.summary !== null && options.summaryFile !== null) {
    console.error("Use either --summary or --summary-file, not both");
    return 1;
  }

  const date = options.date ?? new Date().toISOString().slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) {
    console.error(`Invalid --date: ${date} (expected YYYY-MM-DD)`);
    return 1;
  }

  const repoRoot = options.repoRoot ?? env.NEXUS_REPO_ROOT?.trim() ?? resolveRepoRoot();

  let summary = options.summary;
  if (options.summaryFile !== null) {
    summary = readFileSync(options.summaryFile, "utf8");
  }

  let baseRef = options.base;
  if (baseRef === null) {
    const base = resolveChangelogBase(repoRoot);
    if (base === null) {
      console.error(
        `No v* tag and no commit introducing ${CHANGELOG_PATH}; cannot determine the changelog base.`,
      );
      return 1;
    }
    baseRef = base.source === "tag" ? base.tag : base.commit;
    console.error(
      base.source === "tag"
        ? `changelog base: ${baseRef} (newest v* tag)`
        : `changelog base: ${baseRef} (first-release fallback: ${CHANGELOG_PATH} baseline commit)`,
    );
  }

  let commits;
  try {
    commits = collectCommits(repoRoot, baseRef, options.head);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    return 1;
  }

  const built = buildChangelogSection({
    version: options.version,
    date,
    summary,
    commits,
  });
  console.error(
    `changelog: ${commits.length} commits in ${baseRef}..${options.head}, ${built.included} listed, ${built.skipped.length} skipped (non-conventional or release bumps)`,
  );

  if (options.prepend !== null) {
    const target = join(repoRoot, options.prepend);
    const existing = readFileSync(target, "utf8");
    writeFileSync(target, prependSection(existing, built.section), "utf8");
    console.error(`prepended ${options.version} section to ${options.prepend}`);
  } else {
    console.log(built.section);
  }
  return 0;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  try {
    process.exit(main(process.argv.slice(2)));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
