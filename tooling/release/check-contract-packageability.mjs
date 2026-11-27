#!/usr/bin/env node
/**
 * For each entry in `VERSION_BRACKETS_PATH` this
 *
 *   1. copies the source Git would ship — tracked files with their working-tree
 *      contents plus untracked files that are not ignored — into a disposable
 *      packing tree, so the archive carries the source that exists now rather
 *      than stale Git HEAD, and never a gitignored local file,
 *   2. runs `cargo package --no-verify --allow-dirty --offline` there into a
 *      throwaway `--target-dir`,
 *   3. keeps the produced `.crate` through the whole run, records its SHA-256,
 *      extracts it outside the repository and packing tree, and reads the
 *      normalized manifest through `cargo metadata --no-deps`,
 *   4. runs the optional consumer fixture declared for the entry
 *      (`tooling/release/fixtures/package-consumers/<declared dir>/`, read from
 *      the same staged tree the archives were packed from, so package and
 *      consumer evidence share one set of reviewed inputs) as a fresh workspace
 *      whose `[patch.crates-io]` entries point only at extracted selected
 *      archives, offline and locked, with `cargo run` or — when the fixture
 *      ships a `tests/` target, the only target kind that sees a
 *      dev-dependency — `cargo test`; a fixture that ships a `guest/` crate also
 *      gets that crate built for `wasm32-unknown-unknown` from the same
 *      archives, with the host consumer kept out of it. The fixture a row
 *      declares must be one of the fixtures that actually resolved that row's
 *      archive, so a declaration pointing at another entry's fixture fails
 *      rather than passing on that other fixture's coverage.
 *
 * Nothing downstream sees the original checkout or the packing tree: a
 * consumer that resolves an input from either fails, as does an archive that
 * was not produced (missing packed input), a path or private dependency, an
 * incompatible sibling requirement, a consumer-supplied Cargo config, or any
 * Cargo failure. Cargo strips `path` dependencies when it packs a manifest, so
 * a contract crate that depends on another contract crate cannot resolve it
 * from the registry while both are unpublished; packing therefore receives
 * the staged contract crates its manifest depends on as `--config
 * patch.crates-io.<name>.path`, and those bootstrap patches never reach a
 * consumer. Unused packing patches are not passed: Cargo records them as
 * `[[patch.unused]]` rows and rewrites the staging lock.
 *
 * The whole scratch tree — packing copy, archives, extracted crates, consumer
 * roots, lockfiles and target dirs — is removed in `finally`; the report keeps
 * each archive hash, normalized dependency row, consumer provenance and the
 * revision the staged tree was verified against. A gitlink (`160000 commit`,
 * e.g. an initialized skill submodule) is one tree entry: its subtree is
 * skipped on both sides of that verification, because its contents are not part
 * of this revision's tree.
 *
 * A caller may pass a `shareRoot` — which the caller removes — to reuse
 * compiled work across several runs: extracted archives are cached there under
 * a digest of their source (see {@link treeDigest}), and the consumer target dir
 * lives there too, so a second case that ships the same sources resolves and
 * builds from Cargo's own cache instead of compiling the contract graph again.
 * The digest ignores `.cargo_vcs_info.json` because `cargo package` stamps it
 * with the staging commit, which is the only way two runs of identical sources
 * differ. Nothing else is shared: each run still stages, packs, extracts and
 * reports on its own inputs, and the CLI never passes a `shareRoot`, so a gate
 * run stays hermetic.
 *
 * CLI: node tooling/release/check-contract-packageability.mjs [--json] [--require-clean]
 *
 * Exit 0 when all contract crates are packageable (and every present consumer
 * fixture passes, and every archive is consumed by a passing consumer);
 * non-zero otherwise. `--require-clean` additionally refuses unless the packed
 * source is the clean reviewed revision, and then emits no report.
 *
 * @module tooling/release/check-contract-packageability
 */

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  closeSync,
  constants,
  cpSync,
  existsSync,
  fstatSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  renameSync,
  rmSync,
  statSync,
  symlinkSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { readVersionBrackets, resolveRepoRoot } from "./version-surfaces.mjs";

const PACKAGE_ARGS = ["package", "--no-verify", "--allow-dirty", "--offline"];

/** Consumer fixtures, each declared by name in the accounting below. */
const CONSUMER_FIXTURES_PATH = "tooling/release/fixtures/package-consumers";

/**
 *
 *   { "<package>": { "fixture": "<dir name>" } }
 *   { "<package>": { "fixture": null, "reason": "<why none>" } }
 *
 * It is what separates "this repository has no consumer fixture by design" from
 * "the fixtures were removed": a declared fixture that is missing from the tree
 * fails the run, the declared directory is the one that runs, and an entry the
 * file does not mention fails too, so deleting a fixture means editing this file
 * deliberately. A coordinated rename of a directory *and* its record is
 * therefore an accepted deliberate act: the pair is one reviewed commit, and
 * that review is the guard — no content ledger is kept, so the checker cannot
 * tell that rename from a decision to rename.
 */
const CONSUMER_ACCOUNTING_PATH = "tooling/release/fixtures/package-consumers.json";

const GUEST_DIR = "guest";

const SCRATCH_PREFIX = "nexus-contract-package-";

/**
 * `cargo package` writes this file with the packing commit, so it is the one
 * path two archives of identical sources disagree on. {@link treeDigest} skips
 * it, which is what lets both archives share one extracted cache entry.
 */
const VCS_STAMP_PATH = ".cargo_vcs_info.json";

/**
 * Runs one deadline-bound child in its own process group and takes that group
 * down when it is signalled or its parent disappears. `spawnSync` cannot do this
 * from here: it blocks the loop, so a signal is not dispatched while it waits.
 */
const SUPERVISOR_PATH = fileURLToPath(
  new URL("./supervise-child.mjs", import.meta.url),
);

const SKIPPED_STAGE_ENTRIES = new Set([
  ".direnv",
  ".git",
  ".mstar",
  ".worktrees",
  "node_modules",
  "target",
]);

/** @type {number | null | undefined} */
let deadlineMemo;

/**
 * Wall clock the caller allows this process, as epoch milliseconds in
 * `NEXUS_RELEASE_DEADLINE_MS`. Every child gets what is left of it, so a case
 * that outgrows its budget dies as a named failure inside that budget instead of
 * letting the whole step be killed with nothing to read. Unset or empty means no
 * deadline, which is what the CLI gate uses; anything else must be a future
 * integer, because a malformed value silently read as "unset" is a budget nobody
 * is enforcing. Read on the first child, not at import, so a runner that imports
 * this module can set the budget in its own body.
 *
 * @returns {number | null}
 */
