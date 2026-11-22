#!/usr/bin/env node
/**
 * provenance, content-matched resume and selected-only version preparation.
 *
 * The inventory is `tooling/release/version-brackets.json` through
 * `readVersionBrackets`, the same eight entries the packageability checker
 * packs, so this helper keeps no second list of package names.
 *
 * Boundary: no registry client lives here. `planPackageRelease` reads the
 * repository (manifests plus `git` provenance) and a caller-supplied registry
 * view; `publishSelected` is the only publication seam and it receives its
 * client from the caller. The CLI offers `--check-only` and `--prepare` only,
 * so no path in this file can read a token or upload an archive.
 *
 * Provenance: the whole worktree must be clean at `--source`/`sourceSha`. The
 * checker packs working source, so only a clean tree lets archive hashes be
 * attributed to a revision; an unreadable revision or status refuses instead of
 * reporting a clean tree it could not observe.
 *
 * Registry view shape (`registry` argument / `--registry-state`):
 *   { name: "crates-io", published?: { name, version, checksum }[] | null,
 *     writable?: boolean }
 * `published: null` (the CLI default) means "unknown": nothing is claimed about
 * live registry state, so every selected package stays pending with that reason
 * and unselected prerequisites stay `unverified` — which blocks publication
 * instead of assuming absence. `writable: false` is an explicit permission
 * refusal; absent means unconfigured, which is disclosed, not fabricated.
 *
 * Archive evidence (`evidence` argument / `--archive-evidence`) is the JSON that
 * `check-contract-packageability.mjs --json` prints: it carries the revision it
 * packed and whether that tree was clean, plus each archive's sha256. It is a
 * planner input only — this helper never runs the checker and never refreshes it.
 *
 * Both JSON documents are shape-checked before planning reads them: a wrong
 * shape is a named refusal (`registry-state-unusable` / `evidence-unreadable`,
 * `input-unusable` for unreadable JSON) and a usage error, never a TypeError
 * from a downstream property read.
 *
 * A plan has two verdicts: `ok` (no refusal) and `ready` (also every
 * prerequisite resolved to a satisfying selected or known-published version,
 * every pending row carrying a verified archive, and the registry view known —
 * an unknown view cannot rule out that the version already exists with other
 * content). `--check-only` exits 0 for `ok` and prints what is missing;
 * `--require-publishable` is the gate, and `publishSelected` refuses `not-ready`.
 *
 * CLI:
 *   node tooling/release/release-packages.mjs --check-only [--require-publishable] \
 *     --source <40-hex sha> --packages a,b --registry crates-io \
 *     [--registry-state <json>] [--archive-evidence <json>]
 *   node tooling/release/release-packages.mjs --prepare --packages a,b \
 *     --versions a=1.2.3,b=1.2.4 [--date YYYY-MM-DD]
 *
 * Exit 0 for a valid plan or applied preparation, 1 for a refusal (or, with
 * `--require-publishable`, a blocker), 2 for a usage error.
 */

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { compareSemVer, isReleaseSemVer } from "./semver.mjs";
import {
  parseCargoPackageName,
  parseCargoPackageVersion,
  readVersionBrackets,
  resolveRepoRoot,
} from "./version-surfaces.mjs";

/**
 * GitHub label of the Cargo package family. The product pipeline owns the
 * `release` label and the `v*` tag namespace; a package release uses neither.
 */
export const PACKAGE_RELEASE_LABEL = "crate-release";

/** Registries this helper can label. A label is a tag fact, never a lookup. */
const REGISTRY_NAMES = new Set(["crates-io"]);

/** Full Git object name of a reviewed source revision. */
const SOURCE_SHA_PATTERN = /^[0-9a-f]{40}$/;

/**
 * Tag of one package release. Product releases tag `v<version>`, which triggers
 * `release.yml`, so package tags live under their own `crate/` namespace.
 *
 * @param {string} name
 * @param {string} version
 * @returns {string}
 */
export function releaseTag(name, version) {
  if (typeof name !== "string" || !/^[a-z0-9][a-z0-9-]*$/.test(name)) {
    throw new Error(`Invalid package name for a release tag: "${name}"`);
  }
  if (!isReleaseSemVer(version)) {
    throw new Error(`Invalid package version for a release tag: "${version}"`);
  }
  return `crate/${name}/v${version}`;
}

/**
 */
class ReleaseRefusal extends Error {
  /**
   * @param {string} code
   * @param {string} message
   */
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

/**
 * @param {string} value
 * @returns {string}
 */
function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * string: bare, `"basic"` or `'literal'`. Both quote forms are ordinary TOML,
 * so a scan that accepts one and ignores the other skips a declaration instead
 * of refusing it — the one outcome this file must never produce.
 *
 * @param {string} name
 * @returns {string}
 */
function nameForms(name) {
  const escaped = escapeRegExp(name);
  return `(?:${escaped}|"${escaped}"|'${escaped}')`;
}

/**
 * revision reviewable, and it needs no network.
 *
 * @param {string} repoRoot
 * @param {string[]} args
 * @returns {{ ok: boolean; stdout: string }}
 */
function git(repoRoot, args) {
  const result = spawnSync("git", args, { cwd: repoRoot, encoding: "utf8" });
  if (result.error !== undefined || result.status !== 0) {
    return { ok: false, stdout: "" };
  }
  return { ok: true, stdout: result.stdout ?? "" };
}

/**
 * Reviewed source revision plus every uncommitted change in the worktree.
 *
 * The whole tree, not the selection: the packageability checker stages the
 * tracked/untracked working source of the entire repository, so any modified or
 * untracked source file can change archive bytes, dependency edges or ordering.
 * A worktree with no change is the only state in which the archive hashes can
 * be attributed to `sha`.
 *
 * Both reads fail closed: an unreadable revision or status refuses instead of
 * reporting a clean tree it could not observe.
 *
 * @param {string} repoRoot
 * @returns {{ sha: string; dirty: string[] }}
 */
export function readSourceProvenance(repoRoot) {
  const head = git(repoRoot, ["rev-parse", "HEAD"]);
  if (!head.ok) {
    throw new ReleaseRefusal(
      "source-unreadable",
      `${repoRoot}: cannot read HEAD; a Git checkout is required for provenance`,
    );
  }
  const status = git(repoRoot, ["status", "--porcelain"]);
  if (!status.ok) {
    throw new ReleaseRefusal(
      "source-unreadable",
      `${repoRoot}: git status failed; uncommitted source cannot be ruled out`,
    );
  }
  const dirty = status.stdout
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => line.slice(3));
  return { sha: head.stdout.trim(), dirty };
}

