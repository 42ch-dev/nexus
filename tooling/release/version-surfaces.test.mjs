import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join, posix } from "node:path";
import {
  CARGO_LOCK_PATH,
  ELECTRON_PACKAGE_PATH,
  LOCKSTEP_PATHS,
  NATIVE_LOADER_PACKAGE_PATH,
  NATIVE_SURFACE_PATHS,
  PRODUCT_JSON_PATH,
  ROOT_PACKAGE_PATH,
  VERSION_BRACKETS_PATH,
  parseCargoLockPackageVersion,
  parseCargoPackageName,
  parseCargoPackageVersion,
  parseCargoWorkspaceMembers,
  parseJsonVersion,
  parseWorkspacePackageVersion,
  readSurfaceVersion,
  readSurfaceVersions,
  readStandalonePublishableCrates,
  readVersionBrackets,
  readWorkspaceMemberPackages,
  replaceJsonVersion,
  replaceWorkspacePackageVersion,
  writeReleaseVersion,
} from "./version-surfaces.mjs";
import {
  createTempRepo,
  cleanupTempRepo,
  expectedMemberLockVersions,
  FIXTURE_BASELINE_VERSION,
  readCargoPackageRows,
  readRepoFile,
  REPO_ROOT,
  writeRepoFile,
} from "./test-harness.mjs";

const currentVersion = JSON.parse(readRepoFile(REPO_ROOT, ROOT_PACKAGE_PATH)).version;

test("every declared surface is equal in the real repository", () => {
  const surfaces = readSurfaceVersions(REPO_ROOT);
  assert.deepEqual(
    surfaces.map((surface) => surface.path),
    [...LOCKSTEP_PATHS, ...NATIVE_SURFACE_PATHS],
  );
  for (const surface of surfaces) {
    assert.equal(surface.version, currentVersion, surface.path);
  }
  assert.equal(readSurfaceVersion(REPO_ROOT, PRODUCT_JSON_PATH), currentVersion);
});

test("native surfaces carry the version and the loader pins stay version-stable", () => {
  const loaderPath = NATIVE_LOADER_PACKAGE_PATH;
  const updated = replaceJsonVersion(
    readRepoFile(REPO_ROOT, loaderPath),
    "9.9.9",
    loaderPath,
  );
  const data = JSON.parse(updated);
  assert.equal(data.version, "9.9.9");
  const pins = Object.keys(data.optionalDependencies).filter((name) =>
    name.startsWith("@42ch/nexus-native"),
  );
  assert.equal(pins.length, 4);
  for (const name of pins) {
    assert.equal(data.optionalDependencies[name], "workspace:*", name);
  }

  const platformPath = "packages/nexus-native-darwin-arm64/package.json";
  const platform = JSON.parse(
    replaceJsonVersion(readRepoFile(REPO_ROOT, platformPath), "9.9.9", platformPath),
  );
  assert.equal(platform.version, "9.9.9");
});

test("parseWorkspacePackageVersion reads [workspace.package] only", () => {
  const contents = [
    "[package]",
    'name = "other"',
    'version = "9.9.9"',
    "",
    "[workspace.package]",
    'version = "0.1.0"',
    'edition = "2021"',
    "",
    "[workspace.dependencies]",
    'serde = "1"',
    "",
  ].join("\n");
  assert.equal(parseWorkspacePackageVersion(contents), "0.1.0");
  assert.throws(
    () => parseWorkspacePackageVersion("[package]\nname = \"x\"\n"),
    /missing \[workspace\.package\] section/,
  );
});

test("replaceWorkspacePackageVersion rewrites only the workspace version line", () => {
  const original = readRepoFile(REPO_ROOT, "Cargo.toml");
  const updated = replaceWorkspacePackageVersion(original, "9.9.9");
  assert.equal(
    updated,
    original.replace(/(\[workspace\.package\][\s\S]*?)version = "[^"]*"/, '$1version = "9.9.9"'),
  );
  assert.equal(parseWorkspacePackageVersion(updated), "9.9.9");
});