function childDeadline() {
  if (deadlineMemo === undefined) {
    const raw = process.env.NEXUS_RELEASE_DEADLINE_MS?.trim();
    if (raw === undefined || raw.length === 0) {
      deadlineMemo = null;
    } else {
      const value = Number(raw);
      if (!Number.isInteger(value) || value <= Date.now()) {
        throw new Error(
          `release tooling deadline (NEXUS_RELEASE_DEADLINE_MS) must be a future epoch-millisecond timestamp, got "${raw}"`,
        );
      }
      deadlineMemo = value;
    }
  }
  return deadlineMemo;
}


/**
 * @param {string} manifestContents
 * @param {import("./version-surfaces.mjs").ContractBracketEntry[]} entries
 * @returns {import("./version-surfaces.mjs").ContractBracketEntry[]}
 */
function contractDependencies(manifestContents, entries) {
  return entries.filter((entry) =>
    new RegExp(`^\\s*${entry.name}\\s*[.=]`, "m").test(manifestContents),
  );
}

/**
 * @typedef {object} ArchiveEvidence
 * @property {string} file `.crate` file name produced by `cargo package`.
 * @property {string} sha256 SHA-256 of the archive bytes, kept after cleanup.
 * @property {number} bytes Archive size.
 */

/**
 * @typedef {object} NormalizedDependency
 * @property {string} name Package name.
 * @property {string} req Cargo version requirement from the packed manifest.
 * @property {string} kind `normal`, `dev` or `build`.
 */

/**
 * @typedef {object} ConsumerEvidence
 * @property {string} case Repository-relative consumer fixture path.
 * @property {string} root Disposable consumer root (removed after the run).
 * @property {{name: string, req: string, kind: string, path: string | null}[]} declared
 * @property {{name: string, sha256: string}[]} patched
 * @property {{name: string, version: string, manifest: string, source: string}[]} resolved
 * @property {string} mode `run` for a `src/main.rs` fixture, `test` when it ships a `tests/`
 * @property {{artifact: string, bytes: number, sha256: string} | null} guest
 *   `wasm32-unknown-unknown` artifact of the fixture's `guest/` crate, when it
 *   ships one; null for host-only fixtures.
 * @property {boolean} ok
 * @property {string | null} cause Short named failure reason.
 * @property {string} output Cargo's combined output from the consuming run.
 */

/**
 * @typedef {object} PackageResult
 * @property {string} name Package name.
 * @property {string} manifest Manifest path, relative to the repository root.
 * @property {boolean} ok
 * @property {string} output Cargo's combined output, kept for the failure report.
 * @property {string | null} cause Short named failure reason.
 * @property {ArchiveEvidence | null} archive
 * @property {{version: string, publish: string[] | null} | null} package
 * @property {NormalizedDependency[]} dependencies
 * @property {ConsumerEvidence | null} consumer Actual archive-only consumer, when
 *   a fixture exists for this entry.
 * @property {string[] | null} coveredBy Fixture cases whose *resolved* graph
 *   pulled this entry's archive in (`resolved[].source === "archive:<sha256>"`),
 *   sorted; null when no passing consumer resolved it. This is what the coverage
 *   gate reads — `consumer.patched` only records what was proposed.
 */

/**
 * @typedef {object} PackageabilityReport
 * @property {boolean} ok
 * @property {PackageResult[]} results One row per contract entry, in carrier order.
 * @property {string | null} firstFailure Package name of the first failing crate.
 */

/**
 * Backstop for the supervised group: the supervisor takes its whole group down
 * when it is signalled or orphaned, so this only matters if the supervisor itself
 * is gone before it could act. `spawnSync`'s timeout kills the process it started,
 * never the `rustc` children of that process, and a group pid is the only handle
 * on them. Windows has no process groups: there only the wrapper dies.
 *
 * @param {number | undefined} pid
 */
function killChildGroup(pid) {
  if (pid === undefined || process.platform === "win32") {
    return;
  }
  try {
    process.kill(-pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
  }
}

/**
 * A child under a deadline runs through {@link SUPERVISOR_PATH}: `spawnSync`
 * blocks the loop, so a signal delivered to *this* process while a build runs is
 * not dispatched to it at all (measured: a `SIGTERM` during a 3 s `spawnSync`
 * reached no handler and the call returned normally). The supervisor is the
 * process that can react — it leads the group and takes it down when it is
 * signalled or reparented. Without a deadline nothing is detached, so the caller
 * and its children share one group and an interrupt reaches them all by itself,
 * which is the CLI gate.
 *
 * @param {string[]} command
 * @returns {string[]}
 */
function supervised(command) {
  return [process.execPath, [SUPERVISOR_PATH, ...command]];
}

/**
 * @param {string} command
 * @param {string[]} args
 * @param {string} cwd
 * @param {string} [input] Stdin content, for the batched `hash-object` read.
 * @returns {{ ok: boolean; stdout: string; stderr: string; output: string }}
 */
export function run(command, args, cwd, input) {
  /** @type {{ cwd: string; encoding: "utf8"; env: Record<string, string | undefined>; input?: string; maxBuffer: number; timeout?: number; detached?: boolean }} */
  const options = {
    cwd,
    encoding: "utf8",
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
    input,
    maxBuffer: 64 * 1024 * 1024,
  };
  const deadline = childDeadline();
  if (deadline !== null) {
    // The floor keeps a nearly-expired deadline from failing a `git` call that
    // would have answered at once: a child over the floor is a real overrun. This
    // applies on every platform: only the group machinery below is POSIX-only,
    // and a hung child must never run past the budget just because it cannot be
    // supervised.
    options.timeout = Math.max(1000, deadline - Date.now());
  }
  // Detach and supervise on POSIX only: Windows has no process groups, so there a
  // timeout still kills the child it started, and its own children are the
  // platform's problem.
  const supervise = deadline !== null && process.platform !== "win32";
  if (supervise) {
    // Own process group, so a deadline kill reaches the grandchildren too.
    options.detached = true;
  }
  const [spawnedCommand, spawnedArgs] = supervise
    ? supervised([command, ...args])
    : [command, args];
  const result = spawnSync(spawnedCommand, spawnedArgs, options);
  if (result.error?.code === "ETIMEDOUT" || result.signal !== null) {
    killChildGroup(result.pid);
  }
  if (result.error !== undefined) {
    // `error:` keeps the deadline visible in the row's cause, which every caller
    // builds with {@link firstErrorLine}; a bare "could not run" would read as a
    // crash and hide that the case outgrew the budget.
    return {
      ok: false,
      stdout: "",
      stderr: "",
      output:
        result.error.code === "ETIMEDOUT"
          ? `error: ${command} ${args[0] ?? ""} exceeded the release tooling deadline (NEXUS_RELEASE_DEADLINE_MS)`
          : `${command} could not run: ${result.error.message}`,
    };
  }
  const stdout = result.stdout ?? "";
  const stderr = result.stderr ?? "";
  return {
    ok: result.status === 0,
    stdout,
    stderr,
    output: `${stdout}${stderr}`.trim(),
  };
}

/**
 *
 * @param {string} output
 * @returns {string | null}
 */
function firstErrorLine(output) {
  return (
    output
      .split("\n")
      .map((line) => line.trim())
      .find((line) => /^error(\[[^\]]+\])?:/.test(line)) ?? null
  );
}

