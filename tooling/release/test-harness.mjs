/**
 * Shared harness for release-script unit tests.
 *
 * Fixtures are copied from the real repository so the tests exercise the actual
 * version surfaces (root `Cargo.toml`, root `package.json`, the Electron
 * package + product descriptor, `Cargo.lock`, every workspace member manifest)
 * instead of a hand-maintained mock.
 *
 * Two fixture rules keep the suite independent of the repository's own state:
 * every version surface is pinned to {@link FIXTURE_BASELINE_VERSION} (tests
 * never inherit — nor hard-code — the live release number), and each copied
 * crate gets stub target files so `cargo` can load the fixture workspace.
 */

import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CARGO_LOCK_PATH,
  CARGO_WORKSPACE_PATH,
  LOCKSTEP_PATHS,
  parseCargoLockPackageVersion,
  parseCargoWorkspaceMembers,
  readWorkspaceMemberPackages,
  replaceJsonVersion,
  replaceWorkspacePackageVersion,
} from "./version-surfaces.mjs";

const RELEASE_DIR = dirname(fileURLToPath(import.meta.url));

/** Repository that provides the fixtures. */
export const REPO_ROOT = join(RELEASE_DIR, "../..");

/** Files copied verbatim into every temp repo. */
const FIXTURE_PATHS = [...LOCKSTEP_PATHS, CARGO_LOCK_PATH];

/**
 * Version pinned into every fixture repo. Fixed on purpose: a fixture that
 * inherited the live version would make the whole suite fail as soon as the
 * first governed release set the repository to the version a test expects.
 */
export const FIXTURE_BASELINE_VERSION = "9.8.7";

/** Bump target for fixture repos; strictly greater than the baseline. */
export const FIXTURE_TARGET_VERSION = "9.8.8";

/** Content of the generated crate target stubs. */
const TARGET_STUB = "// release-fixture stub\n";

/** `[lib]` / `[[bin]]` / `[[example]]` / `[[test]]` / `[[bench]]` headers. */
const TARGET_SECTION_PATTERN = /^\s*\[\[?(lib|bin|example|test|bench)\]?\]\s*$/;

/** Dependency tables that may carry a `path = "..."` key. */
const DEPENDENCY_SECTION_PATTERN =
  /^\s*\[(?:target\.[^\]]+\.)?(?:dev-|build-)?dependencies\]\s*$/;

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
 * Create a temp repo with the release version surfaces, the workspace member
 * manifests and the crate tree Cargo needs to load the fixture workspace
 * (manifests copied verbatim, target sources stubbed).
 *
 * Versions are pinned to `version` — never inherited from `sourceRoot` — so a
 * test can assert against {@link FIXTURE_BASELINE_VERSION} regardless of what
 * the live repository carries.
 *
 * @param {{ sourceRoot?: string; version?: string; files?: Record<string, string> }} [options]
 * @returns {string}
 */
export function createTempRepo(options = {}) {
  const sourceRoot = options.sourceRoot ?? REPO_ROOT;
  const dir = mkdtempSync(join(tmpdir(), "nexus-release-"));
  for (const rel of FIXTURE_PATHS) {
    const dest = join(dir, rel);
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(join(sourceRoot, rel), dest);
  }

  for (const rel of collectCrateManifests(sourceRoot)) {
    const crateDir = dirname(rel);
    const destDir = join(dir, crateDir);
    mkdirSync(destDir, { recursive: true });
    const contents = readFileSync(join(sourceRoot, rel), "utf8");
    writeFileSync(join(destDir, "Cargo.toml"), contents, "utf8");
    for (const target of targetPaths(sourceRoot, crateDir, contents)) {
      const dest = join(destDir, target);
      mkdirSync(dirname(dest), { recursive: true });
      writeFileSync(dest, TARGET_STUB, "utf8");
    }
  }

  pinFixtureVersion(dir, options.version ?? FIXTURE_BASELINE_VERSION);

  for (const [rel, contents] of Object.entries(options.files ?? {})) {
    const dest = join(dir, rel);
    mkdirSync(dirname(dest), { recursive: true });
    writeFileSync(dest, contents, "utf8");
  }

  return dir;
}

/**
 * Workspace member manifests plus every crate reachable through a `path`
 * dependency, so the fixture workspace loads without the real sources.
 *
 * @param {string} sourceRoot
 * @returns {string[]} manifest paths relative to the repository root
 */
