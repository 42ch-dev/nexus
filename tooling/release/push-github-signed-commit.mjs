#!/usr/bin/env node
/**
 * Push working-tree changes as a GitHub-verified commit via GraphQL
 * `createCommitOnBranch` (no bot GPG key required).
 *
 * This is the spoke-ported release primitive: a plain `GITHUB_TOKEN` git push
 * would work today but produces an unsigned commit, which breaks the moment
 * `required_signatures` protection lands. `createCommitOnBranch` produces a
 * GitHub-signed (verified) commit instead.
 *
 * Resets the remote `--branch` tip to `--base-ref`, then commits the diff of
 * the current working tree against that base as one signed commit.
 *
 * The working tree must have been prepared from the same `--base-ref` tip this
 * run fetches. `--expected-base-oid` (or `RELEASE_BASE_OID`) pins that OID; a
 * mismatch is a visible refusal instead of a commit that would revert the
 * intervening `--base-ref` changes.
 *
 * CLI:
 *   GITHUB_TOKEN=… node tooling/release/push-github-signed-commit.mjs \
 *     --branch release/X.Y.Z \
 *     --message "chore(release): bump version to X.Y.Z" \
 *     [--body-file path/to/summary] \
 *     [--trailer "Nexus-Prerelease: true"] \
 *     [--base-ref main] \
 *     [--expected-base-oid <oid>] \
 *     [--repo owner/name]
 *
 * Prints `COMMIT_OID=<sha>` for later workflow steps.
 *
 * @module tooling/release/push-github-signed-commit
 */

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { resolveRepoRoot } from "./version-surfaces.mjs";

const HEADLINE_MAX_LENGTH = 256;

/**
 * @param {string} repoRoot
 * @param {string[]} args
 * @returns {string}
 */
export function git(repoRoot, args) {
  return execFileSync("git", args, { cwd: repoRoot, encoding: "utf8" }).trim();
}

/**
 * @param {string[]} argv
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {{ branch: string | null; headline: string | null; bodyFile: string | null; trailers: string[]; baseRef: string; expectedBaseOid: string | null; repo: string | null; help: boolean }}
 */
