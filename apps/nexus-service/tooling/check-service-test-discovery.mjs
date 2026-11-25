// Verifies the nexus-service test entry discovers every suite under tests/ and
// that the helper/fixture naming contract holds: any file carrying `.test.mjs`
// must actually be a suite. The suite discriminator is import-aware: it parses
// the file's `node:test` imports and looks for anchored, modifier-aware calls
// of the bound names — `test(`, `it(`, `describe(`, with `.skip`/`.only`/
// `.todo`/`.concurrent` modifiers. So regex literals like /re/.test(value),
// `obj.test()` methods, and `// test(` comment mentions do not count, while
// `it(`-based and `test.skip(` suites do.
// The entry gate requires the package.json test script to invoke
// tooling/run-service-tests.mjs (the recursive runner): a flat glob
// (`tests/*.test.mjs`) or a narrowed entry (`node --test tests/http/`) skips
// subdirectory suites and must fail.
// Self-check: `node tooling/check-service-test-discovery.mjs --self-test` runs
// the matcher against fixtures and exits nonzero on any mismatch.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = fileURLToPath(new URL("..", import.meta.url));
const testsDir = join(pkgDir, "tests");
const RUNNER = "tooling/run-service-tests.mjs";

const IMPORT_RE =
  /import\s+(?:([A-Za-z_$][\w$]*)(?:\s*,\s*)?)?(?:\{([\s\S]*?)\}|\*\s+as\s+([A-Za-z_$][\w$]*))?\s+from\s*["']node:test["']/g;
const MODIFIERS = "(\\.(skip|only|todo|concurrent))?";

function boundTestNames(src) {
  const names = new Set();
  const namespaces = [];
  for (const m of src.matchAll(IMPORT_RE)) {
    if (m[1]) names.add(m[1]); // default import
    if (m[2]) {
      for (const part of m[2].split(",")) {
        const p = part.trim();
        if (!p) continue;
        const alias = p.match(/^[\w$]+\s+as\s+([\w$]+)$/);
        names.add(alias ? alias[1] : p);
      }
    }
    if (m[3]) namespaces.push(m[3]);
  }
  return { names, namespaces };
}

function isSuite(src) {
  const { names, namespaces } = boundTestNames(src);
  for (const n of names) {
    if (new RegExp(`(^|[^\\w.])${n}${MODIFIERS}\\s*\\(`, "m").test(src)) return true;
  }
  for (const ns of namespaces) {
    if (new RegExp(`(^|[^\\w.])${ns}\\.(describe|it|test)${MODIFIERS}\\s*\\(`, "m").test(src)) {
      return true;
    }
  }
  return false;
}

function selfTest() {
  const cases = [
    ["it( declaration", "import {it} from 'node:test';\nit('works', () => {});", true],
    ["test.skip( modifier", "import {test} from 'node:test';\ntest.skip('later', () => {});", true],
    ["describe( declaration", "import {describe} from 'node:test';\ndescribe('g', () => {});", true],
    ["test.todo( modifier", "import {test} from 'node:test';\ntest.todo('later');", true],
    ["aliased import", "import {test as t} from 'node:test';\nt('works', () => {});", true],
    ["namespace import", "import * as nt from 'node:test';\nnt.test('works', () => {});", true],
    ["default import", "import test from 'node:test';\ntest('works', () => {});", true],
    ["regex .test( helper", "export const ok = /safe integer/.test('safe integer 9');", false],
    ["plain helper", "export const helper = true;\n// test( mentioned in a comment", false],
    ["method call named test", "const obj = { test() {} };\nobj.test();", false],
    ["helper importing node:test without calling", "import {test} from 'node:test';\nexport const t = test;", false],
  ];
  let failed = 0;
  for (const [name, src, expected] of cases) {
    const actual = isSuite(src);
    if (actual !== expected) {
      process.stderr.write(`SELF-TEST FAIL: ${name}: expected suite=${expected}, got ${actual}\n`);
      failed += 1;
    }
  }
  if (failed > 0) process.exitCode = 1;
  else process.stdout.write(`self-test passed (${cases.length} fixtures)\n`);
  return;
}

if (process.argv.includes("--self-test")) {
  selfTest();
} else {
  main();
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

function main() {
  const files = walk(testsDir)
    .filter((f) => f.endsWith(".test.mjs"))
    .sort();
  const topLevel = files.filter((f) => !relative(testsDir, f).includes("/"));
  const subdir = files.filter((f) => relative(testsDir, f).includes("/"));

  const errors = [];
  for (const f of files) {
    const src = readFileSync(f, "utf8");
    if (!isSuite(src)) {
      errors.push(`helper/fixture carries .test.mjs without being a suite: ${relative(pkgDir, f)}`);
    }
  }

  const testScript = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf8")).scripts.test;
  if (!testScript.includes(RUNNER)) {
    errors.push(
      `test entry must invoke ${RUNNER} (recursive runner); flat glob or narrowed entries skip subdirectory suites: ${testScript}`,
    );
  } else if (!existsSync(join(pkgDir, RUNNER))) {
    errors.push(`test script invokes ${RUNNER} but the runner file is missing`);
  }

  process.stdout.write(
    `discovered ${files.length} .test.mjs files (${topLevel.length} top-level, ${subdir.length} in subdirectories)\n`,
  );
  for (const e of errors) process.stderr.write(`FAIL: ${e}\n`);
  if (errors.length > 0) process.exitCode = 1;
  else process.stdout.write("test discovery check passed\n");
}
