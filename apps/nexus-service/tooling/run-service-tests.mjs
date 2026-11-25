// Explicit recursive test runner for nexus-service: finds every tests/**/*.test.mjs
// (portable across Node versions — do not rely on `node --test <dir>` recursing),
// sorts them for deterministic order, and runs them under one node --test process.
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { constants as signals } from "node:os";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
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
  // Set exitCode instead of process.exit() so pipe-backed stdio drains; map a
  // signal-killed child to the conventional 128+signal status.
  if (result.status === null) {
    process.exitCode = result.signal ? 128 + (signals[result.signal] ?? 1) : 1;
  } else {
    process.exitCode = result.status;
  }
}

main();