/**
 * declare the same edge more than once (`nexus-knowledge` names
 * `nexus-contracts` as both a normal and a dev dependency), so planning and
 * preparation both read the declarations here instead of the first match.
 *
 * The only form this helper reads is a single-line inline table under the
 * package's own key, with a version requirement. The key may be bare, basic
 * (`"nexus-contracts"`) or literal (`'nexus-contracts'`): all three are the
 * same TOML key and Cargo treats them identically, so every quote form is read
 * and rewritten as the same declaration rather than refused — the edit is the
 * same one-line change, and reading only some spellings would leave the others
 * silently unplanned.
 *
 * Everything else refuses by name, because preparation rewrites bounds in place
 * and a declaration it silently skipped is one it would leave pointed at the
 * old version: a trailing comment, a table spread over several lines, a plain
 * requirement string, a dotted key, and a renamed dependency
 * (`alias = { package = "<name>", … }`), whose key is not the package name and
 * which this scan therefore cannot claim.
 *
 * `requirement` is null for an inline table without a `version` key, which the
 * caller refuses as unversioned.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @param {string} name
 * @returns {{ line: number; requirement: string | null }[]}
 */
function dependencyDeclarations(contents, manifestPath, name) {
  const key = nameForms(name);
  const keyLine = new RegExp(`^\\s*${key}\\s*[.=]`);
  const inline = new RegExp(`^\\s*${key}\\s*=\\s*\\{([^}\\n]*)\\}\\s*$`);
  const renamed = new RegExp(`\\bpackage\\s*=\\s*${key}`);
  /** @type {{ line: number; requirement: string | null }[]} */
  const declarations = [];
  contents.split("\n").forEach((line, index) => {
    if (renamed.test(line)) {
      throw new ReleaseRefusal(
        "dependency-form-unsupported",
        `${manifestPath}:${index + 1}: ${name} is depended on under another key; a renamed dependency has no bound this scan can rewrite`,
      );
    }
    if (!keyLine.test(line)) {
      return;
    }
    const match = inline.exec(line);
    if (match === null) {
      throw new ReleaseRefusal(
        "dependency-form-unsupported",
        `${manifestPath}:${index + 1}: cannot read the ${name} declaration as one inline table with a version requirement`,
      );
    }
    declarations.push({
      line: index + 1,
      requirement: /\bversion\s*=\s*["']([^"']*)["']/.exec(match[1])?.[1] ?? null,
    });
  });
  return declarations;
}

/**
 * inventory package, with the requirement to satisfy: one edge per declaration.
 * The `[dependencies.x]` table form is refused: this helper rewrites bounds in
 * place and must not claim a package it cannot read the same way Cargo does.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @param {string[]} names
 * @returns {{ name: string; requirement: string }[]}
 */
function inventoryDependencies(contents, manifestPath, names) {
  /** @type {{ name: string; requirement: string }[]} */
  const dependencies = [];
  for (const name of names) {
    if (new RegExp(`^\\s*\\[(?:.*\\.)?(?:dev-|build-)?dependencies\\.${nameForms(name)}\\]\\s*$`, "m").test(contents)) {
      throw new ReleaseRefusal(
        "dependency-form-unsupported",
        `${manifestPath}: ${name} uses a dependency table; an inline table with a version requirement is required`,
      );
    }
    for (const declaration of dependencyDeclarations(contents, manifestPath, name)) {
      if (declaration.requirement === null) {
        throw new ReleaseRefusal(
          "unversioned-dependency",
          `${manifestPath}: ${name} has no version requirement, so the package is not registry-ready`,
        );
      }
      // distinct requirement keeps the plan free of duplicate prerequisite rows
      // while a bound that differs is still checked on its own.
      if (
        dependencies.some(
          (dependency) =>
            dependency.name === name &&
            dependency.requirement === declaration.requirement,
        )
      ) {
        continue;
      }
      dependencies.push({ name, requirement: declaration.requirement });
    }
  }
  return dependencies;
}

/**
 * (`"0.1.0"` means `^0.1.0`). Pre-1.0 the left-most non-zero component is
 * pinned: `^0.1.0` accepts `>=0.1.0 <0.2.0`, `^0.0.3` accepts only `0.0.3`.
 *
 * @param {string} requirement
 * @param {string} version
 * @returns {boolean}
 */
