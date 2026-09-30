/**
 * Shared harness for release-script unit tests.
 *
 * Fixtures are copied from the real repository so the tests exercise the actual
 * version surfaces (root `Cargo.toml`, root `package.json`, the Electron
 * package + product descriptor, `Cargo.lock`, every workspace member manifest)
 * instead of a hand-maintained mock.
 */

import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CARGO_LOCK_PATH,
  CARGO_WORKSPACE_PATH,
  LOCKSTEP_PATHS,
  parseCargoWorkspaceMembers,
} from "./version-surfaces.mjs";

const RELEASE_DIR = dirname(fileURLToPath(import.meta.url));

/** Repository that provides the fixtures. */
export const REPO_ROOT = join(RELEASE_DIR, "../..");

/** Files copied verbatim into every temp repo. */
export const FIXTURE_PATHS = [...LOCKSTEP_PATHS, CARGO_LOCK_PATH];

/**
 * @param {string} repoRoot
 * @param {string[]} args
 * @param {{ input?: string }} [options]
 * @returns {{ status: number; stdout: string; stderr: string }}
 */
export function git(repoRoot, args, options = {}) {
  const result = spawnSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
    input: options.input,
  });
  if (result.error) {
    throw result.error;
  }
  return { status: result.status ?? 1, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
}

/**
 * @param {string} repoRoot
 * @param {string[]} args
 */
export function gitOk(repoRoot, args) {
  const result = git(repoRoot, args);
  if (result.status !== 0) {
    throw new Error(`git ${args.join(" ")} failed: ${result.stderr.trim()}`);
  }
  return result.stdout.trim();
}

/**
 * Create a temp repo with the release version surfaces and workspace member
 * manifests copied from the real repository.
 *
 * @param {{ files?: Record<string, string> }} [options] extra files to write
 * @returns {string}
 */
export function createTempRepo(options = {}) {
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-"));
  for (const rel of FIXTURE_PATHS) {
    const dest = join(dir, rel);
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(join(REPO_ROOT, rel), dest);
  }

  const workspace = readFileSync(join(REPO_ROOT, CARGO_WORKSPACE_PATH), "utf8");
  for (const memberPath of parseCargoWorkspaceMembers(workspace)) {
    const rel = join(memberPath, "Cargo.toml");
    const dest = join(dir, rel);
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(join(REPO_ROOT, rel), dest);
  }

  for (const [rel, contents] of Object.entries(options.files ?? {})) {
    const dest = join(dir, rel);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, contents, "utf8");
  }

  return dir;
}

/**
 * Initialise git in a temp repo and publish `refs/remotes/origin/main`, so
 * scripts can read `origin/main:<path>` the way they do in CI after a fetch.
 *
 * @param {string} dir
 * @returns {string} the initial commit sha
 */
export function initGitRepo(dir) {
  gitOk(dir, ["init", "-q", "-b", "main"]);
  gitOk(dir, ["config", "user.email", "release-test@example.com"]);
  gitOk(dir, ["config", "user.name", "Release Test"]);
  // Ambient developer config must not leak into fixtures (signed commits, tag
  // signing, hooks would all make the tests depend on the host).
  gitOk(dir, ["config", "commit.gpgsign", "false"]);
  gitOk(dir, ["config", "tag.gpgsign", "false"]);
  gitOk(dir, ["add", "-A"]);
  gitOk(dir, ["commit", "-q", "-m", "init"]);
  const sha = gitOk(dir, ["rev-parse", "HEAD"]);
  gitOk(dir, ["update-ref", "refs/remotes/origin/main", "HEAD"]);
  return sha;
}

/**
 * Create an annotated release tag (the release flow only ever creates
 * annotated `v*` tags).
 *
 * @param {string} dir
 * @param {string} name
 * @param {string} [ref]
 * @returns {string} tag object sha
 */
export function tagRelease(dir, name, ref = "HEAD") {
  gitOk(dir, ["tag", "-a", name, ref, "-m", `Release ${name}`]);
  return gitOk(dir, ["rev-parse", name]);
}

/**
 * @param {string} dir
 * @param {string} relPath
 * @param {string} contents
 * @param {string} message
 * @returns {string} commit sha
 */
export function commitFile(dir, relPath, contents, message) {
  const dest = join(dir, relPath);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, contents, "utf8");
  gitOk(dir, ["add", "-A"]);
  gitOk(dir, ["commit", "-q", "-m", message]);
  return gitOk(dir, ["rev-parse", "HEAD"]);
}

/**
 * @param {string} dir
 */
export function cleanupTempRepo(dir) {
  rmSync(dir, { recursive: true, force: true });
}

/**
 * Run a release script as a child process against a temp repo.
 *
 * @param {string} scriptName
 * @param {string[]} args
 * @param {string} repoRoot
 * @param {{ cwd?: string; input?: string; env?: Record<string, string> }} [options]
 * @returns {{ status: number; stdout: string; stderr: string }}
 */
export function runReleaseScript(scriptName, args, repoRoot, options = {}) {
  const scriptPath = join(RELEASE_DIR, scriptName);
  const result = spawnSync(process.execPath, [scriptPath, ...args], {
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, NEXUS_REPO_ROOT: repoRoot, ...options.env },
    encoding: "utf8",
    input: options.input,
  });
  if (result.error) {
    throw result.error;
  }
  return { status: result.status ?? 1, stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
}

/**
 * @param {string} repoRoot
 * @param {string} relPath
 * @returns {string}
 */
export function readRepoFile(repoRoot, relPath) {
  return readFileSync(join(repoRoot, relPath), "utf8");
}

/**
 * @param {string} repoRoot
 * @param {string} relPath
 * @param {string} contents
 */
export function writeRepoFile(repoRoot, relPath, contents) {
  const dest = join(repoRoot, relPath);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, contents, "utf8");
}

/**
 * Rewrite a repo file in place through a transform (used to drift one version
 * surface and assert the check catches it).
 *
 * @param {string} repoRoot
 * @param {string} relPath
 * @param {(contents: string) => string} transform
 * @returns {string} the new contents
 */
export function mutateRepoFile(repoRoot, relPath, transform) {
  const updated = transform(readRepoFile(repoRoot, relPath));
  writeRepoFile(repoRoot, relPath, updated);
  return updated;
}

/**
 * @param {string} repoRoot
 * @param {string} relPath
 */
export function removeRepoFile(repoRoot, relPath) {
  rmSync(join(repoRoot, relPath), { force: true });
}