export function parseArgs(argv, env = process.env) {
  /** @type {{ branch: string | null; headline: string | null; bodyFile: string | null; trailers: string[]; baseRef: string; expectedBaseOid: string | null; repo: string | null; help: boolean }} */
  const out = {
    branch: null,
    headline: null,
    bodyFile: null,
    trailers: [],
    baseRef: "main",
    expectedBaseOid: env.RELEASE_BASE_OID?.trim() || null,
    repo: env.GITHUB_REPOSITORY ?? null,
    help: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--branch") {
      out.branch = argv[++i] ?? null;
    } else if (arg === "--message") {
      out.headline = argv[++i] ?? null;
    } else if (arg === "--body-file") {
      const path = argv[++i];
      if (path === undefined || path.trim().length === 0) {
        throw new Error("--body-file requires a path to a message body file");
      }
      out.bodyFile = path;
    } else if (arg === "--trailer") {
      const trailer = argv[++i];
      if (trailer === undefined || trailer.trim().length === 0) {
        throw new Error("--trailer requires a value like \"Nexus-Prerelease: true\"");
      }
      out.trailers.push(trailer.trim());
    } else if (arg === "--base-ref") {
      out.baseRef = argv[++i] ?? out.baseRef;
    } else if (arg === "--expected-base-oid") {
      out.expectedBaseOid = argv[++i]?.trim() || null;
    } else if (arg === "--repo") {
      out.repo = argv[++i] ?? null;
    } else if (arg === "--help" || arg === "-h") {
      out.help = true;
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  if (out.help) {
    return out;
  }
  if (!out.branch || !out.headline) {
    throw new Error("Required: --branch and --message");
  }
  if (!out.repo || !out.repo.includes("/")) {
    throw new Error("Set --repo owner/name or GITHUB_REPOSITORY");
  }

  return out;
}

/**
 * Compose the commit message: truncated headline, optional body line(s), then
 * the trailer block.
 *
 * The body precedes the trailers so the first usable line of the message body
 * is the human text (the release `summary`), not a trailer — the release `tag`
 * job annotates the tag with that first usable line.
 *
 * @param {{ headline: string; body?: string; trailers?: string[] }} options
 * @returns {{ headline: string; body?: string }}
 */
export function composeCommitMessage({ headline, body = "", trailers = [] }) {
  const trimmed =
    headline.length <= HEADLINE_MAX_LENGTH
      ? headline
      : `${headline.slice(0, HEADLINE_MAX_LENGTH - 3)}...`;
  const parts = [];
  const bodyText = typeof body === "string" ? body.trim() : "";
  if (bodyText.length > 0) {
    parts.push(bodyText);
  }
  for (const trailer of trailers) {
    const value = trailer.trim();
    if (value.length > 0) {
      parts.push(value);
    }
  }
  const messageBody = parts.join("\n");
  return messageBody.length > 0 ? { headline: trimmed, body: messageBody } : { headline: trimmed };
}

/**
 * GraphQL `createCommitOnBranch` input payload.
 *
 * @param {{ repo: string; branch: string; baseOid: string; message: { headline: string; body?: string }; fileChanges: { additions: { path: string; contents: string }[]; deletions: { path: string }[] } }} options
 * @returns {{ branch: { repositoryNameWithOwner: string; branchName: string }; message: { headline: string; body?: string }; fileChanges: { additions: { path: string; contents: string }[]; deletions: { path: string }[] }; expectedHeadOid: string }}
 */
export function buildCreateCommitInput({ repo, branch, baseOid, message, fileChanges }) {
  return {
    branch: { repositoryNameWithOwner: repo, branchName: branch },
    message,
    fileChanges,
    expectedHeadOid: baseOid,
  };
}

/**
 * Collect additions/deletions for `createCommitOnBranch` from the working tree
 * against `origin/<baseRef>`.
 *
 * @param {string} repoRoot
 * @param {string} baseRef
 * @returns {{ additions: { path: string; contents: string }[]; deletions: { path: string }[] }}
 */
export function collectFileChanges(repoRoot, baseRef) {
  const raw = git(repoRoot, ["diff", "--name-status", "--find-renames", `origin/${baseRef}`]);
  /** @type {{ path: string; contents: string }[]} */
  const additions = [];
  /** @type {{ path: string }[]} */
  const deletions = [];

  if (raw) {
    for (const line of raw.split("\n")) {
      if (!line) {
        continue;
      }
      const parts = line.split("\t");
      const code = (parts[0] ?? "")[0];

      if (code === "D") {
        const path = parts[1];
        if (path) {
          deletions.push({ path });
        }
        continue;
      }

      if (code === "R" || code === "C") {
        const from = parts[1];
        const to = parts[2];
        if (code === "R" && from) {
          deletions.push({ path: from });
        }
        if (to && existsSync(join(repoRoot, to))) {
          additions.push({
            path: to,
            contents: readFileSync(join(repoRoot, to)).toString("base64"),
          });
        }
        continue;
      }

      const path = parts[1];
      if (!path) {
        continue;
      }
      if (!existsSync(join(repoRoot, path))) {
        deletions.push({ path });
        continue;
      }
      additions.push({
        path,
        contents: readFileSync(join(repoRoot, path)).toString("base64"),
      });
    }
  }

  // `git diff` never reports untracked files; a release commit must not
  // silently drop one (e.g. a newly bootstrapped artifact).
  for (const path of git(repoRoot, ["ls-files", "--others", "--exclude-standard"])
    .split("\n")
    .filter((entry) => entry.length > 0)) {
    additions.push({
      path,
      contents: readFileSync(join(repoRoot, path)).toString("base64"),
    });
  }

  return { additions, deletions };
}

/**
 * @param {string} path
 * @param {Record<string, unknown>} [body]
 * @param {string} [method]
 * @returns {Promise<unknown>}
 */
async function ghApi(path, body, method = body === undefined ? "GET" : "POST") {
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (!token) {
    throw new Error("GITHUB_TOKEN (or GH_TOKEN) is required");
  }

  const response = await fetch(`https://api.github.com${path}`, {
    method,
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      "X-GitHub-Api-Version": "2022-11-28",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });

  const text = await response.text();
  /** @type {unknown} */
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = text;
    }
  }

  if (!response.ok) {
    throw new Error(
      `GitHub API ${method} ${path} → ${response.status}: ${typeof data === "string" ? data : JSON.stringify(data)}`,
    );
  }

  return data;
}

/**
 * @param {string} query
 * @param {Record<string, unknown>} variables
 * @returns {Promise<any>}
 */
async function ghGraphql(query, variables) {
  const data = await ghApi("/graphql", { query, variables });
  if (data && typeof data === "object" && Array.isArray(data.errors) && data.errors.length > 0) {
    throw new Error(`GraphQL errors: ${JSON.stringify(data.errors)}`);
  }
  return data;
}

/**
 * Point the remote branch at `baseOid` (create or force-update). Release
 * branches live outside the default branch, so force updates are allowed.
 *
 * Exact-match endpoint: singular `GET /git/ref/heads/<branch>`. The plural
 * `refs` form is a prefix match — `release/0.1.0` also matches
 * `release/0.1.0-alpha.3` and cannot decide existence.
 *
 * @param {string} repo
 * @param {string} branch
 * @param {string} baseOid
 */
