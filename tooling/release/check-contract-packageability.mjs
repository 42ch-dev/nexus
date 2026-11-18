#!/usr/bin/env node
/**
 * Contract bracket packageability — prove every contract crate can be packed
 * and consumed from its own archive.
 *
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
 *   4. runs the optional consumer fixture named after the entry
 *      (`tooling/release/fixtures/package-consumers/<name>/`, read from the same
 *      staged tree the archives were packed from, so package and consumer
 *      evidence share one set of reviewed inputs) as a fresh workspace whose
 *      `[patch.crates-io]` entries point only at extracted selected archives,
 *      offline and locked, with `cargo run` or — when the fixture ships a
 *      `tests/` target, the only target kind that sees a dev-dependency —
 *      `cargo test`; a fixture that ships a `guest/` crate also gets that crate
 *      built for `wasm32-unknown-unknown` from the same archives, with the host
 *      consumer kept out of it.
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
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { readVersionBrackets, resolveRepoRoot } from "./version-surfaces.mjs";

const PACKAGE_ARGS = ["package", "--no-verify", "--allow-dirty", "--offline"];

/** Consumer fixtures read by entry name; K4/K5 add the real consumers here. */
const CONSUMER_FIXTURES_PATH = "tooling/release/fixtures/package-consumers";

/**
 * Subdirectory of a consumer fixture whose crate is a `wasm32-unknown-unknown`
 * guest. A guest is a separate crate from the host consumer, so no host-side
 * dependency can reach the artifact.
 */
const GUEST_DIR = "guest";

/**
 * Prefix of the disposable scratch dir. It lives inside `CARGO_TARGET_DIR`
 * when that is set, so a worktree keeps its scoped target cache, and falls
 * back to the OS temp dir otherwise.
 */
const SCRATCH_PREFIX = "nexus-contract-package-";

/**
 * Top-level entries never copied into the packing tree: VCS metadata, the
 * harness control dir, other checkouts and build outputs are not product
 * source and must not reach an archive.
 */
const SKIPPED_STAGE_ENTRIES = new Set([
  ".direnv",
  ".git",
  ".mstar",
  ".worktrees",
  "node_modules",
  "target",
]);

/**
 * Contract entries the manifest names as a dependency (`name = ...` or
 * `name.path = ...`). Cargo package names are hyphen/underscore/alphanumeric,
 * so they need no regex escaping.
 *
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
 *   target (the only target kind that sees a dev-dependency).
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
 *   Normalized `[package]` row read from the extracted archive.
 * @property {NormalizedDependency[]} dependencies
 * @property {ConsumerEvidence | null} consumer Actual archive-only consumer, when
 *   a fixture exists for this entry.
 */

/**
 * @typedef {object} PackageabilityReport
 * @property {boolean} ok
 * @property {PackageResult[]} results One row per contract entry, in carrier order.
 * @property {string | null} firstFailure Package name of the first failing crate.
 * @property {{sha: string | null; clean: boolean; mismatch?: string[]}} source
 *   Revision the packed source came from, and whether the staged bytes are that
 *   revision's bytes: `clean` is true only for a tree with no uncommitted change
 *   **and** staged files whose blobs match `sha` (see `verifyStagedSource`).
 *   `mismatch` names the paths that broke the binding. A consumer may bind
 *   archive hashes to the revision only when `clean` is true and `sha` is the
 *   reviewed revision; a dirty tree makes no claim at all.
 */

/**
 * Run a command and keep the streams apart: `cargo metadata` writes JSON to
 * stdout while warnings may land on stderr.
 *
 * @param {string} command
 * @param {string[]} args
 * @param {string} cwd
 * @param {string} [input] Stdin content, for the batched `hash-object` read.
 * @returns {{ ok: boolean; stdout: string; stderr: string; output: string }}
 */
