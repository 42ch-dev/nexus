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
 *      (`tooling/release/fixtures/package-consumers/<name>/`) as a fresh
 *      workspace whose `[patch.crates-io]` entries point only at extracted
 *      selected archives, offline and locked.
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
 * each archive hash, normalized dependency row and consumer provenance.
 *
 * CLI: node tooling/release/check-contract-packageability.mjs
 *
 * Exit 0 when all contract crates are packageable (and every present consumer
 * fixture passes); non-zero otherwise.
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
 */

/**
 * Run a command and keep the streams apart: `cargo metadata` writes JSON to
 * stdout while warnings may land on stderr.
 *
 * @param {string} command
 * @param {string[]} args
 * @param {string} cwd
 * @returns {{ ok: boolean; stdout: string; stderr: string; output: string }}
 */
function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, CARGO_TERM_COLOR: "never" },
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
    cpSync(source, destination, { recursive: true });
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
 * Consume one extracted archive set with a fresh consumer workspace. Every
 * failure returns named evidence instead of throwing, so a Cargo failure can
 * never leave an empty passing report.
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
  const fixturePath = join(repoRoot, CONSUMER_FIXTURES_PATH, entryName);
  if (!existsSync(fixturePath)) {
    return null;
  }

  const consumerRoot = join(scratchRoot, "consumers", entryName);
  cpSync(fixturePath, consumerRoot, { recursive: true });
  /** @type {ConsumerEvidence} */
  const evidence = {
    case: `${CONSUMER_FIXTURES_PATH}/${entryName}`,
    root: consumerRoot,
    declared: [],
    patched: [],
    resolved: [],
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

  const selected = evidence.declared.filter((dependency) =>
    archives.has(dependency.name),
  );
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

  const ran = run(
    "cargo",
    [
      "run",
      "--offline",
      "--locked",
      "--quiet",
      ...patchArgs,
      "--target-dir",
      join(targetRoot, "consumers", entryName),
    ],
    consumerRoot,
  );
  evidence.output = ran.output;
  if (!ran.ok) {
    return {
      ...evidence,
      cause: `consumer run failed: ${
        firstErrorLine(ran.output) ?? "cargo run failed"
      }`,
    };
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
 * @param {string} repoRoot
 * @returns {PackageabilityReport}
 */
export function checkContractPackageability(repoRoot) {
  const entries = readVersionBrackets(repoRoot);
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
      existsSync(join(repoRoot, CONSUMER_FIXTURES_PATH, entry.name)),
    );
    if (hasConsumer && archives.size > 0) {
      const policy = workspacePolicy(stageRoot);
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
    }
  } finally {
    rmSync(scratchRoot, { recursive: true, force: true });
  }

  const firstFailure = results.find((result) => !result.ok)?.name ?? null;
  return { ok: firstFailure === null, results, firstFailure };
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
    console.log(
      `  ${result.name.padEnd(24)} ${result.manifest.padEnd(44)} ${
        result.ok ? "ok" : "FAILED"
      }  ${archive}${consumer}`,
    );
  }
}

function main(argv) {
  if (argv.includes("--help") || argv.includes("-h")) {
    console.log(
      "Usage: node tooling/release/check-contract-packageability.mjs",
    );
    return 0;
  }

  const report = checkContractPackageability(resolveRepoRoot());
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