/**
 * @param {string} cause
 * @param {string} output
 * @param {ArchiveEvidence | null} [archive]
 */
function failedRow(cause, output, archive = null) {
  return {
    ok: false,
    cause,
    output,
    archive,
    package: null,
    dependencies: [],
    root: null,
  };
}

/**
 * @returns {string}
 */
function packageScratchParent() {
  const targetDir = process.env.CARGO_TARGET_DIR?.trim();
  return targetDir !== undefined && targetDir.length > 0
    ? resolve(targetDir)
    : tmpdir();
}

/**
 * A failed listing fails closed: falling back to a verbatim walk would put
 * gitignored local files back into archives and machine-local hashes.
 *
 * @param {string} repoRoot
 * @returns {string[]}
 */
function workingSourcePaths(repoRoot) {
  const listed = run(
    "git",
    ["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    repoRoot,
  );
  if (!listed.ok) {
    throw new Error(
      `staging failed: git ls-files failed (${
        firstErrorLine(listed.output) ?? "Git could not list the working source"
      }); refusing to pack an unfiltered working tree`,
    );
  }
  return listed.stdout.split("\0").filter((path) => path.length > 0);
}

/**
 * Symlinks are re-created with their stored target bytes rather than copied by
 * `cpSync`, which resolves the target against the source directory (an absolute
 * path in the copy, which {@link verifyStagedSource} then reads as a mismatch on
 * a clean checkout — this repository keeps three such links under
 * `.claude/skills/`) and refuses one whose target is absent. A link is data:
 * its bytes are what Git stores, so nothing else is needed.
 *
 * @param {string} repoRoot
 * @param {string} stageRoot
 */
function stageWorkingSource(repoRoot, stageRoot) {
  mkdirSync(stageRoot, { recursive: true });
  for (const path of workingSourcePaths(repoRoot)) {
    if (SKIPPED_STAGE_ENTRIES.has(path.split("/")[0])) {
      continue;
    }
    const source = join(repoRoot, path);
    /** @type {import("node:fs").Stats} */
    let stats;
    try {
      stats = lstatSync(source);
    } catch (error) {
      if (error?.code === "ENOENT") {
        continue;
      }
      throw error;
    }
    const destination = join(stageRoot, path);
    mkdirSync(dirname(destination), { recursive: true });
    if (stats.isSymbolicLink()) {
      symlinkSync(readlinkSync(source), destination);
      continue;
    }
    cpSync(source, destination, { recursive: true });
    chmodSync(destination, stats.mode);
  }
}

/**
 * @param {string} stageRoot
 * @returns {{ ok: boolean; cause: string | null; publishByName: Map<string, string[] | null> }}
 */
function workspacePolicy(stageRoot) {
  const result = run(
    "cargo",
    [
      "metadata",
      "--no-deps",
      "--format-version",
      "1",
      "--offline",
      "--manifest-path",
      join(stageRoot, "Cargo.toml"),
    ],
    stageRoot,
  );
  if (!result.ok) {
    return {
      ok: false,
      cause: `workspace metadata failed: ${
        firstErrorLine(result.output) ?? "cargo metadata failed"
      }`,
      publishByName: new Map(),
    };
  }
  const packages = JSON.parse(result.stdout).packages;
  return {
    ok: true,
    cause: null,
    publishByName: new Map(
      packages.map((row) => [row.name, row.publish ?? null]),
    ),
  };
}

/**
 * @param {{
 *   entries: import("./version-surfaces.mjs").ContractBracketEntry[];
 *   entry: import("./version-surfaces.mjs").ContractBracketEntry;
 *   scratchRoot: string;
 *   stageRoot: string;
 *   targetRoot: string;
 *   shareRoot: string | null;
 * }} input
 * @returns {{
 *   ok: boolean;
 *   cause: string | null;
 *   output: string;
 *   archive: ArchiveEvidence | null;
 *   package: {version: string, publish: string[] | null} | null;
 *   dependencies: NormalizedDependency[];
 *   root: string | null;
 * }}
 */
function packAndInspect({
  entries,
  entry,
  scratchRoot,
  stageRoot,
  targetRoot,
  shareRoot,
}) {
  const manifestPath = join(stageRoot, entry.manifest);
  const manifestContents = readFileSync(manifestPath, "utf8");
  const patches = contractDependencies(manifestContents, entries)
    .filter((dependency) => dependency.name !== entry.name)
    .flatMap((dependency) => [
      "--config",
      `patch.crates-io.${dependency.name}.path = "${join(
        stageRoot,
        dirname(dependency.manifest),
      )}"`,
    ]);

  const packed = run(
    "cargo",
    [
      ...PACKAGE_ARGS,
      "--manifest-path",
      manifestPath,
      "--target-dir",
      targetRoot,
      ...patches,
    ],
    stageRoot,
  );
  if (!packed.ok) {
    return failedRow(
      `cargo package failed: ${
        firstErrorLine(packed.output) ?? "cargo package failed"
      }`,
      packed.output,
    );
  }

  const packageDir = join(targetRoot, "package");
  const produced = existsSync(packageDir)
    ? readdirSync(packageDir).filter(
        (file) =>
          file.startsWith(`${entry.name}-`) && file.endsWith(".crate"),
      )
    : [];
  if (produced.length !== 1) {
    return failedRow(
      `cargo package produced ${produced.length} archives for ${entry.name} (missing packed input)`,
      packed.output,
    );
  }

  const archivePath = join(packageDir, produced[0]);
  /** @type {ArchiveEvidence} */
  const archive = {
    file: produced[0],
    sha256: createHash("sha256")
      .update(readFileSync(archivePath))
      .digest("hex"),
    bytes: statSync(archivePath).size,
  };

  const extractDir = join(scratchRoot, "archives", entry.name);
  mkdirSync(extractDir, { recursive: true });
  const extracted = run(
    "tar",
    ["-xzf", archivePath, "-C", extractDir],
    scratchRoot,
  );
  if (!extracted.ok) {
    return failedRow(
      `archive extraction failed: ${
        firstErrorLine(extracted.output) ?? "tar failed"
      }`,
      extracted.output,
      archive,
    );
  }
  const roots = readdirSync(extractDir).filter((name) =>
    statSync(join(extractDir, name)).isDirectory(),
  );
  if (roots.length !== 1) {
    return failedRow(
      `archive extracted ${roots.length} roots instead of one`,
      extracted.output,
      archive,
    );
  }
  const extractedRoot = shareArchive(join(extractDir, roots[0]), shareRoot);

  const metadata = run(
    "cargo",
    [
      "metadata",
      "--no-deps",
      "--format-version",
      "1",
      "--offline",
      "--manifest-path",
      join(extractedRoot, "Cargo.toml"),
    ],
    extractedRoot,
  );
  if (!metadata.ok) {
    return failedRow(
      `archive metadata failed: ${
        firstErrorLine(metadata.output) ?? "cargo metadata failed"
      }`,
      metadata.output,
      archive,
    );
  }
  const packages = JSON.parse(metadata.stdout).packages;
  const normalized = packages.length === 1 ? packages[0] : null;
  if (normalized === null || normalized.name !== entry.name) {
    return failedRow(
      `archive package ${normalized?.name ?? "missing"} is not ${entry.name}`,
      metadata.output,
      archive,
    );
  }
  const dependencies = normalized.dependencies.map((dependency) => ({
    name: dependency.name,
    req: dependency.req,
    kind: dependency.kind,
    path: dependency.path ?? null,
  }));
  const escaped = dependencies.find((dependency) => dependency.path !== null);
  if (escaped !== undefined) {
    return failedRow(
      `archive retains path dependency ${escaped.name}; it would need the original checkout`,
      metadata.output,
      archive,
    );
  }

  return {
    ok: true,
    cause: null,
    output: packed.output,
    archive,
    package: { version: normalized.version, publish: normalized.publish ?? null },
    dependencies: dependencies.map(({ name, req, kind }) => ({
      name,
      req,
      kind,
    })),
    root: extractedRoot,
  };
}

/**
 *
 * @param {string} candidate
 * @param {string} root
 * @returns {boolean}
 */
function inside(candidate, root) {
  const target = resolve(candidate);
  const base = resolve(root);
  return target === base || target.startsWith(`${base}${sep}`);
}

/**
 * Content digest of an extracted archive, ignoring {@link VCS_STAMP_PATH}. Two
 * runs that ship the same sources produce the same digest, so they resolve the
 * package from the same directory and Cargo reuses the compiled graph; any
 * source difference — including a dropped file — yields another digest and thus
 * another directory, never a stale artifact.
 *
 * Every entry is opened once and read through the descriptor: nothing is stat'ed
 * and then re-opened by path, so a link swapped in between cannot make the digest
 * describe a file that is not the one that was hashed. `O_NOFOLLOW` turns a link
 * into `ELOOP`, and a link is data — its stored target — exactly as Git records
 * it.
 *
 * Windows has no `O_NOFOLLOW`, so there a descriptor follows the link and the
 * entry would collapse into the target's bytes. Its link case therefore comes
 * from the `dirent` the walk already enumerated — not from a second look at the
 * path — so the same tree hashes the same way on both platforms.
 *
 * @param {string} root
 * @returns {string}
 */
function treeDigest(root) {
  const hash = createHash("sha256");
  for (const { path, dirent } of stagedEntries(
    root,
    new Set([VCS_STAMP_PATH]),
  )) {
    const absolute = join(root, path);
    hash.update(`${path}\0`);
    if (process.platform === "win32" && dirent.isSymbolicLink()) {
      hash.update(`link:${readlinkSync(absolute)}\0`);
      continue;
    }
    let fd;
    try {
      fd = openSync(absolute, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
    } catch (error) {
      if (error?.code !== "ELOOP") {
        throw error;
      }
      hash.update(`link:${readlinkSync(absolute)}\0`);
      continue;
    }
    try {
      hash.update(`mode:${fstatSync(fd).mode}\0`);
      hash.update(readFileSync(fd));
      hash.update("\0");
    } finally {
      closeSync(fd);
    }
  }
  return hash.digest("hex");
}

/**
 * Rename `source` onto `target` as the sole decision about who publishes: no
 * check-then-use on the destination, so two producers racing for one digest
 * cannot both believe they won. `false` means another producer published first.
 *
 * @param {string} source
 * @param {string} target
 * @returns {boolean}
 */
function renameIntoPlace(source, target) {
  try {
    renameSync(source, target);
    return true;
  } catch (error) {
    if (error?.code === "EEXIST" || error?.code === "ENOTEMPTY") {
      return false;
    }
    throw error;
  }
}

/**
 * Publish one extracted archive under `target`, which may already exist because
 * another producer got there first. Returns after exactly one directory holds the
 * content; the caller's extraction is gone either way.
 *
 * @param {string} extractedRoot
 * @param {string} target
 * @returns {boolean} true when this call published the entry, false when another
 *   producer had already published it
 */
function publishExtraction(extractedRoot, target) {
  mkdirSync(dirname(target), { recursive: true });
  let published;
  try {
    published = renameIntoPlace(extractedRoot, target);
  } catch (error) {
    if (error?.code !== "EXDEV") {
      throw error;
    }
    // `shareRoot` is on another filesystem: stage a copy beside the target so the
    // publish itself is still one rename, never a copy onto a live directory.
    const staging = mkdtempSync(join(dirname(target), "incoming-"));
    try {
      const staged = join(staging, "root");
      cpSync(extractedRoot, staged, { recursive: true });
      published = renameIntoPlace(staged, target);
    } finally {
      rmSync(staging, { recursive: true, force: true });
    }
  }
  // A rename that won already moved the source away; this clears the loser's copy
  // and the copied source of an `EXDEV` publish.
  rmSync(extractedRoot, { recursive: true, force: true });
  return published;
}

/**
 * Move one extracted archive into the shared cache and return the root Cargo
 * should resolve: the caller's own extraction when no `shareRoot` is in use,
 * the cache entry once it is. A cache hit drops the caller's copy.
 *
 * Two producers may publish the same digest at once — the rename decides, and the
 * loser reads the winner's complete directory. Concurrent *builds* against one
 * root are still not supported: the consumer target dir is a single Cargo lock
 * and a single set of executables, so callers run one process at a time.
 *
 * @param {string} extractedRoot
 * @param {string | null} shareRoot
 * @returns {string}
 */
export function shareArchive(extractedRoot, shareRoot) {
  if (shareRoot === null) {
    return extractedRoot;
  }
  const shared = join(shareRoot, "archives", treeDigest(extractedRoot));
  publishExtraction(extractedRoot, shared);
  return shared;
}

/**
 * The directory named here is the one the consumer run stages, so the record and
 * the evidence cannot disagree. That also bounds what this can prove: a rename
 * of a directory together with its record is indistinguishable from a
 * deliberate decision to rename it, and is accepted as one (the pair is a single
 * reviewed commit). No content ledger is kept.
 *
 * @param {string} stageRoot
 * @param {import("./version-surfaces.mjs").ContractBracketEntry[]} entries
 * @returns {{fixtures: Map<string, string>, noFixture: Map<string, string>}}
 */
function readConsumerAccounting(stageRoot, entries) {
  const path = join(stageRoot, CONSUMER_ACCOUNTING_PATH);
  if (!existsSync(path)) {
    throw new Error(
      `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH} is missing; record every carrier entry there, with an explicit no-fixture reason where none exists`,
    );
  }
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new Error(
      `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH} is not JSON (${
        error instanceof Error ? error.message : String(error)
      })`,
    );
  }
  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(
      `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH} must be an object keyed by package name`,
    );
  }
  const names = new Set(entries.map((entry) => entry.name));
  /** @type {Map<string, string>} */
  const fixtures = new Map();
  /** @type {Map<string, string>} */
  const noFixture = new Map();
  for (const [name, record] of Object.entries(parsed)) {
    if (!names.has(name)) {
      throw new Error(
        `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH} records ${name}, which is not a carrier entry`,
      );
    }
    if (record === null || typeof record !== "object" || Array.isArray(record)) {
      throw new Error(
        `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH}: ${name} needs {"fixture": <dir> | null, "reason"?}`,
      );
    }
    const fixture = record.fixture ?? null;
    if (fixture === null) {
      const reason = typeof record.reason === "string" ? record.reason.trim() : "";
      if (reason.length === 0) {
        throw new Error(
          `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH}: ${name} has no fixture, so it needs a reason`,
        );
      }
      if (existsSync(join(stageRoot, CONSUMER_FIXTURES_PATH, name))) {
        throw new Error(
          `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH}: ${name} is declared without a fixture, but ${CONSUMER_FIXTURES_PATH}/${name} exists; declare it or remove the directory`,
        );
      }
      noFixture.set(name, reason);
      continue;
    }
    if (typeof fixture !== "string" || !existsSync(join(stageRoot, CONSUMER_FIXTURES_PATH, fixture))) {
      throw new Error(
        `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH}: ${name} declares the fixture ${String(fixture)}, which is not in the tree`,
      );
    }
    fixtures.set(name, fixture);
  }
  for (const entry of entries) {
    if (!fixtures.has(entry.name) && !noFixture.has(entry.name)) {
      throw new Error(
        `consumer accounting failed: ${CONSUMER_ACCOUNTING_PATH}: ${entry.name} is not recorded; every carrier entry needs a fixture or an explicit no-fixture reason`,
      );
    }
  }
  return { fixtures, noFixture };
}

