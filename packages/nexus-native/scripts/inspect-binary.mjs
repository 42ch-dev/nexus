#!/usr/bin/env node
/**
 * P3-T1 — actual binary inspection for one target artifact.
 *
 * Reads the built artifact and reports what it *is*, not what it was meant to
 * be: container format and machine type, minimum OS / glibc symbol ceiling,
 * dependent libraries, the bundled SQLite link, and the toolchain that
 * produced it (architecture-contracts §9).
 *
 * Every measurement is either parsed from the file itself or taken from the
 * platform's own tooling. A missing measurement tool or a missing symbol is
 * reported as `unavailable` and fails the run — never silently skipped.
 *
 * ELF/Mach-O structures are read with the host tools (`readelf`, `otool`)
 * because they are the authoritative parsers for their platform; the PE import
 * table has no guaranteed host tool on the Windows runner, so it is parsed
 * directly from the file.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { release as osRelease } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = 'packages/nexus-native/scripts/inspect-binary.mjs';
const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..', '..', '..');
const IS_WIN = process.platform === 'win32';

const TARGETS = {
  'aarch64-apple-darwin': { suffix: 'darwin-arm64', os: 'darwin', cpu: 'arm64', minOs: '11.0' },
  'x86_64-apple-darwin': { suffix: 'darwin-x64', os: 'darwin', cpu: 'x64', minOs: '11.0' },
  'x86_64-pc-windows-msvc': { suffix: 'win32-x64-msvc', os: 'win32', cpu: 'x64', maxSubsystem: '10.0' },
  'x86_64-unknown-linux-gnu': {
    suffix: 'linux-x64-gnu',
    os: 'linux',
    cpu: 'x64',
    libc: 'gnu',
    maxGlibc: '2.28',
  },
};

/** SQLite rodata markers that only the statically linked engine carries. */
const SQLITE_MARKERS = [
  'unable to open database file',
  'no such table: ',
  'database is locked',
  'SQLITE_',
  'sqlite3_',
];

const USAGE = `usage: node ${SCRIPT} --target <rust-triple> --artifact <path> --out <dir>

  --target    one of ${Object.keys(TARGETS).join(', ')}
  --artifact  the built .node artifact to inspect
  --out       evidence directory (binary-inspection.json is written here)`;

const state = { startedAt: new Date().toISOString(), evidencePath: null, target: null };

function fail(message, detail) {
  const suffix = detail === undefined ? '' : `\n${JSON.stringify(detail, null, 2)}`;
  process.stderr.write(`${SCRIPT}: ${message}${suffix}\n`);
  writeFailEvidence(message, detail);
  process.exit(1);
}

/**
 * A failed run must leave its own record: a stale `pass` document on disk would
 * otherwise be read as a green result by the matrix summary.
 */
function writeFailEvidence(message, detail) {
  if (!state.evidencePath) return;
  try {
    mkdirSync(dirname(state.evidencePath), { recursive: true });
    if (existsSync(state.evidencePath)) {
      const previous = JSON.parse(readFileSync(state.evidencePath, 'utf8'));
      const tag = previous.status === 'pass' ? 'pass' : 'pre';
      renameSync(state.evidencePath, `${state.evidencePath.replace(/\.json$/, '')}.${tag}-${Date.now()}.json`);
    }
    writeFileSync(
      state.evidencePath,
      `${JSON.stringify(
        {
          schema: 'rft-p3-t1-binary-inspection/v1',
          status: 'fail',
          script: SCRIPT,
          target: state.target,
          utc_start: state.startedAt,
          utc_end: new Date().toISOString(),
          error: message,
          error_detail: detail ?? null,
        },
        null,
        2,
      )}\n`,
    );
  } catch {
    // the original failure is reported on stderr; a failed write must not mask it
  }
}