async function ensureBranchAtOid(repo, branch, baseOid) {
  const exactRefPath = `/repos/${repo}/git/ref/heads/${branch}`;
  let exists = false;
  try {
    await ghApi(exactRefPath);
    exists = true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (!/\b404\b/.test(message) && !/Reference does not exist/i.test(message)) {
      throw error;
    }
  }

  if (exists) {
    await ghApi(`/repos/${repo}/git/refs/heads/${branch}`, { sha: baseOid, force: true }, "PATCH");
    console.log(`Updated refs/heads/${branch} → ${baseOid}`);
    return;
  }

  await ghApi(`/repos/${repo}/git/refs`, { ref: `refs/heads/${branch}`, sha: baseOid });
  console.log(`Created refs/heads/${branch} → ${baseOid}`);
}

/**
 * Refuse to diff/commit a working tree that was prepared from an older base
 * than the base ref the helper just fetched.
 *
 * The dispatch checks out `origin/<baseRef>` and mutates the version surfaces,
 * then this helper fetches `<baseRef>` again. If `<baseRef>` advanced in
 * between, diffing the prepared tree against the new ref would collect the
 * intervening commits as reversions (and could bypass the greater-than check),
 * so the only safe answer is a visible refusal: the operator re-dispatches and
 * the bump is rebuilt on the current base. With no expected OID supplied
 * (direct/backward-compatible invocation) the check is skipped.
 *
 * @param {string | null | undefined} expectedBaseOid
 * @param {string} actualBaseOid
 * @param {string} baseRef
 * @returns {null}
 */
export function verifyPinnedBase(expectedBaseOid, actualBaseOid, baseRef = "main") {
  if (!expectedBaseOid || expectedBaseOid === actualBaseOid) {
    return null;
  }
  throw new Error(
    `Stale base: the prepared working tree was based on origin/${baseRef} ${expectedBaseOid}, ` +
      `but origin/${baseRef} is now ${actualBaseOid}. Refusing to create a commit from a stale tree ` +
      `(it would revert the newer origin/${baseRef} changes). Re-dispatch the "New release" workflow.`,
  );
}

export const USAGE = `Usage: GITHUB_TOKEN=… node tooling/release/push-github-signed-commit.mjs \\
  --branch <name> --message <headline> [--body-file <path>] [--trailer "<Name>: <value>"]… [--base-ref main] [--expected-base-oid <oid>] [--repo owner/name]`;

export async function main(argv = process.argv.slice(2)) {
  const { branch, headline, bodyFile, trailers, baseRef, expectedBaseOid, repo, help } =
    parseArgs(argv);
  if (help) {
    console.log(USAGE);
    return 0;
  }

  const repoRoot = resolveRepoRoot();

  git(repoRoot, ["fetch", "origin", baseRef, "--prune"]);
  const baseOid = git(repoRoot, ["rev-parse", `origin/${baseRef}`]);
  verifyPinnedBase(expectedBaseOid, baseOid, baseRef);
  const fileChanges = collectFileChanges(repoRoot, baseRef);

  if (fileChanges.additions.length === 0 && fileChanges.deletions.length === 0) {
    console.error(`No file changes vs origin/${baseRef}; nothing to commit on ${branch}.`);
    return 1;
  }

  await ensureBranchAtOid(/** @type {string} */ (repo), /** @type {string} */ (branch), baseOid);

  const message = composeCommitMessage({
    headline: /** @type {string} */ (headline),
    body: bodyFile ? readFileSync(bodyFile, "utf8") : "",
    trailers,
  });

  const result = await ghGraphql(
    `mutation($input: CreateCommitOnBranchInput!) {
      createCommitOnBranch(input: $input) {
        commit { oid url }
      }
    }`,
    {
      input: buildCreateCommitInput({
        repo: /** @type {string} */ (repo),
        branch: /** @type {string} */ (branch),
        baseOid,
        message,
        fileChanges,
      }),
    },
  );

  const commit = result?.data?.createCommitOnBranch?.commit ?? null;
  if (!commit || typeof commit.oid !== "string") {
    throw new Error(`Unexpected GraphQL response: ${JSON.stringify(result)}`);
  }

  console.log(
    `Created GitHub-signed commit ${commit.oid} on ${branch} (${fileChanges.additions.length} add, ${fileChanges.deletions.length} del)`,
  );
  if (typeof commit.url === "string") {
    console.log(commit.url);
  }
  // Machine-readable for Actions steps (do not change the prefix).
  console.log(`COMMIT_OID=${commit.oid}`);
  return 0;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  main().then(
    (code) => {
      process.exit(code);
    },
    (error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exit(1);
    },
  );
}