function caretAccepts(requirement, version) {
  if (!isReleaseSemVer(requirement) || !isReleaseSemVer(version)) {
    return false;
  }
  if (compareSemVer(version, requirement) < 0) {
    return false;
  }
  const base = requirement.split("-")[0].split(".");
  const candidate = version.split("-")[0].split(".");
  if (base[0] !== "0") {
    return candidate[0] === base[0];
  }
  if (base[1] !== "0") {
    return candidate[0] === "0" && candidate[1] === base[1];
  }
  return candidate.every((part, index) => part === base[index]);
}

/**
 * A consumer crate that inherits the workspace version is not independently
 * releasable, so an inherited literal is a refusal rather than a fallback.
 *
 * @param {string} repoRoot
 * @returns {{ name: string; manifest: string; contents: string; version: string; dependencies: { name: string; requirement: string }[] }[]}
 */
function readInventory(repoRoot) {
  const entries = readVersionBrackets(repoRoot);
  const names = entries.map((entry) => entry.name);
  return entries.map((entry) => {
    const contents = readFileSync(join(repoRoot, entry.manifest), "utf8");
    const declared = parseCargoPackageName(contents, entry.manifest);
    if (declared !== entry.name) {
      throw new ReleaseRefusal(
        "inventory-mismatch",
        `${entry.manifest}: declares ${declared}, carrier entry is ${entry.name}`,
      );
    }
    const version = parseCargoPackageVersion(contents, entry.manifest);
    if (version === null) {
      throw new ReleaseRefusal(
        "version-inherited",
        `${entry.manifest}: inherits the workspace version, so ${entry.name} cannot version independently`,
      );
    }
    return {
      name: entry.name,
      manifest: entry.manifest,
      contents,
      version,
      dependencies: inventoryDependencies(contents, entry.manifest, names),
    };
  });
}

/**
 * consumer, and inventory order breaks ties. A cycle cannot be ordered, so it
 * refuses.
 *
 * @param {{ name: string; dependencies: { name: string; requirement: string }[] }[]} rows
 * @param {Set<string>} selected
 * @returns {string[]}
 */
function dependencyOrder(rows, selected) {
  const byName = new Map(rows.map((row) => [row.name, row]));
  /** @type {Map<string, "visiting" | "done">} */
  const state = new Map();
  /** @type {string[]} */
  const order = [];
  /**
   * @param {string} name
   * @param {string[]} trail
   */
  const visit = (name, trail) => {
    if (state.get(name) === "done") {
      return;
    }
    if (state.get(name) === "visiting") {
      throw new ReleaseRefusal(
        "dependency-cycle",
        `dependency cycle: ${[...trail, name].join(" -> ")}`,
      );
    }
    state.set(name, "visiting");
    for (const dependency of byName.get(name)?.dependencies ?? []) {
      if (selected.has(dependency.name)) {
        visit(dependency.name, [...trail, name]);
      }
    }
    state.set(name, "done");
    order.push(name);
  };
  for (const row of rows) {
    if (selected.has(row.name)) {
      visit(row.name, []);
    }
  }
  return order;
}

/**
 *
 * @param {{ name: string; version: string; checksum?: string }[] | null} published
 * @param {string} name
 * @returns {{ name: string; version: string; checksum?: string }[]}
 */
function publishedRows(published, name) {
  return (published ?? []).filter((row) => row?.name === name);
}

/**
 *
 * @param {{ sourceSha?: string; results?: { name: string; ok: boolean; archive: { sha256: string } | null; package: { version: string } | null }[] } | null} evidence
 * @returns {Map<string, { version: string; archiveSha256: string; ok: boolean }>}
 */
function evidenceRows(evidence) {
  /** @type {Map<string, { version: string; archiveSha256: string; ok: boolean }>} */
  const rows = new Map();
  for (const result of evidence?.results ?? []) {
    if (result.archive === null || result.package === null) {
      continue;
    }
    rows.set(result.name, {
      version: result.package.version,
      archiveSha256: result.archive.sha256,
      ok: result.ok,
    });
  }
  return rows;
}

/**
 * Plan one selected release: allowlist, versions, dependency order, provenance,
 * content matching and the completed/pending split. Performs no publish and no
 * registry query — `registry` is whatever the caller already knows.
 *
 * @param {string} repoRoot
 * @param {{
 *   packages: string[];
 *   sourceSha: string;
 *   registry: { name?: string; published?: { name: string; version: string; checksum?: string }[] | null; writable?: boolean } | string;
 *   evidence?: { sourceSha: string; results: { name: string; ok: boolean; archive: { sha256: string } | null; package: { version: string } | null }[] } | null;
 * }} options
 */