/**
 * @param {{
 *   consumerRoot: string;
 *   buildRoot: string;
 *   entryName: string;
 *   patchArgs: string[];
 * }} input
 * @returns {{
 *   ok: boolean;
 *   cause: string | null;
 *   artifact: {artifact: string, bytes: number, sha256: string} | null;
 * }}
 */
function buildGuest({ consumerRoot, buildRoot, entryName, patchArgs }) {
  const guestManifest = join(consumerRoot, GUEST_DIR, "Cargo.toml");
  if (!existsSync(guestManifest)) {
    return { ok: true, cause: null, artifact: null };
  }

  const guestTargetDir = join(buildRoot, "guests", entryName);
  const built = run(
    "cargo",
    [
      "build",
      "--offline",
      "--locked",
      "--quiet",
      "--target",
      "wasm32-unknown-unknown",
      "--manifest-path",
      guestManifest,
      "--target-dir",
      guestTargetDir,
      ...patchArgs,
    ],
    consumerRoot,
  );
  if (!built.ok) {
    return {
      ok: false,
      cause: `guest wasm32 build failed: ${
        firstErrorLine(built.output) ?? "cargo build failed"
      }`,
      artifact: null,
    };
  }

  const outDir = join(guestTargetDir, "wasm32-unknown-unknown", "debug");
  const produced = existsSync(outDir)
    ? readdirSync(outDir).filter((file) => file.endsWith(".wasm"))
    : [];
  if (produced.length !== 1) {
    return {
      ok: false,
      cause: `guest wasm32 build produced ${produced.length} artifacts`,
      artifact: null,
    };
  }

  const bytes = readFileSync(join(outDir, produced[0]));
  if (!bytes.subarray(0, 4).equals(Buffer.from([0x00, 0x61, 0x73, 0x6d]))) {
    return {
      ok: false,
      cause: `guest artifact ${produced[0]} is not a WebAssembly module`,
      artifact: null,
    };
  }
  return {
    ok: true,
    cause: null,
    artifact: {
      artifact: produced[0],
      bytes: bytes.length,
      sha256: createHash("sha256").update(bytes).digest("hex"),
    },
  };
}