test("replaceJsonVersion changes one line and keeps canonical formatting", () => {
  for (const path of [ROOT_PACKAGE_PATH, ELECTRON_PACKAGE_PATH, PRODUCT_JSON_PATH]) {
    const original = readRepoFile(REPO_ROOT, path);
    const updated = replaceJsonVersion(original, "9.9.9", path);
    assert.equal(updated, original.replace(/"version": "[^"]*"/, '"version": "9.9.9"'), path);
    assert.equal(parseJsonVersion(updated, path), "9.9.9");
    assert.ok(updated.endsWith("\n"), path);
  }
});

test("replaceJsonVersion refuses non-canonical JSON instead of reformatting", () => {
  assert.throws(
    () => replaceJsonVersion('{"version":"0.1.0"}', "0.2.0", "package.json"),
    /not canonical 2-space JSON/,
  );
});

test("parseJsonVersion rejects files without a version string", () => {
  assert.throws(() => parseJsonVersion("{}", "package.json"), /missing "version"/);
  assert.throws(() => parseJsonVersion("{", "package.json"), /invalid JSON/);
});

test("workspace members parse from the real root manifest", () => {
  const members = parseCargoWorkspaceMembers(readRepoFile(REPO_ROOT, "Cargo.toml"));
  assert.ok(members.length > 20, `expected many members, got ${members.length}`);
  assert.ok(members.includes("apps/nexus42"));
  for (const member of members) {
    assert.ok(!/[*?[\]]/.test(member), member);
  }
});

test("package names parse and members resolve to lock entries", () => {
  assert.equal(
    parseCargoPackageName("[package]\nname = \"nexus-core\"\nversion.workspace = true\n"),
    "nexus-core",
  );
  assert.throws(() => parseCargoPackageName("[dependencies]\nfoo = \"1\"\n"), /missing \[package\]\.name/);

  // Two brackets: a contract crate's row equals its own manifest literal, every
  // other member's row equals the product version.
  const contractNames = readVersionBrackets(REPO_ROOT).map((entry) => entry.name);
  const expected = expectedMemberLockVersions(REPO_ROOT, currentVersion);
  assert.ok(
    contractNames.some((name) => expected.has(name)),
    "no contract crate is a workspace member, so the split row would never run",
  );
  const lock = readRepoFile(REPO_ROOT, CARGO_LOCK_PATH);
  for (const [name, version] of expected) {
    assert.equal(parseCargoLockPackageVersion(lock, name), version, name);
  }
});

test("writeReleaseVersion regenerates Cargo.lock through Cargo", () => {
  const dir = createTempRepo();
  try {
    const lockBefore = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8");
    const changed = writeReleaseVersion(dir, "9.9.9");
    assert.deepEqual(
      changed.map((entry) => entry.path),
      [...LOCKSTEP_PATHS, ...NATIVE_SURFACE_PATHS, CARGO_LOCK_PATH],
    );
    assert.equal(changed.at(-1).members, readWorkspaceMemberPackages(dir).length);
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, "9.9.9", surface.path);
    }

    const lock = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8");
    // Non-contract members followed the product bump; contract crates kept the
    // literal their manifest still declares.
    const expected = expectedMemberLockVersions(dir, "9.9.9");
    assert.ok(
      [...expected.values()].some((version) => version !== "9.9.9"),
      "fixture needs a contract crate that is a workspace member",
    );
    for (const [name, version] of expected) {
      assert.equal(parseCargoLockPackageVersion(lock, name), version, name);
    }
    assert.equal(
      parseCargoLockPackageVersion(lock, "nexus-contracts"),
      FIXTURE_BASELINE_VERSION,
    );
    // Non-member packages keep their versions: `nexus-module-manifest` is a
    // standalone (excluded) crate, `leb128fmt` an unrelated dependency.
    for (const name of ["nexus-module-manifest", "leb128fmt"]) {
      assert.equal(
        parseCargoLockPackageVersion(lock, name),
        parseCargoLockPackageVersion(lockBefore, name),
        name,
      );
    }
    assert.equal(lock.split("\n").length, lockBefore.split("\n").length);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("writeReleaseVersion refuses a lockfile missing a workspace member entry", () => {
  const dir = createTempRepo();
  try {
    const lock = readFileSync(join(dir, CARGO_LOCK_PATH), "utf8").replace(
      /\[\[package\]\]\nname = "nexus42"\nversion = "[^"]+"\n/,
      "",
    );
    writeFileSync(join(dir, CARGO_LOCK_PATH), lock, "utf8");

    assert.throws(
      () => writeReleaseVersion(dir, "9.9.9"),
      /missing \[\[package\]\] entry for nexus42/,
    );
    // All-or-nothing: the refused call wrote nothing.
    for (const surface of readSurfaceVersions(dir)) {
      assert.equal(surface.version, FIXTURE_BASELINE_VERSION, surface.path);
    }
    assert.equal(readFileSync(join(dir, CARGO_LOCK_PATH), "utf8"), lock);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("parseCargoPackageVersion separates a literal version from workspace inheritance", () => {
  assert.equal(
    parseCargoPackageVersion('[package]\nname = "a"\nversion = "9.9.9"\n'),
    "9.9.9",
  );
  assert.equal(
    parseCargoPackageVersion("[package]\nname = \"a\"\nversion.workspace = true\n"),
    null,
  );
  assert.throws(
    () => parseCargoPackageVersion('[package]\nname = "a"\n', "crates/a/Cargo.toml"),
    /missing version = "\.\.\." or version\.workspace = true/,
  );
});

test("the version brackets carrier names manifests whose package names agree", () => {
  const entries = readVersionBrackets(REPO_ROOT);
  assert.ok(entries.length > 0, "carrier is empty");
  assert.equal(new Set(entries.map((entry) => entry.name)).size, entries.length);
  for (const entry of entries) {
    const contents = readRepoFile(REPO_ROOT, entry.manifest);
    assert.equal(parseCargoPackageName(contents, entry.manifest), entry.name);
    // Contract crates version independently: a literal, never inheritance.
    assert.equal(typeof parseCargoPackageVersion(contents, entry.manifest), "string", entry.name);
  }
});

/**
 * The eight approved public packages, in carrier order. Pinned here on purpose:
 * publishing one more crate is a new consumer commitment, so it needs a
 * deliberate edit of this list — not just another carrier line.
 */
const PUBLIC_MANIFEST_PATHS = [
  "crates/nexus-contracts/Cargo.toml",
  "crates/nexus-workflow-ir/Cargo.toml",
  "crates/nexus-pattern/Cargo.toml",
  "crates/nexus-knowledge/Cargo.toml",
  "crates/nexus-narrative/Cargo.toml",
  "modules/nexus-module-sdk/Cargo.toml",
  "modules/nexus-module-manifest/Cargo.toml",
  "modules/nexus-module-test/Cargo.toml",
];

/** Approved public normal/build closure: repository edges only. */
const PUBLIC_LOCAL_CLOSURE = {
  "nexus-contracts": [],
  "nexus-workflow-ir": ["nexus-contracts"],
  "nexus-pattern": ["nexus-contracts", "nexus-workflow-ir"],
  "nexus-knowledge": ["nexus-contracts"],
  "nexus-narrative": ["nexus-contracts", "nexus-knowledge"],
  "nexus-module-sdk": [],
  "nexus-module-manifest": [],
  "nexus-module-test": ["nexus-module-sdk"],
};

test("the carrier is exactly the eight approved public manifests", () => {
  const entries = readVersionBrackets(REPO_ROOT);
  assert.deepEqual(
    entries.map((entry) => entry.manifest).sort(),
    [...PUBLIC_MANIFEST_PATHS].sort(),
    "the public set is exactly the eight approved packages",
  );
  for (const entry of entries) {
    assert.equal(
      typeof parseCargoPackageVersion(readRepoFile(REPO_ROOT, entry.manifest), entry.manifest),
      "string",
      `${entry.name}: a public package declares its own literal version`,
    );
  }
});

test("every package outside the eight public manifests is publish = false", () => {
  const rows = readCargoPackageRows(REPO_ROOT);
  const publicManifests = new Set(PUBLIC_MANIFEST_PATHS);
  const privateRows = rows.filter((row) => !publicManifests.has(row.manifest));
  assert.ok(privateRows.length >= 20, `expected the private inventory, got ${privateRows.length}`);
  for (const row of privateRows) {
    assert.deepEqual(
      row.publish,
      [],
      `${row.manifest}: ${row.name} is not one of the eight public packages, so it must declare publish = false`,
    );
  }
  const publicRows = rows.filter((row) => publicManifests.has(row.manifest));
  assert.equal(publicRows.length, 8, "every approved public manifest must be a Cargo package");
  for (const row of publicRows) {
    assert.notDeepEqual(row.publish, [], `${row.name} is public and must stay publishable`);
  }
});

test("the public normal/build closure is exactly the approved local edge set", () => {
  const byName = new Map(readCargoPackageRows(REPO_ROOT).map((row) => [row.name, row]));
  for (const [name, expected] of Object.entries(PUBLIC_LOCAL_CLOSURE)) {
    const row = byName.get(name);
    assert.ok(row, `${name}: missing from the Cargo package rows`);
    const edges = row.dependencies
      .filter((dep) => dep.path !== null && dep.kind !== "dev")
      .map((dep) => dep.name)
      .sort();
    assert.deepEqual(edges, [...expected].sort(), `${name} normal/build edges`);
  }
});

test("every public-to-public local edge carries a caret requirement", () => {
  const rows = readCargoPackageRows(REPO_ROOT);
  const publicManifests = new Set(PUBLIC_MANIFEST_PATHS);
  const publicNames = new Set(
    rows.filter((row) => publicManifests.has(row.manifest)).map((row) => row.name),
  );
  let checked = 0;
  for (const row of rows.filter((row) => publicNames.has(row.name))) {
    for (const dep of row.dependencies) {
      // A dev-only edge to a private package is a development declaration that
      // Cargo omits from a published manifest; its integration target moves to
      // the private crate instead of inventing a version for it here.
      if (dep.path === null || !publicNames.has(dep.name)) {
        continue;
      }
      checked += 1;
      assert.notEqual(
        dep.req,
        "*",
        `${row.name} -> ${dep.name} (${dep.kind ?? "normal"}): needs a registry version requirement`,
      );
      assert.ok(
        dep.req.startsWith("^"),
        `${row.name} -> ${dep.name}: ${dep.req} is not a caret requirement`,
      );
    }
  }
  // IR→contracts, pattern→contracts+IR, knowledge→contracts (normal and dev),
  // narrative→contracts+knowledge, module-test→SDK.
  assert.equal(checked, 8, "every retained public local edge was checked");
});

/**
 * The contract-entry identity guard compares a carrier's POSIX manifest path
 * with the member's, so member paths must be POSIX too — the platform `join`
 * here made every run on win32 throw instead.
 */
test("workspace member manifest paths use the carrier's POSIX convention", () => {
  const members = readWorkspaceMemberPackages(REPO_ROOT);
  assert.ok(members.length > 0, "workspace has no members");
  for (const member of members) {
    assert.equal(
      member.manifestPath,
      posix.normalize(member.manifestPath),
      `${member.name}: member manifest paths must be repo-relative POSIX`,
    );
  }
});

/**
 * Rewrite the fixture carrier through `mutate`, then require the loader to
 * reject it.
 *
 * @param {(carrier: { contract: { name: string; manifest: string }[] }) => void} mutate
 * @param {RegExp} pattern
 */
function assertCarrierRejected(mutate, pattern) {
  const dir = createTempRepo();
  try {
    const carrier = JSON.parse(readRepoFile(dir, VERSION_BRACKETS_PATH));
    mutate(carrier);
    writeRepoFile(dir, VERSION_BRACKETS_PATH, `${JSON.stringify(carrier, null, 2)}\n`);
    assert.throws(() => readVersionBrackets(dir), pattern);
  } finally {
    cleanupTempRepo(dir);
  }
}

test("readVersionBrackets rejects a duplicate contract entry", () => {
  const [first] = readVersionBrackets(REPO_ROOT);
  assertCarrierRejected(
    (carrier) => carrier.contract.push({ ...first }),
    new RegExp(`duplicate contract entry ${first.name}`),
  );
});

test("readVersionBrackets rejects a contract entry with a missing manifest", () => {
  assertCarrierRejected(
    (carrier) => carrier.contract.push({ name: "nexus-absent", manifest: "crates/nexus-absent/Cargo.toml" }),
    /contract entry nexus-absent points at a missing manifest crates\/nexus-absent\/Cargo\.toml/,
  );
});

test("readVersionBrackets rejects a name that disagrees with its manifest", () => {
  assertCarrierRejected(
    (carrier) => {
      carrier.contract[0].name = "nexus-renamed";
    },
    /contract entry nexus-renamed does not match package name .* in crates\/nexus-contracts\/Cargo\.toml/,
  );
});

test("readVersionBrackets rejects an empty contract array instead of exempting nothing", () => {
  assertCarrierRejected((carrier) => {
    carrier.contract = [];
  }, /empty "contract" array/);
});

test("readVersionBrackets rejects a carrier path that is not the member's manifest", () => {
  const dir = createTempRepo();
  try {
    // Same package name as a workspace member, different file: it would satisfy
    // the name check and let the real member go unchecked.
    writeRepoFile(
      dir,
      "shadow/Cargo.toml",
      '[package]\nname = "nexus-workflow-ir"\nversion = "0.1.0"\n',
    );
    const carrier = JSON.parse(readRepoFile(dir, VERSION_BRACKETS_PATH));
    const entry = carrier.contract.find((row) => row.name === "nexus-workflow-ir");
    entry.manifest = "shadow/Cargo.toml";
    writeRepoFile(dir, VERSION_BRACKETS_PATH, `${JSON.stringify(carrier, null, 2)}\n`);

    assert.throws(
      () => readVersionBrackets(dir),
      /contract entry nexus-workflow-ir points at shadow\/Cargo\.toml, but workspace member nexus-workflow-ir is crates\/nexus-workflow-ir\/Cargo\.toml; the entry must name the member's own manifest/,
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

/** Standalone crate added to a fixture: outside the root workspace, own root. */
const FIXTURE_STANDALONE_MANIFEST = "modules/nexus-demo/Cargo.toml";

/**
 * Add a standalone crate to a fixture — its own `[workspace]` table, its own
 * target stub — so Cargo parses it as a workspace root of its own.
 *
 * @param {string} dir
 * @param {string} publishLine `publish = false` or `""` (Cargo's publishable default).
 * @returns {void}
 */
function writeFixtureStandaloneCrate(dir, publishLine) {
  writeRepoFile(
    dir,
    FIXTURE_STANDALONE_MANIFEST,
    `[package]\nname = "nexus-demo"\nversion = "0.1.0"\nedition = "2021"\n${publishLine}\n[workspace]\n`,
  );
  writeRepoFile(dir, "modules/nexus-demo/src/lib.rs", "// release-fixture stub\n");
}

test("readStandalonePublishableCrates skips a crate declared publish = false", () => {
  const dir = createTempRepo();
  try {
    writeFixtureStandaloneCrate(dir, "publish = false");
    const crates = readStandalonePublishableCrates(dir);
    assert.ok(
      !crates.some((crate) => crate.manifest === FIXTURE_STANDALONE_MANIFEST),
      "a publish = false crate cannot version independently, so it needs no carrier entry",
    );
    // The scan still classifies the real declarations, so the result above is
    // not a broken read.
    assert.ok(
      crates.some((crate) => crate.manifest === "modules/nexus-module-sdk/Cargo.toml"),
      "real publishable crate missing from the read",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("readStandalonePublishableCrates requires a crate that omits publish", () => {
  const dir = createTempRepo();
  try {
    // Cargo's default is publishable, so omitting the key does not opt out.
    writeFixtureStandaloneCrate(dir, "");
    assert.ok(
      readStandalonePublishableCrates(dir).some(
        (crate) => crate.manifest === FIXTURE_STANDALONE_MANIFEST,
      ),
      "a crate Cargo would publish belongs in the contract bracket",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

test("readStandalonePublishableCrates finds crates outside the workspace root", () => {
  const dir = createTempRepo();
  try {
    const manifests = readStandalonePublishableCrates(dir).map((crate) => crate.manifest);
    for (const manifest of [
      "modules/nexus-module-sdk/Cargo.toml",
      "modules/nexus-module-manifest/Cargo.toml",
      "modules/nexus-module-test/Cargo.toml",
    ]) {
      assert.ok(manifests.includes(manifest), `${manifest} missing from the read`);
    }
    // The root workspace is a separate root: its members are not standalone.
    assert.ok(
      !manifests.some((manifest) => manifest.startsWith("crates/")),
      "a workspace member was classified as a standalone crate",
    );
  } finally {
    cleanupTempRepo(dir);
  }
});

