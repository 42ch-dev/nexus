#!/usr/bin/env node
/**
 * Release version surfaces — the five-file version SSOT for Nexus.
 *
 * Four hand-written files must always carry the same version:
 *   - `Cargo.toml` `[workspace.package].version` (all workspace crates inherit it)
 *   - root `package.json`
 *   - `apps/desktop-electron/package.json`
 *   - `apps/desktop-electron/resources/product.json`
 *
 * `Cargo.lock` is the fifth file: it pins the resolved version of every
 * workspace member, so the bump regenerates it with
 * `cargo update --workspace --offline` after the manifests are written — Cargo,
 * not a hand-rolled text edit, owns the lockfile. Cargo rewrites exactly the
 * workspace member `[[package]]` entries and leaves non-member entries and
 * dependency pins alone (verified 2026-09-30 in a scratch checkout).
 *
 * Because the vehicle is Cargo, the bump step needs the Rust toolchain on the
 * runner plus a populated registry cache (`--offline` never fetches): a bump
 * job must run after `cargo fetch`/a build in the same `CARGO_HOME`.
 *
 * @module tooling/release/version-surfaces
 */

import { spawnSync } from "node:child_process";
import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RELEASE_DIR = dirname(fileURLToPath(import.meta.url));

/** Repository root derived from this file's location. */
export const REPO_ROOT = join(RELEASE_DIR, "../..");

/**
 * Repo root to operate on: `NEXUS_REPO_ROOT` when set (tests, scratch
 * checkouts), otherwise this file's repository.
 *
 * @returns {string}
 */
export function resolveRepoRoot() {
  const override = process.env.NEXUS_REPO_ROOT?.trim();
  return override && override.length > 0 ? override : REPO_ROOT;
}

/** Cargo workspace manifest — `[workspace.package].version`. */
export const CARGO_WORKSPACE_PATH = "Cargo.toml";
/** Root package manifest. */
export const ROOT_PACKAGE_PATH = "package.json";
/** Electron host package manifest. */
export const ELECTRON_PACKAGE_PATH = "apps/desktop-electron/package.json";
/** Electron packaging product descriptor (asserted by the packaging preflight). */
export const PRODUCT_JSON_PATH = "apps/desktop-electron/resources/product.json";
/** Cargo lockfile pinning workspace member versions. */
export const CARGO_LOCK_PATH = "Cargo.lock";
/**
 * The contracted `Cargo.lock` regeneration vehicle: workspace members only, no
 * network, lockfile written by Cargo itself.
 */
const CARGO_LOCK_REGEN_ARGS = ["update", "--workspace", "--offline"];
/** Human-readable form of the regen vehicle for messages and docs. */
export const CARGO_LOCK_REGEN_COMMAND = `cargo ${CARGO_LOCK_REGEN_ARGS.join(" ")}`;

/**
 * The four hand-written surfaces whose versions must be equal.
 * @type {readonly string[]}
 */
export const LOCKSTEP_PATHS = [
  CARGO_WORKSPACE_PATH,
  ROOT_PACKAGE_PATH,
  ELECTRON_PACKAGE_PATH,
  PRODUCT_JSON_PATH,
];

/**
 * @param {string} value
 * @returns {string}
 */
function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * The `[workspace.package]` table body (up to the next table header).
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @returns {{ body: string; start: number }}
 */
function workspacePackageSection(contents, manifestPath) {
  const match = contents.match(/(?:^|\n)\[workspace\.package\][\s\S]*?(?=\n\[|$)/);
  if (match?.index === undefined) {
    throw new Error(`${manifestPath}: missing [workspace.package] section`);
  }
  const start = match.index + (match[0].startsWith("\n") ? 1 : 0);
  return { body: contents.slice(start, match.index + match[0].length), start };
}

/**
 * @param {string} contents Root `Cargo.toml` contents.
 * @param {string} [manifestPath]
 * @returns {string}
 */
export function parseWorkspacePackageVersion(
  contents,
  manifestPath = CARGO_WORKSPACE_PATH,
) {
  const { body } = workspacePackageSection(contents, manifestPath);
  const version = body.match(/^version\s*=\s*"([^"]*)"/m)?.[1];
  if (version === undefined) {
    throw new Error(
      `${manifestPath}: missing version = "..." in [workspace.package]`,
    );
  }
  return version;
}

/**
 * Replace the workspace version in place, leaving every other byte untouched.
 *
 * @param {string} contents
 * @param {string} version
 * @param {string} [manifestPath]
 * @returns {string}
 */