/**
 * @param {{
 *   repoRoot: string;
 *   stageRoot: string;
 *   scratchRoot: string;
 *   buildRoot: string;
 *   entryName: string;
 *   fixture: string;
 *   archives: Map<string, {root: string, sha256: string, version: string}>;
 *   policy: {ok: boolean, cause: string | null, publishByName: Map<string, string[] | null>};
 * }} input
 * @returns {ConsumerEvidence}
 */
function runConsumer({
  repoRoot,
  stageRoot,
  scratchRoot,
  buildRoot,
  entryName,
  fixture,
  archives,
  policy,
}) {
  const fixturePath = join(stageRoot, CONSUMER_FIXTURES_PATH, fixture);
  const consumerRoot = join(scratchRoot, "consumers", entryName);
  cpSync(fixturePath, consumerRoot, { recursive: true });
  /** @type {ConsumerEvidence} */
  const evidence = {
    case: `${CONSUMER_FIXTURES_PATH}/${fixture}`,
    root: consumerRoot,
    declared: [],
    patched: [],
    resolved: [],
    mode: "run",
    guest: null,
    ok: false,
    cause: null,
    output: "",
  };

  if (existsSync(join(consumerRoot, ".cargo"))) {
    return {
      ...evidence,
      cause:
        "consumer fixture supplies its own Cargo config; consumers must not inherit original or parent Cargo config",
    };
  }
  rmSync(join(consumerRoot, "Cargo.lock"), { force: true });
  const manifestPath = join(consumerRoot, "Cargo.toml");

  const declaredMeta = run(
    "cargo",
    [
      "metadata",
      "--no-deps",
      "--format-version",
      "1",
      "--offline",
      "--manifest-path",
      manifestPath,
    ],
    consumerRoot,
  );
  if (!declaredMeta.ok) {
    return {
      ...evidence,
      cause: `consumer metadata failed: ${
        firstErrorLine(declaredMeta.output) ?? "cargo metadata failed"
      }`,
      output: declaredMeta.output,
    };
  }
  evidence.declared = JSON.parse(declaredMeta.stdout).packages.flatMap((row) =>
    row.dependencies.map((dependency) => ({
      name: dependency.name,
      req: dependency.req,
      kind: dependency.kind,
      path: dependency.path ?? null,
    })),
  );

  const pathDependency = evidence.declared.find(
    (dependency) => dependency.path !== null,
  );
  if (pathDependency !== undefined) {
    return {
      ...evidence,
      cause: `consumer declares path dependency ${pathDependency.name}; consumers may consume selected archives only`,
    };
  }

  if (!policy.ok) {
    return { ...evidence, cause: policy.cause };
  }
  const privateDependency = evidence.declared.find((dependency) => {
    const publish = policy.publishByName.get(dependency.name);
    return Array.isArray(publish) && publish.length === 0;
  });
  if (privateDependency !== undefined) {
    return {
      ...evidence,
      cause: `consumer depends on private package ${privateDependency.name}`,
    };
  }

  const selected = [
    ...new Map(
      evidence.declared
        .filter((dependency) => archives.has(dependency.name))
        .map((dependency) => [dependency.name, dependency]),
    ).values(),
  ];
  evidence.patched = selected.map((dependency) => ({
    name: dependency.name,
    sha256: /** @type {{sha256: string}} */ (archives.get(dependency.name))
      .sha256,
  }));
  const patchArgs = selected.flatMap((dependency) => [
    "--config",
    `patch.crates-io.${dependency.name}.path = "${
      /** @type {{root: string}} */ (archives.get(dependency.name)).root
    }"`,
  ]);

  const resolvedLock = run(
    "cargo",
    ["generate-lockfile", "--offline", ...patchArgs],
    consumerRoot,
  );
  if (!resolvedLock.ok) {
    return {
      ...evidence,
      cause: `consumer lock resolution failed: ${
        firstErrorLine(resolvedLock.output) ?? "cargo generate-lockfile failed"
      }`,
      output: resolvedLock.output,
    };
  }

  const prepared = run(
    "cargo",
    ["fetch", "--offline", "--locked", ...patchArgs],
    consumerRoot,
  );
  if (!prepared.ok) {
    return {
      ...evidence,
      cause: `consumer cache preparation failed: ${
        firstErrorLine(prepared.output) ?? "cargo fetch failed"
      }`,
      output: prepared.output,
    };
  }

  const mode = existsSync(join(consumerRoot, "tests")) ? "test" : "run";
  evidence.mode = mode;
  const ran = run(
    "cargo",
    [
      mode,
      "--offline",
      "--locked",
      "--quiet",
      ...patchArgs,
      "--target-dir",
      // One dir for every consumer: they build the same sources, and the guest
      // target below keeps the per-entry isolation that still matters.
      join(buildRoot, "consumers"),
      ...(mode === "test" ? ["--", "--nocapture"] : []),
    ],
    consumerRoot,
  );
  evidence.output = ran.output;
  if (!ran.ok) {
    return {
      ...evidence,
      cause: `consumer ${mode} failed: ${
        firstErrorLine(ran.output) ?? `cargo ${mode} failed`
      }`,
    };
  }

  const guest = buildGuest({ consumerRoot, buildRoot, entryName, patchArgs });
  evidence.guest = guest.artifact;
  if (!guest.ok) {
    return { ...evidence, cause: guest.cause };
  }

  const resolvedMeta = run(
    "cargo",
    ["metadata", "--format-version", "1", "--offline", ...patchArgs],
    consumerRoot,
  );
  if (!resolvedMeta.ok) {
    return {
      ...evidence,
      cause: `consumer metadata failed: ${
        firstErrorLine(resolvedMeta.output) ?? "cargo metadata failed"
      }`,
      output: resolvedMeta.output,
    };
  }
  const packages = JSON.parse(resolvedMeta.stdout).packages;
  evidence.resolved = packages.map((row) => {
    const archive = archives.get(row.name);
    return {
      name: row.name,
      version: row.version,
      manifest: row.manifest_path,
      source:
        archive !== undefined
          ? `archive:${archive.sha256}`
          : (row.source ?? "workspace"),
    };
  });

  for (const row of packages) {
    if (inside(row.manifest_path, stageRoot)) {
      return {
        ...evidence,
        cause: `consumer resolved ${row.name} from the packing tree (${row.manifest_path})`,
      };
    }
    if (
      inside(row.manifest_path, repoRoot) &&
      !inside(row.manifest_path, scratchRoot)
    ) {
      return {
        ...evidence,
        cause: `consumer resolved ${row.name} from the original checkout (${row.manifest_path})`,
      };
    }
    const archive = archives.get(row.name);
    if (
      archive !== undefined &&
      resolve(row.manifest_path) !== resolve(join(archive.root, "Cargo.toml"))
    ) {
      return {
        ...evidence,
        cause: `consumer resolved ${row.name} from ${row.manifest_path}, not its selected archive`,
      };
    }
  }

  return { ...evidence, ok: true };
}