function collectCrateManifests(sourceRoot) {
  const workspace = readFileSync(join(sourceRoot, CARGO_WORKSPACE_PATH), "utf8");
  /** @type {string[]} */
  const queue = [...parseCargoWorkspaceMembers(workspace)];
  const seen = new Set();
  /** @type {string[]} */
  const manifests = [];
  while (queue.length > 0) {
    const cratePath = /** @type {string} */ (queue.shift());
    if (seen.has(cratePath)) {
      continue;
    }
    seen.add(cratePath);
    const manifestPath = join(cratePath, "Cargo.toml");
    const contents = readFileSync(join(sourceRoot, manifestPath), "utf8");
    manifests.push(manifestPath);
    for (const dependency of pathDependencies(contents)) {
      queue.push(join(cratePath, dependency));
    }
  }
  return manifests;
}

/**
 * `path = "..."` values of every dependency table in a manifest.
 *
 * @param {string} manifest
 * @returns {string[]}
 */
function pathDependencies(manifest) {
  /** @type {string[]} */
  const paths = [];
  let inDependencies = false;
  for (const line of manifest.split("\n")) {
    if (/^\s*\[/.test(line)) {
      inDependencies = DEPENDENCY_SECTION_PATTERN.test(line);
      continue;
    }
    if (!inDependencies) {
      continue;
    }
    const match = line.match(/path\s*=\s*"([^"]+)"/);
    if (match !== null) {
      paths.push(match[1]);
    }
  }
  return paths;
}

/**
 * Files Cargo expects for a crate's targets. Declared paths are mirrored from
 * the manifest; when nothing is declared the real crate's default target is
 * mirrored instead (`src/lib.rs` / `src/main.rs`).
 *
 * @param {string} sourceRoot
 * @param {string} crateDir crate directory relative to the repository root
 * @param {string} manifest
 * @returns {string[]}
 */
function targetPaths(sourceRoot, crateDir, manifest) {
  /** @type {string[]} */
  const paths = [];
  let section = null;
  let libSection = false;
  let libPath = false;
  for (const line of manifest.split("\n")) {
    const header = line.match(TARGET_SECTION_PATTERN);
    if (header !== null) {
      section = header[1];
      libSection ||= section === "lib";
      continue;
    }
    if (/^\s*\[/.test(line)) {
      section = null;
      continue;
    }
    if (section === null) {
      continue;
    }
    const match = line.match(/^\s*path\s*=\s*"([^"]+)"/);
    if (match === null) {
      continue;
    }
    paths.push(match[1]);
    libPath ||= section === "lib";
  }
  if (libSection && !libPath) {
    paths.push("src/lib.rs");
  }
  if (paths.length > 0) {
    return paths;
  }
  const crateRoot = join(sourceRoot, crateDir);
  return existsSync(join(crateRoot, "src/lib.rs")) ? ["src/lib.rs"] : ["src/main.rs"];
}

/**
 * Pin every version surface of a fixture repo to `version`, `Cargo.lock`
 * member entries included.
 *
 * The lockfile is rewritten in place here on purpose: this is fixture setup for
 * a throwaway tree, not the release path — the bump regenerates `Cargo.lock`
 * with `cargo update --workspace --offline`.
 *
 * @param {string} dir
 * @param {string} version
 * @returns {void}
 */
function pinFixtureVersion(dir, version) {
  for (const path of LOCKSTEP_PATHS) {
    const contents = readFileSync(join(dir, path), "utf8");
    const updated =
      path === CARGO_WORKSPACE_PATH
        ? replaceWorkspacePackageVersion(contents, version, path)
        : replaceJsonVersion(contents, version, path);
    writeFileSync(join(dir, path), updated, "utf8");
  }

  const lockPath = join(dir, CARGO_LOCK_PATH);
  let lock = readFileSync(lockPath, "utf8");
  for (const member of readWorkspaceMemberPackages(dir)) {
    lock = pinLockMemberVersion(lock, member.name, version);
  }
  writeFileSync(lockPath, lock, "utf8");
}

/**
 * @param {string} lock
 * @param {string} packageName
 * @param {string} version
 * @returns {string}
 */
function pinLockMemberVersion(lock, packageName, version) {
  const current = parseCargoLockPackageVersion(lock, packageName);
  if (current === null) {
    throw new Error(`${CARGO_LOCK_PATH}: fixture is missing ${packageName}`);
  }
  if (current === version) {
    return lock;
  }
  const escaped = packageName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(
    `(^\\[\\[package\\]\\]\\nname = "${escaped}"\\nversion = ")[^"]+(")`,
    "m",
  );
  return lock.replace(pattern, `$1${version}$2`);
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
