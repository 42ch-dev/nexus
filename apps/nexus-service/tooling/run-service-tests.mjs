// Explicit recursive test runner for nexus-service: finds every tests/**/*.test.mjs
// (portable across Node versions — do not rely on `node --test <dir>` recursing),
// sorts them for deterministic order, and runs them under one node --test process.
import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

const files = walk(testsDir)
  .filter((f) => f.endsWith(".test.mjs"))
  .sort();
if (files.length === 0) {
  process.stderr.write(`no .test.mjs files found under ${testsDir}\n`);
  process.exit(1);
}

const result = spawnSync(
  process.execPath,
  ["--test", "--test-concurrency=1", "--test-timeout=180000", ...files],
  { stdio: "inherit", cwd: pkgDir },
);
if (result.error) throw result.error;
process.exit(result.status ?? 1);