/**
 * @param {string} repoRoot
 * @returns {{sha: string | null; clean: boolean}}
 */
function workingSourceState(repoRoot) {
  const head = run("git", ["rev-parse", "HEAD"], repoRoot);
  const status = run("git", ["status", "--porcelain"], repoRoot);
  return {
    sha: head.ok ? head.stdout.trim() : null,
    clean: status.ok && status.stdout.trim().length === 0,
  };
}

/**
 *
 * @param {Buffer} bytes
 * @returns {string}
 */
function blobId(bytes) {
  return createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");
}

/**
 * A gitlink (`160000 commit`) is one tree entry, not a directory of tracked
 * content, so an initialized submodule's files are not part of the revision and
 * are not compared. Only paths the revision itself names are pruned — this is
 * not a general "skip directories" exemption.
 *
 * The entry comes from the enumeration, so a caller that needs to know whether a
 * path is a link reads it here instead of stat'ing the path again.
 *
 * @param {string} directory
 * @param {Set<string>} gitlinks
 * @returns {{ path: string; dirent: import("node:fs").Dirent }[]}
 */
function stagedEntries(directory, gitlinks) {
  /** @type {{ path: string; dirent: import("node:fs").Dirent }[]} */
  const entries = [];
  /** @param {string} relative */
  const walk = (relative) => {
    const absolute = relative.length === 0 ? directory : join(directory, relative);
    for (const dirent of readdirSync(absolute, { withFileTypes: true })) {
      const path = relative.length === 0 ? dirent.name : `${relative}/${dirent.name}`;
      if (SKIPPED_STAGE_ENTRIES.has(path.split("/")[0]) || gitlinks.has(path)) {
        continue;
      }
      if (dirent.isDirectory()) {
        walk(path);
        continue;
      }
      entries.push({ path, dirent });
    }
  };
  walk("");
  return entries.sort((left, right) =>
    left.path < right.path ? -1 : left.path > right.path ? 1 : 0,
  );
}