export function planPackageRelease(repoRoot, options) {
  const registry =
    typeof options.registry === "string" ? { name: options.registry } : options.registry ?? {};
  const registryName = registry.name;
  const registryKnown = registry.published !== null && registry.published !== undefined;
  const packages = Array.isArray(options.packages) ? options.packages : [];
  const evidence = options.evidence ?? null;
  /** @type {{ code: string; message: string }[]} */
  const refusals = [];
  /** @param {string} code @param {string} message */
  const refuse = (code, message) => {
    refusals.push({ code, message });
  };

  if (!REGISTRY_NAMES.has(registryName)) {
    refuse(
      "unknown-registry",
      `registry "${registryName}" is not one of: ${[...REGISTRY_NAMES].join(", ")}`,
    );
  }
  if (!SOURCE_SHA_PATTERN.test(options.sourceSha ?? "")) {
    refuse("invalid-source", `source "${options.sourceSha}" is not a full 40-hex commit sha`);
  }
  if (packages.length === 0) {
    refuse("empty-selection", "no packages selected");
  }

  let rows = [];
  try {
    rows = readInventory(repoRoot);
  } catch (error) {
    refuse(error.code ?? "inventory-unreadable", error.message);
  }
  const byName = new Map(rows.map((row) => [row.name, row]));
  const selected = new Set();
  for (const name of packages) {
    if (selected.has(name)) {
      refuse("duplicate-selection", `${name} is selected twice`);
      continue;
    }
    if (!byName.has(name)) {
      refuse(
        "unknown-selection",
        `${name} is not in the ${rows.length}-entry package inventory`,
      );
      continue;
    }
    selected.add(name);
  }

  if (evidence !== null) {
    const packed = evidence.source;
    if (typeof packed?.sha !== "string" || packed.sha !== options.sourceSha) {
      refuse(
        "source-mismatch",
        `archive evidence was packed from ${packed?.sha ?? "an unreadable revision"}, not ${options.sourceSha}`,
      );
    }
    if (packed?.clean !== true) {
      refuse(
        "unclean-evidence",
        "archive evidence was packed from a working tree with uncommitted changes, so its hashes are not the reviewed revision's bytes",
      );
    }
    if (!Array.isArray(evidence.results)) {
      refuse("evidence-unreadable", "archive evidence carries no results array");
    }
  }

  let provenance = { sha: "", dirty: /** @type {string[]} */ ([]) };
  try {
    provenance = readSourceProvenance(repoRoot);
  } catch (error) {
    refuse(error.code ?? "source-unreadable", error.message);
  }
  if (provenance.sha.length > 0 && provenance.sha !== options.sourceSha) {
    refuse(
      "unclean-source",
      `--source ${options.sourceSha} is not HEAD ${provenance.sha}; plan the reviewed revision itself`,
    );
  }
  if (provenance.dirty.length > 0) {
    refuse(
      "unclean-provenance",
      `${provenance.dirty.length} uncommitted change(s) in the packed source tree: ${provenance.dirty
        .slice(0, 5)
        .join(", ")}${provenance.dirty.length > 5 ? ", …" : ""}`,
    );
  }

  let order = [];
  try {
    order = dependencyOrder(rows, selected);
  } catch (error) {
    refuse(error.code ?? "order-unreadable", error.message);
  }

  const archives = evidenceRows(evidence);
  /** @type {{ name: string; manifest: string; version: string; tag: string; archiveSha256: string | null; status: "completed" | "pending"; reason: string; prerequisites: { name: string; requirement: string; resolved: string }[] }[]} */
  const planned = [];
  for (const name of order) {
    const row = byName.get(name);
    if (row === undefined) {
      continue;
    }
    const archive = archives.get(name) ?? null;
    if (archive !== null && !archive.ok) {
      refuse("provenance-unverified", `${name}: the archive or its consumer proof failed`);
    }
    if (archive !== null && archive.version !== row.version) {
      refuse(
        "version-mismatch",
        `${name}: archive evidence is ${archive.version}, manifest is ${row.version}`,
      );
    }
    const rowsPublished = publishedRows(registry.published ?? null, name);
    for (const published of rowsPublished) {
      if (isReleaseSemVer(published.version) && compareSemVer(published.version, row.version) > 0) {
        refuse(
          "stale-version",
          `${name}: registry already carries ${published.version}, which is greater than ${row.version}`,
        );
      }
    }
    const sameVersion = rowsPublished.find((published) => published.version === row.version);
    let status = /** @type {"completed" | "pending"} */ ("pending");
    let reason = registryKnown
      ? "not published at the selected registry"
      : "registry state unknown; content not matched";
    if (sameVersion !== undefined) {
      if (archive === null) {
        refuse(
          "content-unverified",
          `${name}@${row.version} is already published; without archive evidence its content cannot be matched, so it must not be re-uploaded or skipped`,
        );
        reason = "published, content unmatched";
      } else if (sameVersion.checksum !== archive.archiveSha256) {
        refuse(
          "content-mismatch",
          `${name}@${row.version} is published as ${sameVersion.checksum ?? "no checksum"}, review produced ${archive.archiveSha256}`,
        );
        reason = "published with different content";
      } else {
        status = "completed";
        reason = "published with identical content";
      }
    }
    /** @type {{ name: string; requirement: string; resolved: string; version: string | null }[]} */
    const prerequisites = [];
    for (const dependency of row.dependencies) {
      if (!byName.has(dependency.name)) {
        continue;
      }
      const dependencyRow = byName.get(dependency.name);
      let resolved;
      if (selected.has(dependency.name)) {
        // version has to satisfy this consumer's own requirement — otherwise
        // the consumer would be packed against a version it cannot use.
        const satisfies = caretAccepts(dependency.requirement, /** @type {{version: string}} */ (dependencyRow).version);
        resolved = satisfies ? "selected" : "incompatible";
        if (!satisfies) {
          refuse(
            "incompatible-prerequisite",
            `${name}: requires ${dependency.name} ${dependency.requirement}, but the selected ${dependency.name} is ${/** @type {{version: string}} */ (dependencyRow).version}`,
          );
        }
      } else if (registryKnown) {
        const satisfied = publishedRows(registry.published ?? null, dependency.name).some((published) =>
          caretAccepts(dependency.requirement, published.version),
        );
        if (!satisfied) {
          refuse(
            "unmet-prerequisite",
            `${name}: ${dependency.name} ${dependency.requirement} is neither selected nor published`,
          );
        }
        resolved = satisfied ? "published" : "missing";
      } else {
        // prerequisite exists; it stays unresolved and blocks publication.
        resolved = "unverified";
      }
      prerequisites.push({
        name: dependency.name,
        requirement: dependency.requirement,
        resolved,
        version: dependencyRow?.version ?? null,
      });
    }
    planned.push({
      name,
      manifest: row.manifest,
      version: row.version,
      tag: releaseTag(name, row.version),
      archiveSha256: archive?.archiveSha256 ?? null,
      status,
      reason,
      prerequisites,
    });
  }

  const completed = planned.filter((row) => row.status === "completed").map((row) => `${row.name}@${row.version}`);
  const pending = planned.filter((row) => row.status === "pending").map((row) => `${row.name}@${row.version}`);
  if (registry.writable === false && pending.length > 0) {
    refuse(
      "permissions",
      `registry ${registryName} is read-only for this selection; ${pending.length} package(s) still need publication`,
    );
  }

  /**
   * Every prerequisite needs a satisfying selected or known-published version,
   * and every pending row needs the verified archive it would upload.
   *
   * @type {{ code: string; message: string }[]}
   */
  const blockers = [];
  for (const row of planned) {
    for (const prerequisite of row.prerequisites) {
      if (prerequisite.resolved === "unverified" || prerequisite.resolved === "missing") {
        blockers.push({
          code: "prerequisite-unverified",
          message: `${row.name} needs ${prerequisite.name} ${prerequisite.requirement}; registry state is unknown, so it cannot be shown to exist`,
        });
      }
    }
    if (row.status === "pending" && !registryKnown) {
      // with different content, and registry publication is irreversible.
      blockers.push({
        code: "registry-unverified",
        message: `${row.name}@${row.version} is pending and the registry view is unknown, so existing content cannot be matched; pass --registry-state before publishing`,
      });
    }
    if (row.status === "pending" && row.archiveSha256 === null) {
      blockers.push({
        code: "archive-unverified",
        message: `${row.name}@${row.version} has no verified archive in the evidence; run the packageability checker and pass its JSON report`,
      });
    }
  }

  return {
    ok: refusals.length === 0,
    ready: refusals.length === 0 && blockers.length === 0,
    registry: registryName,
    label: PACKAGE_RELEASE_LABEL,
    sourceSha: options.sourceSha,
    source: provenance,
    archiveEvidence: evidence === null ? "absent" : "present",
    order: planned.map((row) => row.name),
    packages: planned,
    completed,
    pending,
    blockers,
    credentials: registry.writable === true ? "write" : "unconfigured",
    refusals,
  };
}