export function replaceWorkspacePackageVersion(
  contents,
  version,
  manifestPath = CARGO_WORKSPACE_PATH,
) {
  const { body, start } = workspacePackageSection(contents, manifestPath);
  const updated = body.replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`);
  if (updated === body) {
    throw new Error(
      `${manifestPath}: could not find version = "..." in [workspace.package]`,
    );
  }
  return contents.slice(0, start) + updated + contents.slice(start + body.length);
}

/**
 * @param {string} contents JSON file contents.
 * @param {string} path
 * @returns {string}
 */
export function parseJsonVersion(contents, path) {
  let data;
  try {
    data = JSON.parse(contents);
  } catch (error) {
    throw new Error(`${path}: invalid JSON (${error instanceof Error ? error.message : error})`);
  }
  const version = data?.version;
  if (typeof version !== "string" || version.length === 0) {
    throw new Error(`${path}: missing "version" string field`);
  }
  return version;
}

/**
 * Replace the top-level `version` field. Refuses files that are not canonical
 * `JSON.stringify(data, null, 2)` + trailing newline, so a bump never silently
 * reformats unrelated lines into the release diff.
 *
 * @param {string} contents
 * @param {string} version
 * @param {string} path
 * @returns {string}
 */
export function replaceJsonVersion(contents, version, path) {
  const data = JSON.parse(contents);
  if (typeof data?.version !== "string") {
    throw new Error(`${path}: missing "version" string field`);
  }
  if (`${JSON.stringify(data, null, 2)}\n` !== contents) {
    throw new Error(
      `${path}: not canonical 2-space JSON with a trailing newline; refusing to rewrite`,
    );
  }
  data.version = version;
  return `${JSON.stringify(data, null, 2)}\n`;
}

/**
 * Read the version of one file surface.
 *
 * @param {string} repoRoot
 * @param {string} path
 * @returns {string}
 */
export function readSurfaceVersion(repoRoot, path) {
  const contents = readFileSync(join(repoRoot, path), "utf8");
  return path === CARGO_WORKSPACE_PATH
    ? parseWorkspacePackageVersion(contents, path)
    : parseJsonVersion(contents, path);
}

/**
 * @param {string} repoRoot
 * @returns {{ path: string; version: string }[]}
 */
export function readSurfaceVersions(repoRoot) {
  return LOCKSTEP_PATHS.map((path) => ({
    path,
    version: readSurfaceVersion(repoRoot, path),
  }));
}

/**
 * Workspace member directories declared by `[workspace].members`.
 *
 * @param {string} contents Root `Cargo.toml` contents.
 * @returns {string[]}
 */
export function parseCargoWorkspaceMembers(contents) {
  const workspaceStart = contents.indexOf("[workspace]");
  const workspaceSection =
    workspaceStart < 0
      ? null
      : contents.slice(workspaceStart + "[workspace]".length);
  const membersBody = workspaceSection?.match(
    /^\s*members\s*=\s*\[([\s\S]*?)\]/m,
  )?.[1];
  if (membersBody === undefined) {
    throw new Error("Cargo.toml: missing [workspace].members array");
  }
  const memberPaths = [
    ...membersBody.replace(/#.*$/gm, "").matchAll(/"([^"]+)"/g),
  ].map(([, memberPath]) => memberPath);
  for (const memberPath of memberPaths) {
    if (/[*?[\]]/.test(memberPath)) {
      throw new Error(
        `Cargo.toml: workspace member "${memberPath}" uses glob metacharacters; explicit member paths are required`,
      );
    }
  }
  return memberPaths;
}

/**
 * Package name of a workspace member manifest.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @returns {string}
 */
export function parseCargoPackageName(contents, manifestPath) {
  const section = contents.match(/(?:^|\n)\[package\][\s\S]*?(?=\n\[|$)/)?.[0];
  const name = section?.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
  if (!name) {
    throw new Error(`${manifestPath}: missing [package].name`);
  }
  return name;
}

/**
 * Workspace members with their package names (the Cargo.lock SSOT rows).
 *
 * @param {string} repoRoot
 * @returns {{ path: string; manifestPath: string; name: string }[]}
 */
export function readWorkspaceMemberPackages(repoRoot) {
  const workspace = readFileSync(join(repoRoot, CARGO_WORKSPACE_PATH), "utf8");
  return parseCargoWorkspaceMembers(workspace).map((memberPath) => {
    const manifestPath = join(memberPath, "Cargo.toml");
    return {
      path: memberPath,
      manifestPath,
      name: parseCargoPackageName(
        readFileSync(join(repoRoot, manifestPath), "utf8"),
        manifestPath,
      ),
    };
  });
}

/**
 * @param {string} contents Cargo.lock contents.
 * @param {string} packageName
 * @returns {string | null}
 */
export function parseCargoLockPackageVersion(contents, packageName) {
  const escaped = escapeRegExp(packageName);
  const match = contents.match(
    new RegExp(
      `^\\[\\[package\\]\\]\\nname = "${escaped}"\\nversion = "([^"]+)"`,
      "m",
    ),
  );
  return match?.[1] ?? null;
}