function run(command, args, options = {}) {
  const res = spawnSync(command, args, {
    cwd: options.cwd ?? ROOT,
    env: options.env ?? process.env,
    encoding: 'utf8',
    shell: options.shell ?? (IS_WIN && options.forceNoShell !== true),
    maxBuffer: 64 * 1024 * 1024,
  });
  if (res.error) return { status: 127, stdout: '', stderr: res.error.message, available: false };
  return { status: res.status ?? 1, stdout: res.stdout ?? '', stderr: res.stderr ?? '', available: true };
}

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function parseArgs(argv) {
  const parsed = { target: null, artifact: null, out: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const value = () => {
      i += 1;
      if (i >= argv.length) fail(`missing value for ${arg}`);
      return argv[i];
    };
    if (arg === '--target') parsed.target = value();
    else if (arg === '--artifact') parsed.artifact = value();
    else if (arg === '--out') parsed.out = value();
    else if (arg === '--help' || arg === '-h') {
      process.stdout.write(`${USAGE}\n`);
      process.exit(0);
    } else fail(`unknown argument ${arg}\n${USAGE}`);
  }
  if (!parsed.target || !parsed.artifact || !parsed.out) {
    fail(`--target, --artifact and --out are required\n${USAGE}`);
  }
  if (!TARGETS[parsed.target]) fail(`unknown target ${parsed.target}`, { known: Object.keys(TARGETS) });
  return parsed;
}

function versionParts(version) {
  return String(version)
    .split('.')
    .map((part) => {
      const parsed = Number.parseInt(part, 10);
      return Number.isNaN(parsed) ? 0 : parsed;
    });
}