/**
 * Publish the pending rows of an accepted, ready plan, prerequisite first,
 * confirming each upload before its consumer. Registry publication is
 * irreversible: a failure leaves the remaining rows pending for an
 * operator-authorized resume and never withdraws what already landed.
 *
 * Every returned outcome carries the full completed/pending split — including
 * an upload whose confirmation read failed, which stays pending because only a
 * new plan's exact content match may treat it as done.
 *
 * `client` is the only publication seam. This helper never constructs one; the
 * isolated protocol fixture in `release-packages.test.mjs` supplies it.
 *
 * @param {ReturnType<typeof planPackageRelease>} plan
 * @param {{ publish: (entry: { name: string; version: string; tag: string; archiveSha256: string }) => Promise<void>; read: (name: string, version: string) => Promise<{ checksum?: string } | null> }} client
 */
export async function publishSelected(plan, client) {
  const completed = [...plan.completed];
  if (!plan.ok) {
    return {
      ok: false,
      completed,
      pending: [...plan.pending],
      failed: null,
      blockers: plan.blockers,
      refusals: plan.refusals,
    };
  }
  if (!plan.ready) {
    return {
      ok: false,
      completed,
      pending: [...plan.pending],
      failed: null,
      blockers: plan.blockers,
      refusals: [
        {
          code: "not-ready",
          message: `${plan.blockers.length} blocker(s) leave the selection unpublishable; resolve them and re-plan`,
        },
      ],
    };
  }
  const entries = plan.packages.filter((entry) => entry.status === "pending");
  /** @param {number} index */
  const remaining = (index) =>
    entries.slice(index).map((row) => `${row.name}@${row.version}`);
  for (const [index, entry] of entries.entries()) {
    const pair = `${entry.name}@${entry.version}`;
    try {
      await client.publish({
        name: entry.name,
        version: entry.version,
        tag: entry.tag,
        archiveSha256: /** @type {string} */ (entry.archiveSha256),
      });
    } catch (error) {
      return {
        ok: false,
        completed,
        pending: remaining(index),
        failed: pair,
        blockers: [],
        refusals: [
          {
            code: "publish-failed",
            message: `${pair}: ${error instanceof Error ? error.message : String(error)}`,
          },
        ],
      };
    }
    /** @type {{ checksum?: string } | null} */
    let confirmed;
    try {
      confirmed = await client.read(entry.name, entry.version);
    } catch (error) {
      return {
        ok: false,
        completed,
        pending: remaining(index),
        failed: pair,
        blockers: [],
        refusals: [
          {
            code: "upload-unconfirmed",
            message: `${pair}: the confirmation read failed (${
              error instanceof Error ? error.message : String(error)
            }); its upload state is unknown, so re-plan and resume only on an exact content match`,
          },
        ],
      };
    }
    if (confirmed?.checksum !== entry.archiveSha256) {
      return {
        ok: false,
        completed,
        pending: remaining(index),
        failed: pair,
        blockers: [],
        refusals: [
          {
            code: "upload-unconfirmed",
            message: `${pair}: registry reports ${
              confirmed?.checksum ?? "nothing"
            } after upload, expected ${entry.archiveSha256}; re-plan and resume only on an exact content match`,
          },
        ],
      };
    }
    completed.push(pair);
  }
  return { ok: true, completed, pending: [], failed: null, blockers: [], refusals: [] };
}