/**
 * Regenerate `Cargo.lock` from the manifests on disk with Cargo itself.
 *
 * Fails loudly when Cargo is unavailable or refuses to resolve: a lockfile
 * that was not produced by Cargo must never be committed with the bump.
 *
 * @param {string} repoRoot
 * @returns {void}
 */
function regenerateCargoLock(repoRoot) {
  const result = spawnSync("cargo", CARGO_LOCK_REGEN_ARGS, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.error) {
    throw new Error(
      `${CARGO_LOCK_PATH}: \`${CARGO_LOCK_REGEN_COMMAND}\` could not run (${result.error.message}); ` +
        "the release runner needs the Rust toolchain and a populated cargo registry cache",
    );
  }
  if (result.status !== 0) {
    const detail = `${result.stderr ?? ""}${result.stdout ?? ""}`.trim();
    throw new Error(
      `${CARGO_LOCK_PATH}: \`${CARGO_LOCK_REGEN_COMMAND}\` failed (exit ${result.status})` +
        (detail.length > 0 ? `: ${detail}` : ""),
    );
  }
}

/**
 * Every workspace member must already be pinned in `Cargo.lock`. Checked
 * before the first write so a drifted lockfile refuses the bump instead of
 * leaving a half-bumped repository behind.
 *
 * @param {string} lockContents
 * @param {readonly string[]} packageNames
 * @returns {void}
 */
function assertLockEntriesExist(lockContents, packageNames) {
  for (const packageName of packageNames) {
    if (parseCargoLockPackageVersion(lockContents, packageName) === null) {
      throw new Error(
        `${CARGO_LOCK_PATH}: missing [[package]] entry for ${packageName}`,
      );
    }
  }
}

/**
 * Replace `path` with `contents` in one step: the bytes land in a
 * same-directory temporary file that is then renamed over the target, so no
 * reader can observe a half-written surface and the target never depends on a
 * prior existence check.
 *
 * @param {string} path
 * @param {string} contents
 * @returns {void}
 */
function writeFileAtomically(path, contents) {
  const tempPath = `${path}.tmp`;
  writeFileSync(tempPath, contents, "utf8");
  renameSync(tempPath, path);
}

/**
 * Write the release version into all five surfaces: the four hand-written
 * manifests first, then `Cargo.lock` through `cargo update --workspace
 * `--offline`. A Cargo failure rolls every written file back, so a refused
 * bump leaves the repository untouched.
 *
 * @param {string} repoRoot
 * @param {string} version
 * @returns {{ path: string; from: string; members: number }}[] changed files
 */
export function writeReleaseVersion(repoRoot, version) {
  /** @type {{ path: string; contents: string; from: string; members: number }[]} */
  const planned = [];

  for (const path of LOCKSTEP_PATHS) {
    const contents = readFileSync(join(repoRoot, path), "utf8");
    const from =
      path === CARGO_WORKSPACE_PATH
        ? parseWorkspacePackageVersion(contents, path)
        : parseJsonVersion(contents, path);
    if (from === version) {
      continue;
    }
    const updated =
      path === CARGO_WORKSPACE_PATH
        ? replaceWorkspacePackageVersion(contents, version, path)
        : replaceJsonVersion(contents, version, path);
    planned.push({ path, contents: updated, from, members: 0 });
  }

  const lockPath = join(repoRoot, CARGO_LOCK_PATH);
  let lockContents;
  try {
    // Read directly instead of probing with `existsSync` first: the check would
    // open a check/use window on a path this function later rewrites.
    lockContents = readFileSync(lockPath, "utf8");
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw error;
    }
    throw new Error(
      `${CARGO_LOCK_PATH}: missing; cannot regenerate workspace member versions`,
    );
  }
  const memberNames = readWorkspaceMemberPackages(repoRoot).map(
    (member) => member.name,
  );
  assertLockEntriesExist(lockContents, memberNames);

  if (planned.length === 0) {
    return [];
  }

  const originals = planned.map((entry) => ({
    path: entry.path,
    contents: readFileSync(join(repoRoot, entry.path), "utf8"),
  }));
  try {
    for (const entry of planned) {
      writeFileSync(join(repoRoot, entry.path), entry.contents, "utf8");
    }
    regenerateCargoLock(repoRoot);
  } catch (error) {
    for (const entry of originals) {
      writeFileAtomically(join(repoRoot, entry.path), entry.contents);
    }
    writeFileAtomically(lockPath, lockContents);
    throw error;
  }

  const changed = planned.map(({ path, from, members }) => ({ path, from, members }));
  if (readFileSync(lockPath, "utf8") !== lockContents) {
    changed.push({
      path: CARGO_LOCK_PATH,
      from: "members",
      members: memberNames.length,
    });
  }
  return changed;
}
