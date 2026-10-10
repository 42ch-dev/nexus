import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { assembleSha256sums } from "./assemble-sha256sums.mjs";

const VERSION = "0.1.1";
const DIGESTS = ["a", "b", "c", "d", "e", "f", "0"].map((char) => char.repeat(64));
const names = [
  "nexus-runtime-windows-x64.zip",
  "nexus-runtime-macos-arm64.zip",
  "nexus-runtime-linux-x64.zip",
  `Nexus-${VERSION}-darwin-arm64-unsigned.dmg`,
  `Nexus-${VERSION}-darwin-arm64-unsigned.app.zip`,
  `Nexus-${VERSION}-darwin-x64-unsigned.dmg`,
  `Nexus-${VERSION}-darwin-x64-unsigned.app.zip`,
];

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "assemble-sha256sums-"));
  const runtimeSlugs = ["windows-x64", "macos-arm64", "linux-x64"];
  for (let index = 0; index < runtimeSlugs.length; index += 1) {
    const slug = runtimeSlugs[index];
    const dir = join(root, `nexus-runtime-${slug}`);
    mkdirSync(dir);
    writeFileSync(join(dir, `${names[index]}.sha256`), `${DIGESTS[index]}  ${names[index]}\n`);
  }
  for (const [offset, arch] of [[3, "arm64"], [5, "x64"]]) {
    const dir = join(root, `nexus-desktop-unsigned-darwin-${arch}`);
    mkdirSync(dir);
    writeFileSync(join(dir, "SHA256SUMS"), `${DIGESTS[offset]}  ${names[offset]}\n${DIGESTS[offset + 1]}  ${names[offset + 1]}\n`);
  }
  return root;
}

test("assembles exactly seven producer digests in release-body order", () => {
  const root = fixture();
  try {
    const actual = assembleSha256sums({ version: VERSION, artifactsDir: root });
    assert.equal(actual, names.map((name, index) => `${DIGESTS[index]}  ${name}\n`).join(""));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("CLI writes the named manifest from producer sidecars", () => {
  const root = fixture();
  const output = join(root, `Nexus-${VERSION}-SHA256SUMS`);
  try {
    const script = new URL("./assemble-sha256sums.mjs", import.meta.url);
    const result = spawnSync(process.execPath, [script.pathname, "--version", VERSION, "--artifacts-dir", root, "--output", output], { encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(readFileSync(output, "utf8"), names.map((name, index) => `${DIGESTS[index]}  ${name}\n`).join(""));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("refuses a missing sidecar and names its artifact", () => {
  const root = fixture();
  try {
    rmSync(join(root, "nexus-runtime-macos-arm64", "nexus-runtime-macos-arm64.zip.sha256"));
    assert.throws(() => assembleSha256sums({ version: VERSION, artifactsDir: root }), /missing producer checksum sidecar for nexus-runtime-macos-arm64\.zip/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("refuses a malformed sidecar line", () => {
  const root = fixture();
  try {
    writeFileSync(join(root, "nexus-runtime-linux-x64", "nexus-runtime-linux-x64.zip.sha256"), `not-a-digest  ${names[2]}\n`);
    assert.throws(() => assembleSha256sums({ version: VERSION, artifactsDir: root }), /malformed checksum line for nexus-runtime-linux-x64\.zip/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("refuses a missing desktop artifact line", () => {
  const root = fixture();
  try {
    writeFileSync(join(root, "nexus-desktop-unsigned-darwin-arm64", "SHA256SUMS"), `${DIGESTS[3]}  ${names[3]}\n`);
    assert.throws(() => assembleSha256sums({ version: VERSION, artifactsDir: root }), /checksum absent for Nexus-0\.1\.1-darwin-arm64-unsigned\.app\.zip/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