/**
 * the file untouched.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @param {string} version
 * @returns {string}
 */
function replacePackageVersion(contents, manifestPath, version) {
  const section = /(^|\n)\[package\]\n([\s\S]*?)(?=\n\[|$)/.exec(contents);
  if (section === null) {
    throw new ReleaseRefusal("version-unwritable", `${manifestPath}: missing [package] table`);
  }
  const [, lead, body] = section;
  if (!/^version\s*=\s*"[^"]*"/m.test(body)) {
    throw new ReleaseRefusal(
      "version-unwritable",
      `${manifestPath}: [package].version is not a literal`,
    );
  }
  const replaced = body.replace(/^version\s*=\s*"[^"]*"/m, () => `version = "${version}"`);
  return contents.replace(section[0], () => `${lead}[package]\n${replaced}`);
}

/**
 * A manifest may declare the same edge twice (`nexus-knowledge` names
 * `nexus-contracts` in `[dependencies]` and `[dev-dependencies]`); rewriting
 * one bound leaves the other requiring a version the same local path no longer
 * carries, so every declaration this scan can read is rewritten or the write
 * refuses before touching the file.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @param {string} name
 * @param {string} version
 * @returns {string}
 */
function replaceDependencyRequirement(contents, manifestPath, name, version) {
  const pattern = new RegExp(
    `^(\\s*${nameForms(name)}\\s*=\\s*\\{[^}\\n]*?\\bversion\\s*=\\s*)["'][^"']*["']`,
    "gm",
  );
  const declarations = dependencyDeclarations(contents, manifestPath, name);
  const rewritable = contents.match(pattern) ?? [];
  if (declarations.length === 0 || rewritable.length !== declarations.length) {
    throw new ReleaseRefusal(
      "dependency-form-unsupported",
      `${manifestPath}: cannot rewrite the ${name} requirement in place`,
    );
  }
  return contents.replace(pattern, (_match, prefix) => `${prefix}"${version}"`);
}

/**
 * the convention each package changelog documents.
 *
 * @param {string} contents
 * @param {string} manifestPath
 * @param {string} version
 * @param {string} date
 * @returns {string}
 */
function prepareChangelog(contents, manifestPath, version, date) {
  const heading = "## [Unreleased]";
  if (!contents.includes(heading)) {
    throw new ReleaseRefusal(
      "changelog-unready",
      `${manifestPath}: no "${heading}" section to date for ${version}`,
    );
  }
  return contents.replace(heading, () => `${heading}\n\n## [${version}] - ${date}`);
}

/**
 * Prepare selected package versions: the selected literals, the dependency
 * bounds that would no longer accept them, and the selected changelogs.
 * Nothing else is written — no product version surface, no lockfile, no
 * unrelated package.
 *
 * @param {string} repoRoot
 * @param {{ packages: string[]; versions: Record<string, string>; date?: string }} options
 */
export function preparePackageVersions(repoRoot, options) {
  const date = options.date ?? new Date().toISOString().slice(0, 10);
  const requested = options.versions ?? {};
  let rows = [];
  try {
    rows = readInventory(repoRoot);
  } catch (error) {
    return { ok: false, changed: [], refusals: [{ code: error.code ?? "inventory-unreadable", message: error.message }] };
  }
  const byName = new Map(rows.map((row) => [row.name, row]));
  /** @type {{ code: string; message: string }[]} */
  const refusals = [];
  /** @type {Map<string, string>} */
  const targets = new Map();
  for (const name of options.packages ?? []) {
    const row = byName.get(name);
    if (row === undefined) {
      refusals.push({ code: "unknown-selection", message: `${name} is not in the package inventory` });
      continue;
    }
    const version = requested[name];
    if (version === undefined) {
      refusals.push({ code: "missing-version", message: `${name} has no target version` });
      continue;
    }
    if (!isReleaseSemVer(version)) {
      refusals.push({ code: "invalid-version", message: `${name}: "${version}" is not a release SemVer` });
      continue;
    }
    if (compareSemVer(version, row.version) <= 0) {
      refusals.push({
        code: "version-not-greater",
        message: `${name}: ${version} is not greater than ${row.version}`,
      });
      continue;
    }
    targets.set(name, version);
  }
  if (refusals.length > 0 || targets.size === 0) {
    return { ok: false, changed: [], refusals };
  }

  /** @type {{ path: string; contents: string }[]} */
  const writes = [];
  /**
   * @param {string} path
   * @param {(contents: string) => string} transform
   */
  const patch = (path, transform) => {
    const current = writes.find((write) => write.path === path)?.contents ??
      readFileSync(join(repoRoot, path), "utf8");
    const next = transform(current);
    if (next === current) {
      return;
    }
    const existing = writes.find((write) => write.path === path);
    if (existing === undefined) {
      writes.push({ path, contents: next });
    } else {
      existing.contents = next;
    }
  };

  try {
    for (const [name, version] of targets) {
      const row = /** @type {{ manifest: string }} */ (byName.get(name));
      patch(row.manifest, (contents) => replacePackageVersion(contents, row.manifest, version));
    }
    // tree cannot resolve the bumped local path dependency.
    for (const row of rows) {
      for (const dependency of row.dependencies) {
        const version = targets.get(dependency.name);
        if (version === undefined || caretAccepts(dependency.requirement, version)) {
          continue;
        }
        patch(row.manifest, (contents) =>
          replaceDependencyRequirement(contents, row.manifest, dependency.name, version),
        );
      }
    }
    for (const [name, version] of targets) {
      const row = /** @type {{ manifest: string }} */ (byName.get(name));
      const changelog = join(dirname(row.manifest), "CHANGELOG.md");
      if (!existsSync(join(repoRoot, changelog))) {
        throw new ReleaseRefusal("changelog-missing", `${changelog}: selected package has no changelog`);
      }
      patch(changelog, (contents) => prepareChangelog(contents, changelog, version, date));
    }
  } catch (error) {
    return {
      ok: false,
      changed: [],
      refusals: [{ code: error.code ?? "prepare-failed", message: error.message }],
    };
  }

  for (const write of writes) {
    writeFileSync(join(repoRoot, write.path), write.contents, "utf8");
  }
  return {
    ok: true,
    changed: writes.map((write) => write.path),
    versions: [...targets].map(([name, version]) => ({ name, version })),
    refusals,
  };
}

