// Verifies the nexus-service test entry discovers every suite under tests/ and
// that the helper/fixture naming contract holds: any file carrying `.test.mjs`
// must actually be a suite (contain at least one `test(` or `describe(` call).
// A suite is "unreachable" when the package.json test entry does not recurse
// (`tests/*.test.mjs`) while a `.test.mjs` lives in a tests/<subdir>/.
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

const files = walk(testsDir).filter((f) => f.endsWith(".test.mjs")).sort();
const topLevel = files.filter((f) => !relative(testsDir, f).includes("/"));
const subdir = files.filter((f) => relative(testsDir, f).includes("/"));

const errors = [];
for (const f of files) {
  const src = readFileSync(f, "utf8");
  if (!/\b(test|describe)\s*\(/.test(src)) {
    errors.push(`helper/fixture carries .test.mjs without being a suite: ${relative(pkgDir, f)}`);
  }
}

const testScript = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf8")).scripts.test;
const entryRecurses = !testScript.includes("tests/*.test.mjs");
if (!entryRecurses && subdir.length > 0) {
  errors.push(
    `test entry is not recursive (${testScript}) but subdirectory suites exist: ${subdir
      .map((f) => relative(testsDir, f))
      .join(", ")}`,
  );
}

console.log(`discovered ${files.length} .test.mjs files (${topLevel.length} top-level, ${subdir.length} in subdirectories)`);
for (const e of errors) console.error(`FAIL: ${e}`);
if (errors.length > 0) process.exit(1);
console.log("test discovery check passed");