/**
 * @param {string} directory
 * @param {Set<string>} gitlinks
 * @returns {string[]}
 */
function stagedPaths(directory, gitlinks) {
  return stagedEntries(directory, gitlinks).map((entry) => entry.path);
}

/**
 * Differences between the revision `sha` and the copied tree at `directory`,
 * in both directions: a staged path the revision does not have (a file added
 * after the revision, which the status sample may have missed), a revision path
 * the copy lacks, and a path whose bytes differ. An empty result means the copy
 * is exactly the revision's tree for everything the packer stages — which is
 * what makes an archive hash attributable to a revision even though the copy
 * happens later.
 *
 * Symlinks are compared by their stored target. Regular files are compared by
 * the content id Git computes for the staged copy with the repository's own
 * conversion (clean filters included), because a filtered path's stored blob is
 * not its working-tree bytes. Ignored/scratch entries are skipped exactly as
 * {@link stageWorkingSource} skips them. A gitlink subtree is skipped on both
 * sides: the revision records one tree entry there, so an initialized
 * submodule's contents are neither extras nor compared.
 *
 * @param {string} repoRoot
 * @param {string} sha
 * @param {string} directory
 * @returns {string[]}
 */
export function verifyStagedSource(repoRoot, sha, directory) {
  const listed = run("git", ["ls-tree", "-r", "-z", "--full-tree", sha], repoRoot);
  if (!listed.ok) {
    return ["<unreadable tree>"];
  }
  /** @type {Map<string, {mode: string; id: string}>} */
  const revision = new Map();
  /** @type {Set<string>} */
  const gitlinks = new Set();
  for (const entry of listed.stdout.split("\0")) {
    if (entry.length === 0) {
      continue;
    }
    const tab = entry.indexOf("\t");
    if (tab < 0) {
      continue;
    }
    const [mode, type, id] = entry.slice(0, tab).split(" ");
    const path = entry.slice(tab + 1);
    if (SKIPPED_STAGE_ENTRIES.has(path.split("/")[0])) {
      continue;
    }
    if (type === "commit") {
      gitlinks.add(path);
      continue;
    }
    if (type !== "blob") {
      continue;
    }
    revision.set(path, { mode, id });
  }

  /** @type {string[]} */
  const mismatches = [];
  const staged = stagedPaths(directory, gitlinks);
  const stagedSet = new Set(staged);
  /** Staged regular files whose content id is read in one batch below. */
  const toHash = [];
  for (const path of staged) {
    const entry = revision.get(path);
    if (entry === undefined) {
      mismatches.push(`${path} (staged, absent from ${sha})`);
      continue;
    }
    const stagedFile = join(directory, path);
    /** @type {import("node:fs").Stats} */
    let stats;
    try {
      stats = lstatSync(stagedFile);
    } catch {
      mismatches.push(path);
      continue;
    }
    if (entry.mode === "120000") {
      const target = stats.isSymbolicLink() ? readlinkSync(stagedFile) : null;
      if (target === null || blobId(Buffer.from(target)) !== entry.id) {
        mismatches.push(path);
      }
      continue;
    }
    if (!stats.isFile()) {
      mismatches.push(path);
      continue;
    }
    toHash.push(path);
  }
  if (toHash.length > 0) {
    // Regular files are hashed by Git with the repository's own content
    // conversion, against the staged copy as its work tree: this repository
    // tracks binary assets through an LFS clean filter, so the stored blob of
    // such a path is a pointer rather than the working-tree bytes. Hashing the
    // staged file raw would report every clean checkout as unclean; hashing it
    // through `--work-tree` keeps the packed copy the thing being measured.
    // shortcut: `--stdin-paths` splits on newline, so a tracked path containing
    // one aborts the batch as unverifiable instead of being compared wrongly;
    // upgrade: when a tracked staged path contains a newline.
    const hashed = run(
      "git",
      ["--work-tree", directory, "hash-object", "--stdin-paths"],
      repoRoot,
      `${toHash.map((path) => join(directory, path)).join("\n")}\n`,
    );
    const ids = hashed.ok ? hashed.stdout.split("\n").filter((id) => id.length > 0) : [];
    if (ids.length !== toHash.length) {
      mismatches.push(
        `<unreadable staged content: ${
          hashed.output.split("\n")[0] || "git hash-object failed"
        }>`,
      );
    } else {
      toHash.forEach((path, index) => {
        if (ids[index] !== revision.get(path)?.id) {
          mismatches.push(path);
        }
      });
    }
  }
  for (const path of [...revision.keys()].sort()) {
    if (!stagedSet.has(path)) {
      mismatches.push(`${path} (in ${sha}, absent from the staged tree)`);
    }
  }
  return mismatches;
}

/**
 * @param {string} repoRoot
 * @param {{ shareRoot?: string }} [options] `shareRoot` reuses extracted
 *   archives and one consumer target dir across calls (see the module comment);
 *   callers sharing one must run one at a time.
 * @returns {PackageabilityReport}
 */