/**
 * @param {ReturnType<typeof planPackageRelease>} plan
 */
function printPlan(plan) {
  console.log(
    `Package release plan — registry ${plan.registry}, label ${plan.label}, archive evidence ${plan.archiveEvidence}`,
  );
  console.log(
    `source    ${plan.sourceSha} (uncommitted changes: ${plan.source.dirty.length})`,
  );
  console.log(`order     ${plan.order.join(", ")}`);
  for (const row of plan.packages) {
    console.log(
      `  ${row.name.padEnd(22)} ${row.version.padEnd(12)} ${row.tag.padEnd(34)} ${
        row.status
      } (${row.reason})`,
    );
    for (const prerequisite of row.prerequisites) {
      console.log(
        `      needs ${prerequisite.name} ${prerequisite.requirement} — ${prerequisite.resolved}`,
      );
    }
  }
  console.log(`plan      ${plan.ok ? "ok" : "refused"}`);
  console.log(`completed ${plan.completed.length === 0 ? "<none>" : plan.completed.join(", ")}`);
  console.log(`pending   ${plan.pending.length === 0 ? "<none>" : plan.pending.join(", ")}`);
  console.log(
    `ready     ${plan.ready ? "yes" : "no"}${plan.ready ? "" : ` — ${plan.blockers.length} blocker(s)`}`,
  );
  for (const blocker of plan.blockers) {
    console.log(`      ${blocker.code}: ${blocker.message}`);
  }
  console.log(`credentials ${plan.credentials} (publication is an operator action outside this check)`);
  for (const refusal of plan.refusals) {
    console.error(`refused: ${refusal.code}: ${refusal.message}`);
  }
}

/**
 * @param {string[]} argv
 * @returns {{ flags: Map<string, string>; switches: Set<string> } | string}
 */
function parseArgs(argv) {
  /** @type {Map<string, string>} */
  const flags = new Map();
  const switches = new Set();
  const valued = new Set([
    "--source",
    "--packages",
    "--registry",
    "--registry-state",
    "--archive-evidence",
    "--versions",
    "--date",
  ]);
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (
      arg === "--check-only" ||
      arg === "--require-publishable" ||
      arg === "--prepare" ||
      arg === "--help" ||
      arg === "-h"
    ) {
      switches.add(arg);
      continue;
    }
    if (!valued.has(arg)) {
      return `unknown argument: ${arg}`;
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      return `${arg} needs a value`;
    }
    flags.set(arg, value);
    index += 1;
  }
  return { flags, switches };
}

/**
 * rather than a `TypeError` from whatever reads the value next.
 *
 * @param {string} path
 * @returns {unknown}
 */
function readJsonFile(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (error) {
    throw new ReleaseRefusal(
      "input-unusable",
      error instanceof SyntaxError
        ? "is not JSON"
        : error instanceof Error
          ? error.message
          : String(error),
    );
  }
}

/**
 * @param {unknown} value
 * @returns {value is Record<string, unknown>}
 */
function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * shape would otherwise surface as a `TypeError` deep in the plan.
 *
 * @param {unknown} value
 * @returns {{ published: { name: string; version: string; checksum?: string }[] | null; writable?: boolean }}
 */
function readRegistryView(value) {
  if (!isRecord(value)) {
    throw new ReleaseRefusal(
      "registry-state-unusable",
      'expected an object {"published": [...] | null, "writable": boolean}',
    );
  }
  const published = value.published ?? null;
  if (
    published !== null &&
    (!Array.isArray(published) ||
      !published.every(
        (row) =>
          isRecord(row) &&
          typeof row.name === "string" &&
          typeof row.version === "string" &&
          (row.checksum === undefined || typeof row.checksum === "string"),
      ))
  ) {
    throw new ReleaseRefusal(
      "registry-state-unusable",
      '"published" must be null or an array of {name, version, checksum?}',
    );
  }
  if (value.writable !== undefined && typeof value.writable !== "boolean") {
    throw new ReleaseRefusal(
      "registry-state-unusable",
      '"writable" must be a boolean when present',
    );
  }
  return /** @type {{ published: { name: string; version: string; checksum?: string }[] | null; writable?: boolean }} */ (
    { published, writable: value.writable }
  );
}

/**
 * Only the fields the plan consumes are required; extra checker fields pass
 * through untouched.
 *
 * @param {unknown} value
 * @returns {{ source: { sha: string; clean: boolean }; results: { name: string; ok: boolean; archive: { sha256: string } | null; package: { version: string } | null }[] }}
 */