function run(command, args, cwd, input) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
    input,
    // The staging path list is a whole checkout; the default pipe buffer is
    // smaller than a big `git ls-files` result.
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error !== undefined) {
    return {
      ok: false,
      stdout: "",
      stderr: "",
      output: `${command} could not run: ${result.error.message}`,
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
 * First Cargo/tool error line of an output, used as the short named cause.
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
 * Scratch parent: the worktree's own `CARGO_TARGET_DIR` when configured, else
 * the OS temp dir. The scratch dir is a child of it and always disposable.
 *
 * @returns {string}
 */
function packageScratchParent() {
  const targetDir = process.env.CARGO_TARGET_DIR?.trim();
  return targetDir !== undefined && targetDir.length > 0
    ? resolve(targetDir)
    : tmpdir();
}

/**
 * Paths Git would ship for the current working source: tracked files (with any
 * uncommitted modifications) plus untracked files that are not ignored. Git is
 * only asked for the path list; every byte copied comes from the working tree,
 * so an archive never carries stale HEAD or index content.
 *
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
 * Copy the source Git would ship into a disposable packing tree. The skip set
 * stays as a guard for VCS/build entries even if a checkout tracks one, and
 * executable bits and symlinks are preserved because packed build scripts and
 * tools depend on them.
 *
 * `verbatimSymlinks` copies a link's target bytes instead of resolving them
 * against the source directory: a tracked relative link (this repository keeps
 * three under `.claude/skills/`) must stay relative, or {@link verifyStagedSource}
 * compares an absolute path with the revision's stored target and reports a
 * clean checkout as unclean.
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
        // Listed by Git but already removed from the working tree.
        continue;
      }
      throw error;
    }
    const destination = join(stageRoot, path);
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(source, destination, { recursive: true, verbatimSymlinks: true });
    if (!stats.isSymbolicLink()) {
      chmodSync(destination, stats.mode);
    }
  }
}

/**
 * Package-name policy from Cargo's normalized metadata for the staged
 * workspace. `publish: []` is Cargo's "publish = false"; anything else
 * (including an unset key) is not this check's business.
 *
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
 * Pack one contract entry from the staging tree, keep its archive, extract it
 * outside the repository and read the normalized manifest Cargo actually
 * shipped.
 *
 * @param {{
 *   entries: import("./version-surfaces.mjs").ContractBracketEntry[];
 *   entry: import("./version-surfaces.mjs").ContractBracketEntry;
 *   scratchRoot: string;
 *   stageRoot: string;
 *   targetRoot: string;
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
  const extractedRoot = join(extractDir, roots[0]);

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
 * Whether `candidate` is `root` or a descendant of it.
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
 * Build a consumer fixture's `wasm32-unknown-unknown` guest, when it ships one
 * in `guest/`. The guest crate is a separate workspace member, so the host
 * consumer's dependencies stay out of the artifact, and the target itself
 * refuses host-only crates (nexus-module-test, wasmtime).
 *
 * @param {{
 *   consumerRoot: string;
 *   targetRoot: string;
 *   entryName: string;
 *   patchArgs: string[];
 * }} input
 * @returns {{
 *   ok: boolean;
 *   cause: string | null;
 *   artifact: {artifact: string, bytes: number, sha256: string} | null;
 * }}
 */
function buildGuest({ consumerRoot, targetRoot, entryName, patchArgs }) {
  const guestManifest = join(consumerRoot, GUEST_DIR, "Cargo.toml");
  if (!existsSync(guestManifest)) {
    return { ok: true, cause: null, artifact: null };
  }

  const guestTargetDir = join(targetRoot, "guests", entryName);
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
 * Consume one extracted archive set with a fresh consumer workspace. Every
 * failure returns named evidence instead of throwing, so a Cargo failure can
 * never leave an empty passing report. `repoRoot` is only the escape detector's
 * reference (a consumer must not resolve from the original checkout); the
 * fixture itself is read from the verified stage.
 *
 * @param {{
 *   repoRoot: string;
 *   stageRoot: string;
 *   scratchRoot: string;
 *   targetRoot: string;
 *   entryName: string;
 *   archives: Map<string, {root: string, sha256: string, version: string}>;
 *   policy: {ok: boolean, cause: string | null, publishByName: Map<string, string[] | null>};
 * }} input
 * @returns {ConsumerEvidence | null}
 */
function runConsumer({
  repoRoot,
  stageRoot,
  scratchRoot,
  targetRoot,
  entryName,
  archives,
  policy,
}) {
  // Fixture existence and contents come from the same copied tree the archives
  // were packed from, so package and consumer evidence share one set of
  // reviewed inputs; the original checkout is no longer a consumer input.
  const fixturePath = join(stageRoot, CONSUMER_FIXTURES_PATH, entryName);
  if (!existsSync(fixturePath)) {
    return null;
  }

  const consumerRoot = join(scratchRoot, "consumers", entryName);
  cpSync(fixturePath, consumerRoot, { recursive: true, verbatimSymlinks: true });
  /** @type {ConsumerEvidence} */
  const evidence = {
    case: `${CONSUMER_FIXTURES_PATH}/${entryName}`,
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

  // One patch per archive; a workspace fixture may name the same archive from
  // more than one member.
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

  // A fixture that ships a `tests/` target is exercised with `cargo test`: a dev-dependency is
  // visible to a test target only, so that is the shape a module crate's ABI harness has.
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
      join(targetRoot, "consumers", entryName),
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

  const guest = buildGuest({ consumerRoot, targetRoot, entryName, patchArgs });
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
    // The scratch tree can sit under CARGO_TARGET_DIR, which may itself be
    // inside the checkout; only inputs outside it are original-source escapes.
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
 * Revision the working source is packed from. The archive bytes come from the
 * working tree (see {@link stageWorkingSource}), so the revision alone is not
 * provenance: `clean` says whether that tree still matched it. Both reads fail
 * closed — an unreadable revision or status reports "not clean".
 *
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
 * Git blob id of a byte sequence: how Git's object database addresses content.
 *
 * @param {Buffer} bytes
 * @returns {string}
 */
function blobId(bytes) {
  return createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");
}

/**
 * Every file and symlink under `directory`, relative and POSIX-joined, minus the
 * entries staging never copies (VCS metadata, build output, other checkouts) and
 * minus the subtrees the revision records as gitlinks.
 *
 * A gitlink (`160000 commit`) is one tree entry, not a directory of tracked
 * content, so an initialized submodule's files are not part of the revision and
 * are not compared. Only paths the revision itself names are pruned — this is
 * not a general "skip directories" exemption.
 *
 * @param {string} directory
 * @param {Set<string>} gitlinks
 * @returns {string[]}
 */
function stagedPaths(directory, gitlinks) {
  /** @type {string[]} */
  const paths = [];
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
      paths.push(path);
    }
  };
  walk("");
  return paths.sort();
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
    // one aborts the batch as unverifiable instead of being compared wrongly.
    const hashed = run(
      "git",
      ["--work-tree", directory, "hash-object", "--stdin-paths"],
      repoRoot,
      // Absolute paths: `hash-object` opens exactly these, and `--work-tree`
      // is what maps them back to the staged tree's attributes.
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
 * @returns {PackageabilityReport}
 */
export function checkContractPackageability(repoRoot) {
  const entries = readVersionBrackets(repoRoot);
  let source = /** @type {{sha: string | null; clean: boolean; mismatch?: string[]}} */ (
    workingSourceState(repoRoot)
  );
  const scratchParent = packageScratchParent();
  mkdirSync(scratchParent, { recursive: true });
  const scratchRoot = mkdtempSync(join(scratchParent, SCRATCH_PREFIX));
  const stageRoot = join(scratchRoot, "stage");
  const targetRoot = join(scratchRoot, "target");
  /** @type {PackageResult[]} */
  const results = [];
  /** @type {Map<string, {root: string, sha256: string, version: string}>} */
  const archives = new Map();
  try {
    stageWorkingSource(repoRoot, stageRoot);
    // The copy happens after the status sample, so a file can change in
    // between. Only a clean tree is claimed, and only when the bytes actually
    // staged hash to that revision's blobs; anything else downgrades the stamp
    // to "not clean" instead of naming a revision the archive does not match.
    if (source.clean && source.sha !== null) {
      const mismatch = verifyStagedSource(repoRoot, source.sha, stageRoot);
      if (mismatch.length > 0) {
        source = { sha: source.sha, clean: false, mismatch };
      }
    }

    for (const entry of entries) {
      const inspected = packAndInspect({
        entries,
        entry,
        scratchRoot,
        stageRoot,
        targetRoot,
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

    const hasConsumer = entries.some((entry) =>
      existsSync(join(stageRoot, CONSUMER_FIXTURES_PATH, entry.name)),
    );
    if (hasConsumer && archives.size > 0) {
      const policy = workspacePolicy(stageRoot);
      // Standalone contract crates (`modules/*`) are not workspace members, so
      // the staged workspace cannot report their `publish`; their archive's own
      // normalized row is the source for them.
      for (const row of results) {
        if (row.package !== null) {
          policy.publishByName.set(row.name, row.package.publish);
        }
      }
      for (const row of results) {
        if (!row.ok) {
          continue;
        }
        const consumer = runConsumer({
          repoRoot,
          stageRoot,
          scratchRoot,
          targetRoot,
          entryName: row.name,
          archives,
          policy,
        });
        if (consumer === null) {
          continue;
        }
        row.consumer = consumer;
        if (!consumer.ok) {
          row.ok = false;
          row.cause = consumer.cause;
        }
      }
      // An entry with no fixture of its own is consumed by another entry's
      // fixture (knowledge by narrative), so a passing package row is not proof
      // its archive is usable. When fixtures exist, every archive must be
      // consumed by a passing consumer; a row without one stays a pack-only
      // result and is named as such instead of counting as covered.
      const consumed = new Set(
        results.flatMap((row) =>
          row.consumer?.ok === true
            ? row.consumer.patched.map((dependency) => dependency.name)
            : [],
        ),
      );
      for (const row of results) {
        if (row.archive === null || !row.ok || consumed.has(row.name)) {
          continue;
        }
        row.ok = false;
        row.cause = `pack-only: no passing consumer consumes ${row.name}`;
      }
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }

  const firstFailure = results.find((result) => !result.ok)?.name ?? null;
  return { ok: firstFailure === null, results, firstFailure, source };
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

  const report = checkContractPackageability(resolveRepoRoot());
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