function compareVersions(a, b) {
  const left = versionParts(a);
  const right = versionParts(b);
  for (let i = 0; i < Math.max(left.length, right.length); i += 1) {
    const diff = (left[i] ?? 0) - (right[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

// --- container / machine type ---------------------------------------------

function detectContainer(buffer, target) {
  const spec = TARGETS[target];
  if (buffer.length > 64 && buffer[0] === 0x7f && buffer[1] === 0x45 && buffer[2] === 0x4c && buffer[3] === 0x46) {
    const little = buffer[5] === 1;
    const machine = little ? buffer.readUInt16LE(18) : buffer.readUInt16BE(18);
    const names = { 0x3e: 'x86_64', 0xb7: 'aarch64' };
    return { container: 'elf', machine: names[machine] ?? `0x${machine.toString(16)}`, matches_target: (names[machine] ?? null) === spec.cpu };
  }
  if (buffer.length > 32 && buffer.readUInt32LE(0) === 0xfeedfacf) {
    const cputype = buffer.readInt32LE(4);
    const names = { 0x0100000c: 'arm64', 0x01000007: 'x64' };
    return { container: 'mach-o', machine: names[cputype] ?? `0x${cputype.toString(16)}`, matches_target: (names[cputype] ?? null) === spec.cpu };
  }
  if (buffer.length > 64 && buffer[0] === 0x4d && buffer[1] === 0x5a) {
    const peOffset = buffer.readUInt32LE(0x3c);
    const signatureOk =
      peOffset + 6 < buffer.length && buffer.toString('latin1', peOffset, peOffset + 4) === 'PE\u0000\u0000';
    if (!signatureOk) return { container: 'pe', machine: 'unreadable-header', matches_target: false };
    const machine = buffer.readUInt16LE(peOffset + 4);
    const names = { 0x8664: 'x64', 0xaa64: 'arm64' };
    return { container: 'pe', machine: names[machine] ?? `0x${machine.toString(16)}`, matches_target: (names[machine] ?? null) === spec.cpu };
  }
  return { container: 'unknown', machine: 'unknown', matches_target: false };
}

/** Direct PE parse: subsystem + OS version + import table (no host tool needed). */
function parsePe(buffer) {
  const peOffset = buffer.readUInt32LE(0x3c);
  const coff = peOffset + 4;
  const numberOfSections = buffer.readUInt16LE(coff + 2);
  const sizeOfOptionalHeader = buffer.readUInt16LE(coff + 16);
  const optional = coff + 20;
  const optionalMagic = buffer.readUInt16LE(optional);
  const is64 = optionalMagic === 0x20b;
  const directoriesOffset = optional + (is64 ? 112 : 96);
  const importRva = buffer.readUInt32LE(directoriesOffset + 8);
  const sections = [];
  for (let index = 0; index < numberOfSections; index += 1) {
    const header = optional + sizeOfOptionalHeader + index * 40;
    sections.push({
      virtualSize: buffer.readUInt32LE(header + 8),
      virtualAddress: buffer.readUInt32LE(header + 12),
      rawSize: buffer.readUInt32LE(header + 16),
      rawPointer: buffer.readUInt32LE(header + 20),
    });
  }
  const toOffset = (rva) => {
    for (const section of sections) {
      const size = Math.max(section.virtualSize, section.rawSize);
      if (rva >= section.virtualAddress && rva < section.virtualAddress + size) {
        return section.rawPointer + (rva - section.virtualAddress);
      }
    }
    return null;
  };
  const readCString = (offset) => {
    let end = offset;
    while (end < buffer.length && buffer[end] !== 0) end += 1;
    return buffer.toString('latin1', offset, end);
  };
  const imports = [];
  if (importRva > 0) {
    const descriptorOffset = toOffset(importRva);
    if (descriptorOffset !== null) {
      for (let index = 0; index < 256; index += 1) {
        const entry = descriptorOffset + index * 20;
        if (entry + 20 > buffer.length) break;
        const nameRva = buffer.readUInt32LE(entry + 12);
        const originalFirstThunk = buffer.readUInt32LE(entry);
        if (nameRva === 0 && originalFirstThunk === 0) break;
        const nameOffset = toOffset(nameRva);
        imports.push(nameOffset === null ? `<rva:0x${nameRva.toString(16)}>` : readCString(nameOffset));
      }
    }
  }
  return {
    optional_magic: `0x${optionalMagic.toString(16)}`,
    os_version: `${buffer.readUInt16LE(optional + 40)}.${buffer.readUInt16LE(optional + 42)}`,
    subsystem_version: `${buffer.readUInt16LE(optional + 48)}.${buffer.readUInt16LE(optional + 50)}`,
    subsystem: buffer.readUInt16LE(optional + 68),
    imported_dlls: imports,
  };
}

// --- platform inspections --------------------------------------------------

function inspectDarwin(artifact, spec, findings) {
  const loadCommands = run('otool', ['-l', artifact], { forceNoShell: true });
  if (!loadCommands.available || loadCommands.status !== 0) {
    fail('otool is required to inspect a Mach-O artifact', { stderr: loadCommands.stderr });
  }
  const minosMatches = [...loadCommands.stdout.matchAll(/^\s*minos\s+([0-9.]+)/gm)].map((m) => m[1]);
  const versionMinMatches = [...loadCommands.stdout.matchAll(/LC_VERSION_MIN_MACOSX[\s\S]{0,200}?\n\s*version\s+([0-9.]+)/g)].map((m) => m[1]);
  const minimumOs = minosMatches[0] ?? versionMinMatches[0] ?? null;
  findings.minimum_os = minimumOs;
  findings.minimum_os_source = minosMatches[0] ? 'LC_BUILD_VERSION.minos' : versionMinMatches[0] ? 'LC_VERSION_MIN_MACOSX.version' : null;
  findings.deployment_target_env = process.env.MACOSX_DEPLOYMENT_TARGET ?? null;

  const deps = run('otool', ['-L', artifact], { forceNoShell: true });
  const libraries = deps.stdout
    .split('\n')
    .slice(1)
    .map((line) => line.trim().split(' ')[0])
    .filter(Boolean);
  findings.dependent_libraries = libraries;
  findings.non_system_libraries = libraries.filter(
    (library) => !/^(@rpath|@loader_path|@executable_path|\/usr\/lib\/|\/System\/Library\/)/.test(library),
  );

  const codesign = run('codesign', ['-dv', '--verbose=4', artifact], { forceNoShell: true });
  findings.codesign = {
    display_ok: codesign.status === 0,
    stderr_tail: codesign.stderr.slice(-800),
    output_tail: codesign.stdout.slice(-800),
    note: 'ad-hoc development signature of the local proof artifact; SEC-1 signing/notarization is P3-T3',
  };
  return [
    {
      name: 'darwin_minimum_os_floor',
      ok: minimumOs !== null && compareVersions(minimumOs, spec.minOs) <= 0,
      detail: `minos=${minimumOs ?? 'unavailable'} required<=${spec.minOs} source=${findings.minimum_os_source}`,
    },
    {
      name: 'darwin_system_libraries_only',
      ok: findings.non_system_libraries.length === 0,
      detail: `non_system=${JSON.stringify(findings.non_system_libraries)}`,
    },
  ];
}

function inspectLinux(artifact, spec, findings) {
  const header = run('readelf', ['-h', artifact], { forceNoShell: true });
  if (!header.available || header.status !== 0) {
    fail('readelf is required to inspect an ELF artifact', { stderr: header.stderr });
  }
  findings.elf_machine = header.stdout.match(/Machine:\s+(.+)/)?.[1]?.trim() ?? null;
  findings.elf_type = header.stdout.match(/Type:\s+(.+)/)?.[1]?.trim() ?? null;

  const dynamic = run('readelf', ['-d', artifact], { forceNoShell: true });
  findings.needed_libraries = [...dynamic.stdout.matchAll(/\(NEEDED\)\s+Shared library: \[([^\]]+)\]/g)].map((m) => m[1]);
  findings.soname = dynamic.stdout.match(/\(SONAME\)\s+Library soname: \[([^\]]+)\]/)?.[1] ?? null;
  findings.rpath = [...dynamic.stdout.matchAll(/\((?:RPATH|RUNPATH)\)\s+Library (?:rpath|runpath): \[([^\]]+)\]/g)].map((m) => m[1]);

  const versionInfo = run('readelf', ['--version-info', artifact], { forceNoShell: true });
  // The glibc ceiling is the set of *needed* version symbols. Version
  // definitions (`.gnu.version_d`) describe symbols this library exports and
  // must not be mixed into the requirement.
  const needsStart = versionInfo.stdout.indexOf('Version needs section');
  const definitionsStart = versionInfo.stdout.indexOf('Version definition section');
  const needsScope =
    needsStart >= 0
      ? versionInfo.stdout.slice(needsStart, definitionsStart > needsStart ? definitionsStart : undefined)
      : versionInfo.stdout;
  findings.glibc_scan_scope = needsStart >= 0 ? '.gnu.version_r' : 'whole-output fallback (no version needs section)';
  const glibcVersionMatches = [...needsScope.matchAll(/Name:\s+GLIBC_([0-9.]+)/g)].map((m) => m[1]);
  const glibcVersions = [...new Set(glibcVersionMatches)];
  const maxGlibc = maxOf(glibcVersions);
  findings.required_glibc_symbols = glibcVersions.sort(compareVersions);
  findings.max_required_glibc = maxGlibc;
  findings.other_version_namespaces = [
    ...new Set([...needsScope.matchAll(/Name:\s+((?:GLIBCXX|CXXABI|GCC)_[0-9.]+)/g)].map((m) => m[1])),
  ];

  const ldd = run('ldd', [artifact], { forceNoShell: true });
  findings.ldd = ldd.status === 0 ? ldd.stdout.trim().split('\n') : [`unavailable: ${ldd.stderr.trim()}`];

  return [
    {
      name: 'linux_glibc_symbol_ceiling',
      ok: maxGlibc !== null && compareVersions(maxGlibc, spec.maxGlibc) <= 0,
      detail: `max=${maxGlibc ?? 'unavailable'} required<=${spec.maxGlibc} scope=${findings.glibc_scan_scope}`,
    },
    {
      name: 'linux_needed_libraries_are_sonames',
      ok: findings.needed_libraries.every((library) => !library.startsWith('/')),
      detail: JSON.stringify(findings.needed_libraries),
    },
  ];
}

function inspectWindows(buffer, spec, findings) {
  let pe;
  try {
    pe = parsePe(buffer);
  } catch (error) {
    findings.pe_parse_error = String(error);
    return [
      { name: 'windows_pe_imports_readable', ok: false, detail: `parse failed: ${error}` },
      { name: 'windows_subsystem_floor', ok: false, detail: 'PE header unreadable' },
    ];
  }
  findings.pe = pe;
  const merged = [...new Set([...pe.imported_dlls, ...(findings.host_tool_imports ?? [])])];
  findings.imported_dlls = merged;
  findings.non_system_dlls = merged.filter((dll) => dll.includes('/') || dll.includes('\\'));
  return [
    {
      name: 'windows_pe_imports_readable',
      ok: merged.length > 0,
      detail: `imports=${JSON.stringify(merged)} via=${findings.host_tool_imports_from ?? 'direct PE parse'}`,
    },
    {
      name: 'windows_subsystem_floor',
      ok: compareVersions(pe.subsystem_version, spec.maxSubsystem) <= 0,
      detail: `subsystem_version=${pe.subsystem_version} required<=${spec.maxSubsystem} subsystem=${pe.subsystem}`,
    },
    {
      name: 'windows_imports_are_bare_dll_names',
      ok: findings.non_system_dlls.length === 0,
      detail: `non_system=${JSON.stringify(findings.non_system_dlls)}`,
    },
  ];
}

/** Optional second opinion on Windows when a dumpbin-compatible tool exists. */
function windowsHostImports(artifact) {
  const res = run('objdump', ['-p', artifact], { forceNoShell: true });
  if (!res.available || res.status !== 0) return { tool: null, imports: [] };
  const imports = [...res.stdout.matchAll(/DLL Name:\s*(\S+)/g)].map((m) => m[1]);
  return { tool: 'objdump -p', imports };
}

function maxOf(versions) {
  let max = null;
  for (const version of versions) {
    if (max === null || compareVersions(version, max) > 0) max = version;
  }
  return max;
}

// --- SQLite link -----------------------------------------------------------

function sqliteEvidence(buffer, findings) {
  const external = (findings.needed_libraries ?? []).filter((library) => /sqlite/i.test(library));
  const externalDlls = (findings.imported_dlls ?? []).filter((dll) => /sqlite/i.test(dll));
  const embedded = SQLITE_MARKERS.filter((marker) => buffer.includes(marker, 0, 'latin1'));
  const symbolTools = [];
  let symbols = [];
  for (const [tool, args] of [
    ['nm', ['-D', '--defined-only']],
    ['nm', ['--defined-only']],
    ['objdump', ['-T']],
  ]) {
    const res = run(tool, [...args, '-C'], { forceNoShell: true, allowFailure: true });
    if (res.available && res.status === 0) {
      const found = [...res.stdout.matchAll(/\b(sqlite3_[A-Za-z0-9_]+)\b/g)].map((m) => m[1]);
      symbolTools.push({ tool: `${tool} ${args[0]}`, symbols: found.length });
      symbols = symbols.concat(found);
    }
  }
  return {
    external_libraries: [...external, ...externalDlls],
    embedded_markers: embedded,
    symbol_tools: symbolTools,
    sqlite_symbols_found: [...new Set(symbols)].slice(0, 20),
    bundled: external.length === 0 && externalDlls.length === 0,
  };
}

// --- toolchain -------------------------------------------------------------

function toolchainEvidence() {
  const versions = {};
  const capture = (key, command, args) => {
    const res = run(command, args, { forceNoShell: true });
    versions[key] = res.available && res.status === 0 ? res.stdout.trim().split('\n')[0] : null;
  };
  capture('rustc', 'rustc', ['-Vv']);
  capture('cc', 'cc', ['--version']);
  capture('clang', 'clang', ['--version']);
  capture('gcc', 'gcc', ['--version']);
  capture('ld', 'ld', ['--version']);
  capture('xcodebuild', 'xcodebuild', ['-version']);
  capture('node', process.execPath, ['-p', 'process.versions.node']);
  return {
    ...versions,
    glibc_host: process.report?.getReport?.()?.header?.glibcVersionRuntime ?? null,
    macos_deployment_target: process.env.MACOSX_DEPLOYMENT_TARGET ?? null,
    windows_sdk_version: process.env.WindowsSDKVersion ?? null,
    windows_sdk_dir: process.env.WindowsSdkDir ?? null,
    vc_tools_version: process.env.VCToolsVersion ?? null,
    vscmd_ver: process.env.VSCMD_VER ?? null,
  };
}

// --- main ------------------------------------------------------------------

const args = parseArgs(process.argv.slice(2));
const spec = TARGETS[args.target];
const outDir = resolve(args.out);
const evidencePath = join(outDir, 'binary-inspection.json');
state.evidencePath = evidencePath;
state.target = args.target;
const artifact = resolve(args.artifact);
if (!existsSync(artifact)) fail(`artifact not found at ${artifact}`);
const checks = [];
const record = (name, ok, detail) => checks.push({ name, ok: Boolean(ok), detail });

const buffer = readFileSync(artifact);
const findings = {};
const container = detectContainer(buffer, args.target);
findings.container = container;

record('artifact_container_matches_target', container.container !== 'unknown' && container.matches_target, container);
let platformChecks = [];
if (args.target.includes('darwin')) platformChecks = inspectDarwin(artifact, spec, findings);
else if (args.target.includes('linux')) platformChecks = inspectLinux(artifact, spec, findings);
else if (args.target.includes('windows')) {
  const hostImports = windowsHostImports(artifact);
  findings.host_tool_imports = hostImports.imports;
  findings.host_tool_imports_from = hostImports.tool;
  platformChecks = inspectWindows(buffer, spec, findings);
} else fail(`no platform inspection implemented for ${args.target}`);
for (const check of platformChecks) record(check.name, check.ok, check.detail);

const sqlite = sqliteEvidence(buffer, findings);
findings.sqlite = sqlite;
record('sqlite_is_bundled_not_a_second_library', sqlite.bundled, { external: sqlite.external_libraries });
record(
  'sqlite_link_found',
  sqlite.embedded_markers.length > 0 || sqlite.sqlite_symbols_found.length > 0,
  { markers: sqlite.embedded_markers, symbols: sqlite.sqlite_symbols_found },
);

const toolchain = toolchainEvidence();
const payload = {
  schema: 'rft-p3-t1-binary-inspection/v1',
  status: checks.every((check) => check.ok) ? 'pass' : 'fail',
  script: SCRIPT,
  target: args.target,
  platform_package: `nexus-native-${spec.suffix}`,
  utc_start: state.startedAt,
  utc_end: new Date().toISOString(),
  artifact: {
    path: artifact,
    bytes: statSync(artifact).size,
    sha256: sha256File(artifact),
  },
  host: {
    platform: process.platform,
    arch: process.arch,
    os_release: osRelease(),
    node: process.versions.node,
  },
  toolchain,
  findings,
  checks,
};
mkdirSync(outDir, { recursive: true });
if (existsSync(evidencePath)) {
  try {
    const previous = JSON.parse(readFileSync(evidencePath, 'utf8'));
    if (previous.status !== 'pass') renameSync(evidencePath, `${evidencePath.replace(/\.json$/, '')}.pre-${Date.now()}.json`);
  } catch {
    renameSync(evidencePath, `${evidencePath.replace(/\.json$/, '')}.pre-${Date.now()}.json`);
  }
}
writeFileSync(evidencePath, `${JSON.stringify(payload, null, 2)}\n`);
process.stdout.write(`${SCRIPT}: ${payload.status} -> ${evidencePath}\n`);
process.exit(payload.status === 'pass' ? 0 : 1);