function readArchiveEvidence(value) {
  if (
    !isRecord(value) ||
    !isRecord(value.source) ||
    typeof value.source.sha !== "string" ||
    typeof value.source.clean !== "boolean" ||
    !Array.isArray(value.results)
  ) {
    throw new ReleaseRefusal(
      "evidence-unreadable",
      'expected checker output {"source": {"sha", "clean"}, "results": [...]}',
    );
  }
  for (const row of value.results) {
    if (
      !isRecord(row) ||
      typeof row.name !== "string" ||
      typeof row.ok !== "boolean" ||
      (row.archive !== null && typeof row.archive?.sha256 !== "string") ||
      (row.package !== null && typeof row.package?.version !== "string")
    ) {
      throw new ReleaseRefusal(
        "evidence-unreadable",
        "every result needs {name, ok, archive: {sha256} | null, package: {version} | null}",
      );
    }
  }
  return /** @type {ReturnType<typeof readArchiveEvidence>} */ (value);
}

/**
 * @param {string[]} argv
 * @returns {number}
 */
function main(argv) {
  const parsed = parseArgs(argv);
  if (typeof parsed === "string") {
    console.error(parsed);
    return 2;
  }
  const { flags, switches } = parsed;
  if (switches.has("--help") || switches.has("-h")) {
    console.log(
      [
        "Usage:",
        "  node tooling/release/release-packages.mjs --check-only [--require-publishable] \\",
        "    --packages <a,b> --registry crates-io \\",
        "    [--registry-state <json>] [--archive-evidence <json>]",
        "  node tooling/release/release-packages.mjs --prepare --packages <a,b> \\",
        "    --versions <a=1.2.3,b=1.2.4> [--date YYYY-MM-DD]",
        "",
        "  --registry-state    {\"published\": [{name, version, checksum}], \"writable\": true}",
        "                      a snapshot the operator must capture fresh; never verified",
        "  --archive-evidence  `check-contract-packageability.mjs --json` output",
        "",
        "Planning reads manifests, the version carrier, Git provenance and the",
        "given JSON files only: no credential, no network, no live registry query.",
        "",
        "Exit 0 for a valid plan; --require-publishable additionally demands a",
        "ready plan. A green check-only run never means publication was granted.",
      ].join("\n"),
    );
    return 0;
  }
  const checkOnly = switches.has("--check-only");
  const prepare = switches.has("--prepare");
  if (checkOnly === prepare) {
    console.error("exactly one of --check-only or --prepare is required");
    return 2;
  }
  if (prepare && switches.has("--require-publishable")) {
    console.error("--require-publishable belongs to --check-only");
    return 2;
  }
  const repoRoot = resolveRepoRoot();
  const packages = (flags.get("--packages") ?? "").split(",").filter((name) => name.length > 0);

  if (prepare) {
    /** @type {Record<string, string>} */
    const versions = {};
    for (const pair of (flags.get("--versions") ?? "").split(",").filter((entry) => entry.length > 0)) {
      const [name, version] = pair.split("=");
      if (name === undefined || version === undefined) {
        console.error(`--versions entry "${pair}" is not <name>=<version>`);
        return 2;
      }
      versions[name] = version;
    }
    const result = preparePackageVersions(repoRoot, {
      packages,
      versions,
      date: flags.get("--date"),
    });
    if (!result.ok) {
      for (const refusal of result.refusals) {
        console.error(`refused: ${refusal.code}: ${refusal.message}`);
      }
      return 1;
    }
    console.log(`Prepared ${result.changed.length} file(s):`);
    for (const path of result.changed) {
      console.log(`  ${path}`);
    }
    return 0;
  }

  const registryName = flags.get("--registry");
  if (registryName === undefined) {
    console.error("--registry is required");
    return 2;
  }
  const sourceSha = flags.get("--source");
  if (sourceSha === undefined) {
    console.error("--source is required (the reviewed full commit sha)");
    return 2;
  }
  /** @type {{ name: string; published?: { name: string; version: string; checksum?: string }[] | null; writable?: boolean }} */
  const registry = { name: registryName };
  const statePath = flags.get("--registry-state");
  if (statePath !== undefined) {
    try {
      const view = readRegistryView(readJsonFile(statePath));
      registry.published = view.published;
      if (view.writable !== undefined) {
        registry.writable = view.writable;
      }
    } catch (error) {
      console.error(
        `refused: ${error.code ?? "input-unusable"}: --registry-state ${statePath}: ${error.message}`,
      );
      return 2;
    }
  }
  /** @type {{ source: { sha: string; clean: boolean }; results: unknown[] } | null} */
  let evidence = null;
  const evidencePath = flags.get("--archive-evidence");
  if (evidencePath !== undefined) {
    try {
      evidence = readArchiveEvidence(readJsonFile(evidencePath));
    } catch (error) {
      console.error(
        `refused: ${error.code ?? "input-unusable"}: --archive-evidence ${evidencePath}: ${error.message}`,
      );
      return 2;
    }
  }
  const plan = planPackageRelease(repoRoot, { packages, sourceSha, registry, evidence });
  printPlan(plan);
  // what is still missing, while `--require-publishable` is the gate a publish
  // path must pass. Neither one grants publication by itself.
  return plan.ok && (!switches.has("--require-publishable") || plan.ready) ? 0 : 1;
}

const invokedDirectly =
  process.argv[1] !== undefined && process.argv[1] === fileURLToPath(import.meta.url);

if (invokedDirectly) {
  process.exit(main(process.argv.slice(2)));
}