export function checkContractPackageability(repoRoot, options = {}) {
  const shareRoot = options.shareRoot ?? null;
  const entries = readVersionBrackets(repoRoot);
  let source = /** @type {{sha: string | null; clean: boolean; mismatch?: string[]}} */ (
    workingSourceState(repoRoot)
  );
  const scratchParent = packageScratchParent();
  mkdirSync(scratchParent, { recursive: true });
  const scratchRoot = mkdtempSync(join(scratchParent, SCRATCH_PREFIX));
  const stageRoot = join(scratchRoot, "stage");
  const targetRoot = join(scratchRoot, "target");
  /**
   * Consumer builds land here: one dir per run normally, one dir shared by every
   * call when `shareRoot` is given, so Cargo reuses the compiled graph instead of
   * rebuilding the same contract crates per case.
   */
  const buildRoot =
    shareRoot === null ? join(scratchRoot, "build") : join(shareRoot, "build");
  /** @type {PackageResult[]} */
  const results = [];
  /** @type {Map<string, {root: string, sha256: string, version: string}>} */
  const archives = new Map();
  /** @type {ReturnType<typeof readConsumerAccounting>} */
  let consumer = { fixtures: new Map(), noFixture: new Map() };
  try {
    stageWorkingSource(repoRoot, stageRoot);
    if (source.clean && source.sha !== null) {
      const mismatch = verifyStagedSource(repoRoot, source.sha, stageRoot);
      if (mismatch.length > 0) {
        source = { sha: source.sha, clean: false, mismatch };
      }
    }

    consumer = readConsumerAccounting(stageRoot, entries);

    for (const entry of entries) {
      const inspected = packAndInspect({
        entries,
        entry,
        scratchRoot,
        stageRoot,
        targetRoot,
        shareRoot,
      });
      /** @type {PackageResult} */
      const row = {
        name: entry.name,
        manifest: entry.manifest,
        ok: inspected.ok,
        output: inspected.output,
        cause: inspected.cause,
        archive: inspected.archive,
        package: inspected.package,
        dependencies: inspected.dependencies,
        consumer: null,
        coveredBy: null,
      };
      results.push(row);
      if (inspected.ok && inspected.archive !== null && inspected.root !== null) {
        archives.set(entry.name, {
          root: inspected.root,
          sha256: inspected.archive.sha256,
          version: /** @type {{version: string}} */ (inspected.package).version,
        });
      }
    }

    if (consumer.fixtures.size > 0 && archives.size > 0) {
      const policy = workspacePolicy(stageRoot);
      for (const row of results) {
        if (row.package !== null) {
          policy.publishByName.set(row.name, row.package.publish);
        }
      }
      for (const row of results) {
        const fixture = consumer.fixtures.get(row.name);
        if (!row.ok || fixture === undefined) {
          continue;
        }
        const evidence = runConsumer({
          repoRoot,
          stageRoot,
          scratchRoot,
          buildRoot,
          entryName: row.name,
          fixture,
          archives,
          policy,
        });
        row.consumer = evidence;
        if (!evidence.ok) {
          row.ok = false;
          row.cause = evidence.cause;
        }
      }
      /** @type {Map<string, {sha256: string, cases: Set<string>}>} */
      const coverage = new Map();
      for (const row of results) {
        if (row.consumer?.ok !== true) {
          continue;
        }
        for (const entry of row.consumer.resolved) {
          if (!entry.source.startsWith("archive:")) {
            continue;
          }
          const covered = coverage.get(entry.name) ?? {
            sha256: entry.source.slice("archive:".length),
            cases: new Set(),
          };
          covered.cases.add(row.consumer.case);
          coverage.set(entry.name, covered);
        }
      }
      /** Package names a passing consumer declared but never resolved. */
      const declaredOnly = new Set(
        results.flatMap((row) =>
          row.consumer?.ok === true
            ? row.consumer.patched
                .map((dependency) => dependency.name)
                .filter((name) => !coverage.has(name))
            : [],
        ),
      );
      for (const row of results) {
        if (row.archive === null || !row.ok) {
          continue;
        }
        const covered = coverage.get(row.name);
        const resolvedBy =
          covered !== undefined && covered.sha256 === row.archive.sha256
            ? [...covered.cases].sort()
            : null;
        const declared = consumer.fixtures.get(row.name);
        const declaredCase =
          declared === undefined
            ? null
            : `${CONSUMER_FIXTURES_PATH}/${declared}`;
        if (
          resolvedBy !== null &&
          (declaredCase === null || resolvedBy.includes(declaredCase))
        ) {
          row.coveredBy = resolvedBy;
          continue;
        }
        row.ok = false;
        row.cause =
          resolvedBy !== null
            ? `misdirected-fixture: ${row.name} declares ${declaredCase}, but only ${resolvedBy.join(", ")} resolved its archive`
            : declaredOnly.has(row.name)
              ? `unresolved-coverage: ${row.name} was patched into a consumer but resolution never used its archive`
              : `pack-only: no passing consumer consumes ${row.name}`;
      }
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }

  const firstFailure = results.find((result) => !result.ok)?.name ?? null;
  return {
    ok: firstFailure === null,
    results,
    firstFailure,
    consumerAccounting: {
      path: CONSUMER_ACCOUNTING_PATH,
      fixtures: [...consumer.fixtures.keys()].sort(),
      noFixture: [...consumer.noFixture]
        .map(([name, reason]) => ({ name, reason }))
        .sort((left, right) => left.name.localeCompare(right.name)),
    },
    source,
  };
}

/**
 * @param {string} text
 * @returns {string}
 */
function indent(text) {
  return text
    .split("\n")
    .map((line) => `  ${line}`)
    .join("\n");
}

/**
 * @param {PackageabilityReport} report
 */
function printRows(report) {
  for (const result of report.results) {
    const archive =
      result.archive === null ? "no-archive" : result.archive.sha256.slice(0, 12);
    const consumer =
      result.consumer === null
        ? ""
        : ` consumer ${result.consumer.ok ? "ok" : "FAILED"}`;
    const guest =
      result.consumer?.guest == null
        ? ""
        : ` guest ${result.consumer.guest.bytes}B ${result.consumer.guest.sha256.slice(0, 12)}`;
    console.log(
      `  ${result.name.padEnd(24)} ${result.manifest.padEnd(44)} ${
        result.ok ? "ok" : "FAILED"
      }  ${archive}${consumer}${guest}`,
    );
  }
}

function main(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(
      [
        "Usage: node tooling/release/check-contract-packageability.mjs [--json] [--require-clean]",
        "",
        "--json prints the report as JSON for a release plan to consume",
        "(`release-packages.mjs --check-only --archive-evidence <file>`);",
        "the human output and the exit code are unchanged.",
        "--require-clean refuses (exit 1) unless the packed source is the clean",
        "revision, and prints no report: a plan rejects such evidence as",
        "`unclean-evidence`, so a step producing it must not stay green.",
      ].join("\n"),
    );
    return 0;
  }

  /** @type {PackageabilityReport} */
  let report;
  try {
    report = checkContractPackageability(resolveRepoRoot());
  } catch (error) {
    console.error(
      `refused: ${error instanceof Error ? error.message : String(error)}`,
    );
    return 1;
  }
  if (report.source.mismatch !== undefined) {
    console.error(
      `warning: staged source differs from ${report.source.sha} at ${report.source.mismatch.length} path(s); the archives are not that revision's bytes`,
    );
  }
  if (argv.includes("--require-clean") && !report.source.clean) {
    console.error(
      `refused: unclean-evidence: the packed source is not a clean ${report.source.sha ?? "readable revision"} at ${
        report.source.mismatch?.length ?? 0
      } path(s); no receipt is emitted`,
    );
    return 1;
  }
  if (argv.includes("--json")) {
    console.log(JSON.stringify(report, null, 2));
    return report.ok ? 0 : 1;
  }
  const header = `Contract packageability (${report.results.length} crates):`;

  if (report.ok) {
    console.log(header);
    printRows(report);
    console.log(`Packageable: all ${report.results.length} contract crates`);
    return 0;
  }

  console.error(header);
  printRows(report);
  for (const failure of report.results) {
    if (failure.ok) {
      continue;
    }
    console.error(
      `\n${failure.name} (${failure.manifest}) ${failure.cause ?? "failed"}:`,
    );
    if (failure.output.length > 0) {
      console.error(indent(failure.output));
    }
    if (failure.consumer !== null && failure.consumer.output.length > 0) {
      console.error(indent(failure.consumer.output));
    }
  }
  console.error(`\nFirst failure: ${report.firstFailure}`);
  return 1;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
