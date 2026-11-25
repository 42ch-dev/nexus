// Explicit recursive test runner for nexus-service: finds every tests/**/*.test.mjs
// (portable across Node versions — do not rely on `node --test <dir>` recursing),
// sorts them for deterministic order, and runs them under one node --test process.
// Self-check: `node tooling/run-service-tests.mjs --self-test` verifies the child
// exit-code mapping (including 128+signal for signal-killed children).
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { constants } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");

// Exported for --self-test: forward the child's status; a signal-killed child
// maps to the conventional 128+signal status (1 when the signal is unknown).
export function mapChildExit(result) {
  if (result.status !== null && result.status !== undefined) return result.status;
  if (result.signal) return 128 + (constants.signals[result.signal] ?? 1);
  return 1;
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

function selfTest() {
  const sigTerm = constants.signals.SIGTERM;
  const cases = [
    ["status 0", { status: 0, signal: null }, 0],
    ["status 3", { status: 3, signal: null }, 3],
    ["SIGTERM", { status: null, signal: "SIGTERM" }, 128 + sigTerm],
    ["SIGKILL", { status: null, signal: "SIGKILL" }, 128 + constants.signals.SIGKILL],
    ["unknown signal", { status: null, signal: "SIGWONTEXIST" }, 129],
    ["no status no signal", { status: null, signal: null }, 1],
  ];
  let failed = 0;
  for (const [name, result, expected] of cases) {
    const actual = mapChildExit(result);
    if (actual !== expected) {
      process.stderr.write(`SELF-TEST FAIL: ${String(name)}: expected ${String(expected)}, got ${String(actual)}\n`);
      failed += 1;
    }
  }
  if (failed > 0) process.exitCode = 1;
  else process.stdout.write(`runner self-test passed (${cases.length} fixtures)\n`);
}

function main() {
  const files = walk(testsDir)
    .filter((f) => f.endsWith(".test.mjs"))
    .sort();
  if (files.length === 0) {
    process.stderr.write(`no .test.mjs files found under ${testsDir}\n`);
    process.exitCode = 1;
    return;
  }

  const result = spawnSync(
    process.execPath,
    ["--test", "--test-concurrency=1", "--test-timeout=180000", ...files],
    { stdio: "inherit", cwd: pkgDir },
  );
  if (result.error) throw result.error;
  // Set exitCode instead of process.exit() so pipe-backed stdio drains.
  process.exitCode = mapChildExit(result);
}

if (process.argv.includes("--self-test")) {
  selfTest();
} else {
  main();
}
