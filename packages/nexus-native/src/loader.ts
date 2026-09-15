import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { NativeCompatibility } from '@42ch/nexus-contracts';

const require = createRequire(import.meta.url);
const __dirname = dirname(fileURLToPath(import.meta.url));

const PLACEHOLDER_HASH = '0'.repeat(64);

/** Fixed N-API contract value declared by `native-compatibility.schema.json`. */
export const REQUIRED_NAPI_MINIMUM = 8;

export interface ProviderCallbacksNative {
  call(requestJson: string): Promise<string>;
  next(requestJson: string): Promise<string>;
}

export interface NativeCoreBinding {
  activePrincipal(): Promise<string>;
  worldKbGraph(principal: string, worldId: string, includeSuggested: boolean): Promise<Uint8Array>;
  patchWorldKbEntity(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  worldKbCandidates(
    principal: string,
    worldId: string,
    limit?: number | null,
    cursor?: string | null,
  ): Promise<Uint8Array>;
  hostQuery(requestJson: Uint8Array): Promise<Uint8Array>;
  changes(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  providerCall(requestJson: Uint8Array): Promise<Uint8Array>;
  nextProviderEvents(operationId: string, maxEvents: number, maxBytes: number): Promise<Uint8Array>;
  close(): Promise<Uint8Array>;
  // ── P5-T1 World / Work / content / knowledge family surface ──────────────
  // Raw napi signatures; the typed facade lives in `index.ts`.
  narrativeListWorlds(principal: string): Promise<Uint8Array>;
  narrativeGetWorld(principal: string, worldId: string): Promise<Uint8Array>;
  createWorld(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  deleteWorld(principal: string, worldId: string): Promise<Uint8Array>;
  promoteWorldKbCandidate(
    principal: string,
    worldId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  patchWorldKbRelationship(
    principal: string,
    worldId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  worldKbKeyBlockState(
    principal: string,
    worldId: string,
    keyBlockId: string,
  ): Promise<Uint8Array>;
  createWorldFork(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  exportWorldPack(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  importWorldPack(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listWorldRules(principal: string, worldId: string): Promise<Uint8Array>;
  createWorldRule(principal: string, worldId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  updateWorldRule(
    principal: string,
    worldId: string,
    ruleId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listWorldFindings(principal: string, worldId: string): Promise<Uint8Array>;
  timelineOverview(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  listTimelineEvents(
    principal: string,
    worldId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  listWorks(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  getWork(principal: string, workId: string): Promise<Uint8Array>;
  createWork(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  patchWork(principal: string, workId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  deleteWork(principal: string, workId: string): Promise<Uint8Array>;
  appendWorkInspiration(
    principal: string,
    workId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  setWorkPoolActive(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  releaseWorkCompletionLock(
    principal: string,
    workId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  reconcileWorkChapters(
    principal: string,
    workId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  selectWork(principal: string, workId: string): Promise<Uint8Array>;
  listWorkPool(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  promoteWorkPoolEntry(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  archiveWorkPoolEntry(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  addWorkInspiration(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listWorkInspiration(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  promoteWorkInspiration(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  archiveWorkInspiration(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listChapters(principal: string, workId: string, queryJson: Uint8Array): Promise<Uint8Array>;
  chapterDetail(
    principal: string,
    workId: string,
    chapterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  chapterOutline(
    principal: string,
    workId: string,
    chapterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  chapterBody(
    principal: string,
    workId: string,
    chapterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  patchChapter(
    principal: string,
    workId: string,
    chapterId: string,
    queryJson: Uint8Array,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  getWorkOutline(principal: string, workId: string): Promise<Uint8Array>;
  patchOutlineStructure(principal: string, workId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  patchOutlineChapter(
    principal: string,
    workId: string,
    chapterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  patchTimelineEvent(principal: string, workId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listKbEntries(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  addKbEntry(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  getKbEntry(principal: string, entryId: string): Promise<Uint8Array>;
  deleteKbEntry(principal: string, entryId: string): Promise<Uint8Array>;
  createFinding(principal: string, workId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  createFindingFromReview(
    principal: string,
    workId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listFindings(principal: string, workId: string, queryJson: Uint8Array): Promise<Uint8Array>;
  getWorkFinding(principal: string, workId: string, findingId: string): Promise<Uint8Array>;
  getFinding(principal: string, findingId: string): Promise<Uint8Array>;
  updateFinding(principal: string, findingId: string, requestJson: Uint8Array): Promise<Uint8Array>;
  deleteFinding(principal: string, findingId: string): Promise<Uint8Array>;
  batchUpdateFindings(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listStaleFindings(principal: string, thresholdSeconds: number): Promise<Uint8Array>;
  pruneFindings(
    principal: string,
    olderThanDays: number | null,
    dryRun: boolean,
  ): Promise<Uint8Array>;
  getReadingProgress(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  putReadingProgress(
    principal: string,
    workId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  deleteReadingProgress(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  listAnnotations(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  createAnnotation(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  patchAnnotation(
    principal: string,
    annotationId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  deleteAnnotation(principal: string, annotationId: string): Promise<Uint8Array>;
  listReferences(principal: string): Promise<Uint8Array>;
  getReference(principal: string, referenceId: string): Promise<Uint8Array>;
  // ── P5-T2 Actor / memory / context family surface ─────────────────────────
  listCharacters(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  createCharacter(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  getCharacter(principal: string, characterId: string): Promise<Uint8Array>;
  patchCharacter(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  archiveCharacter(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  restoreCharacter(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  addCharacterBinding(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listCharacterBindings(
    principal: string,
    characterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  getCharacterBinding(
    principal: string,
    characterId: string,
    bindingId: string,
  ): Promise<Uint8Array>;
  patchCharacterBinding(
    principal: string,
    characterId: string,
    bindingId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  removeCharacterBinding(principal: string, characterId: string, bindingId: string): Promise<void>;
  actorKnowledgeView(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  addActorKnowledgeEntry(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listCharacterKnowledge(
    principal: string,
    characterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  getKnowledgeEntry(principal: string, characterId: string, entryId: string): Promise<Uint8Array>;
  patchKnowledgeEntry(
    principal: string,
    characterId: string,
    entryId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  deleteKnowledgeEntry(
    principal: string,
    characterId: string,
    entryId: string,
    expectedRevision: number,
  ): Promise<void>;
  listCreators(queryJson: Uint8Array): Promise<Uint8Array>;
  createCreator(displayName: string): Promise<Uint8Array>;
  getCreator(creatorId: string): Promise<Uint8Array>;
  patchCreator(creatorId: string, displayName: string | null): Promise<Uint8Array>;
  setActiveCreator(requestJson: Uint8Array): Promise<Uint8Array>;
  getActiveCreator(): Promise<Uint8Array>;
  logoutCreator(creatorId: string): Promise<Uint8Array>;
  captureCharacterPendingReview(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listCharacterPendingReviews(
    principal: string,
    characterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  countCharacterPendingReviews(
    principal: string,
    characterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  deleteCharacterPendingReview(
    principal: string,
    characterId: string,
    pendingId: string,
  ): Promise<Uint8Array>;
  reviewCharacterMemory(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listCharacterMemoryFragments(
    principal: string,
    characterId: string,
    queryJson: Uint8Array,
  ): Promise<Uint8Array>;
  promoteCharacterFragment(
    principal: string,
    characterId: string,
    fragmentId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  reflectCharacterSoul(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  recordCharacterTom(
    principal: string,
    characterId: string,
    requestJson: Uint8Array,
  ): Promise<Uint8Array>;
  listCharacterTom(principal: string, characterId: string, queryJson: Uint8Array): Promise<Uint8Array>;
  listPendingReviews(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  countPendingReviews(principal: string): Promise<Uint8Array>;
  deletePendingReview(principal: string, pendingId: string): Promise<Uint8Array>;
  reviewMemory(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  listMemoryFragments(principal: string, queryJson: Uint8Array): Promise<Uint8Array>;
  reflectCreatorSoul(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  inspectMoment(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
  momentDirective(principal: string, requestJson: Uint8Array): Promise<Uint8Array>;
}

export interface NativeBinding {
  compatibility(): string;
  open(optionsJson: string, callbacks?: ProviderCallbacksNative): NativeCoreBinding;
}

interface PlatformPackageManifest {
  name: string;
  version: string;
  os?: string[];
  cpu?: string[];
  libc?: string[];
}

/** The one platform this process may load, with its Rust target triple. */
export interface PlatformTarget {
  name: string;
  targetTriple: string;
  libc?: 'glibc';
}

/** Runtime-derived expectations a manifest must match before any DB open. */
export interface RuntimeExpectations {
  target_triple: string;
  package_version: string;
  contract_tree_sha256?: string;
  db_schema_min?: number;
  db_schema_max?: number;
  napi_minimum?: number;
}

export function detectLinuxLibc(): 'glibc' | 'musl' {
  try {
    const report = process.report?.getReport?.() as
      | { header?: { glibcVersionRuntime?: string } }
      | undefined;
    if (report?.header?.glibcVersionRuntime) return 'glibc';
  } catch {
    // fall through to musl
  }
  return 'musl';
}

/** Current OS/arch/libc → the exact platform package and Rust target triple. */
export function expectedPlatformPackage(): PlatformTarget {
  const { platform, arch } = process;
  if (platform === 'darwin' && arch === 'arm64') {
    return { name: '@42ch/nexus-native-darwin-arm64', targetTriple: 'aarch64-apple-darwin' };
  }
  if (platform === 'darwin' && arch === 'x64') {
    return { name: '@42ch/nexus-native-darwin-x64', targetTriple: 'x86_64-apple-darwin' };
  }
  if (platform === 'win32' && arch === 'x64') {
    return { name: '@42ch/nexus-native-win32-x64-msvc', targetTriple: 'x86_64-pc-windows-msvc' };
  }
  if (platform === 'linux' && arch === 'x64') {
    return {
      name: '@42ch/nexus-native-linux-x64-gnu',
      targetTriple: 'x86_64-unknown-linux-gnu',
      libc: 'glibc',
    };
  }
  throw new Error(`unsupported platform ${platform}/${arch}`);
}

export function expectedTargetTriple(): string {
  return expectedPlatformPackage().targetTriple;
}

function resolvePlatformPackageRoot(pkgName: string): string {
  const shortName = pkgName.replace('@42ch/', '');
  const workspacePath = join(__dirname, '..', '..', shortName);
  if (existsSync(join(workspacePath, 'package.json'))) {
    return workspacePath;
  }
  return dirname(require.resolve(`${pkgName}/package.json`));
}

export function readPackageManifest(pkgName: string): PlatformPackageManifest {
  const pkgJsonPath = join(resolvePlatformPackageRoot(pkgName), 'package.json');
  const parsed = JSON.parse(readFileSync(pkgJsonPath, 'utf8')) as PlatformPackageManifest;
  if (typeof parsed.name !== 'string' || typeof parsed.version !== 'string') {
    throw new Error(`invalid platform package manifest for ${pkgName}`);
  }
  return parsed;
}

/** Exact-platform fencing: os/cpu (and Linux libc) are required, not optional. */
function assertPlatformManifest(
  manifest: PlatformPackageManifest,
  target: PlatformTarget,
): void {
  const { platform, arch } = process;
  if (!manifest.os?.length) throw new Error('platform package manifest is missing "os"');
  if (!manifest.os.includes(platform)) {
    throw new Error(`platform package os mismatch: expected ${platform}, got ${manifest.os.join(',')}`);
  }
  if (!manifest.cpu?.length) throw new Error('platform package manifest is missing "cpu"');
  if (!manifest.cpu.includes(arch)) {
    throw new Error(`platform package cpu mismatch: expected ${arch}, got ${manifest.cpu.join(',')}`);
  }
  if (target.libc === 'glibc') {
    if (detectLinuxLibc() !== 'glibc') {
      throw new Error('linux musl host cannot load the gnu platform package');
    }
    if (!manifest.libc?.length) throw new Error('platform package manifest is missing "libc"');
    if (!manifest.libc.includes('glibc')) {
      throw new Error(`platform package libc mismatch: expected glibc, got ${manifest.libc.join(',')}`);
    }
  }
}

export function loadNodePath(): string {
  const target = expectedPlatformPackage();
  assertPlatformManifest(readPackageManifest(target.name), target);
  const nodePath = join(resolvePlatformPackageRoot(target.name), 'native', 'nexus_core_node.node');
  if (!existsSync(nodePath)) {
    throw new Error(
      `platform native artifact missing at ${nodePath}; run packages/nexus-native/scripts/build.mjs`,
    );
  }
  return nodePath;
}

export function loadNativeBinding(): NativeBinding {
  const nodePath = loadNodePath();
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  return require(nodePath) as NativeBinding;
}

export function readCompatibilityManifest(binding: NativeBinding): NativeCompatibility {
  const parsed = JSON.parse(binding.compatibility()) as NativeCompatibility;
  assertManifestShape(parsed, 'native compatibility manifest');
  return parsed;
}

/** The manifest shipped beside the loaded artifact — required, never optional. */
export function readBundledCompatibility(nodePath?: string): NativeCompatibility {
  const resolved = nodePath ?? loadNodePath();
  const compatPath = join(dirname(resolved), 'compatibility.json');
  if (!existsSync(compatPath)) {
    throw new Error(`missing platform compatibility manifest at ${compatPath}`);
  }
  const parsed = JSON.parse(readFileSync(compatPath, 'utf8')) as NativeCompatibility;
  assertManifestShape(parsed, 'platform compatibility manifest');
  return parsed;
}

function assertManifestShape(manifest: NativeCompatibility, label: string): void {
  const fields: readonly (keyof NativeCompatibility)[] = [
    'native_api_version',
    'writer_protocol',
    'target_triple',
    'package_version',
    'contract_tree_sha256',
    'db_schema_min',
    'db_schema_max',
    'napi_minimum',
  ];
  if (typeof manifest !== 'object' || manifest === null) {
    throw new Error(`${label}: not an object`);
  }
  for (const field of fields) {
    if (manifest[field] === undefined) throw new Error(`${label}: missing "${field}"`);
  }
}

/**
 * Fence a manifest against the *runtime-derived* expectations (target triple,
 * package version) plus, when supplied, the adjacent manifest's contract hash.
 */
export function assertCompatibility(
  manifest: NativeCompatibility,
  expected?: RuntimeExpectations,
): void {
  // Fixed contract value first: the declared N-API floor is part of the
  // compatibility contract, not a per-build choice.
  if (manifest.napi_minimum !== REQUIRED_NAPI_MINIMUM) {
    throw new Error(
      `napi_minimum must be the contract value ${REQUIRED_NAPI_MINIMUM}, got ${manifest.napi_minimum}`,
    );
  }
  const napiVersion = Number(process.versions.napi ?? '0');
  if (napiVersion < manifest.napi_minimum) {
    throw new Error(`napi version ${napiVersion} < required ${manifest.napi_minimum}`);
  }
  if (manifest.native_api_version !== 1) {
    throw new Error(`native_api_version mismatch: ${manifest.native_api_version}`);
  }
  if (manifest.writer_protocol !== 1) {
    throw new Error(`writer_protocol mismatch: ${manifest.writer_protocol}`);
  }
  if (!/^[a-f0-9]{64}$/.test(manifest.contract_tree_sha256)) {
    throw new Error('contract_tree_sha256 shape invalid');
  }
  if (manifest.contract_tree_sha256 === PLACEHOLDER_HASH) {
    throw new Error('contract_tree_sha256 placeholder rejected');
  }
  if (manifest.db_schema_min > manifest.db_schema_max) {
    throw new Error('db_schema range invalid');
  }
  if (expected) {
    if (manifest.target_triple !== expected.target_triple) {
      throw new Error(
        `target_triple mismatch: host requires ${expected.target_triple}, artifact reports ${manifest.target_triple}`,
      );
    }
    if (manifest.package_version !== expected.package_version) {
      throw new Error(
        `package_version mismatch: expected ${expected.package_version}, got ${manifest.package_version}`,
      );
    }
    if (
      expected.contract_tree_sha256 &&
      manifest.contract_tree_sha256 !== expected.contract_tree_sha256
    ) {
      throw new Error('contract_tree_sha256 mismatch against the adjacent manifest');
    }
    if (expected.db_schema_min !== undefined && manifest.db_schema_min !== expected.db_schema_min) {
      throw new Error(
        `db_schema_min mismatch: runtime requires ${expected.db_schema_min}, artifact declares ${manifest.db_schema_min}`,
      );
    }
    if (expected.db_schema_max !== undefined && manifest.db_schema_max !== expected.db_schema_max) {
      throw new Error(
        `db_schema_max mismatch: runtime requires ${expected.db_schema_max}, artifact declares ${manifest.db_schema_max}`,
      );
    }
    if (expected.napi_minimum !== undefined && manifest.napi_minimum !== expected.napi_minimum) {
      throw new Error(
        `napi_minimum mismatch: expected ${expected.napi_minimum}, got ${manifest.napi_minimum}`,
      );
    }
  }
}

/**
 * Fence the loading pair: the native manifest and the manifest shipped beside
 * the artifact must each satisfy the host's runtime expectations and must agree
 * on every compatibility value before the database is opened.
 */
export function fenceCompatibilityPair(
  nativeManifest: NativeCompatibility,
  adjacentManifest: NativeCompatibility,
  runtime: { target_triple: string; package_version: string },
): void {
  assertCompatibility(nativeManifest, {
    ...runtime,
    contract_tree_sha256: adjacentManifest.contract_tree_sha256,
    db_schema_min: adjacentManifest.db_schema_min,
    db_schema_max: adjacentManifest.db_schema_max,
    napi_minimum: adjacentManifest.napi_minimum,
  });
  assertCompatibility(adjacentManifest, {
    ...runtime,
    db_schema_min: nativeManifest.db_schema_min,
    db_schema_max: nativeManifest.db_schema_max,
    napi_minimum: nativeManifest.napi_minimum,
  });
}
