// Explicit recursive test runner for nexus-service (entry point, CLI wiring
// only): finds every tests/**/*.test.mjs (portable across Node versions — do
// not rely on `node --test <dir>` recursing), sorts them for deterministic
// order, and runs them under one node --test process. Exit-code mapping lives
// in ./lib/child-exit.mjs (side-effect-free, importable).
// Self-check: `node tooling/run-service-tests.mjs --self-test` verifies the
// child exit-code mapping (including 128+signal for signal-killed children).
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { CHILD_EXIT_CASES, mapChildExit } from "./lib/child-exit.mjs";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

function selfTest() {
  let failed = 0;
  for (const [name, result, expected] of CHILD_EXIT_CASES) {
    const actual = mapChildExit(result);
    if (actual !== expected) {
      process.stderr.write(`SELF-TEST FAIL: ${name}: expected ${expected}, got ${actual}\n`);
      failed += 1;
    }
  }
  if (failed > 0) process.exitCode = 1;
  else process.stdout.write(`runner self-test passed (${CHILD_EXIT_CASES.length} fixtures)\n`);
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

  // Every suite imports the built service instead of rebuilding it (a rebuild
  // inside a suite races the other files on the one shared `dist/`). The
  // `test` script stages the native addon and runs `tsc` before this runner; a
  // direct suite run must fail here, by name, rather than at a dynamic import.
  const entry = join(pkgDir, "dist", "index.js");
  if (!existsSync(entry)) {
    process.stderr.write(
      `dist/index.js is missing — run \`pnpm --filter @42ch/nexus-service run test\` (builds dist and stages the native addon); \`… run build\` only writes dist\n`,
    );
    process.exitCode = 1;
    return;
  }

  const result = spawnSync(
    process.execPath,
    ["--test", "--test-timeout=180000", ...files],
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
