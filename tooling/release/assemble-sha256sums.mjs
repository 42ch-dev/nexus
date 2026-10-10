#!/usr/bin/env node
/** Assemble the release-wide SHA-256 manifest from producer sidecars. */

import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const USAGE = "Usage: node tooling/release/assemble-sha256sums.mjs --version <version> --artifacts-dir <path> --output <path>";
const HEX_DIGEST = /^[a-fA-F0-9]{64}$/;

const artifactsFor = (version) => [
  { name: "nexus-runtime-windows-x64.zip", sidecar: "nexus-runtime-windows-x64/nexus-runtime-windows-x64.zip.sha256", line: "single" },
  { name: "nexus-runtime-macos-arm64.zip", sidecar: "nexus-runtime-macos-arm64/nexus-runtime-macos-arm64.zip.sha256", line: "single" },
  { name: "nexus-runtime-linux-x64.zip", sidecar: "nexus-runtime-linux-x64/nexus-runtime-linux-x64.zip.sha256", line: "single" },
  ...["arm64", "x64"].flatMap((arch) => ["dmg", "app.zip"].map((suffix) => ({
    name: `Nexus-${version}-darwin-${arch}-unsigned.${suffix}`,
    sidecar: `nexus-desktop-unsigned-darwin-${arch}/SHA256SUMS`,
    line: "manifest",
  }))),
];

function parseSidecarLine(text, artifact, sidecar) {
  const match = /^(\S+)\s+\*?(.+)$/.exec(text);
  if (!match || !HEX_DIGEST.test(match[1]) || match[2] !== artifact) {
    throw new Error(`malformed checksum line for ${artifact} in ${sidecar}`);
  }
  return `${match[1].toLowerCase()}  ${artifact}`;
}

export function assembleSha256sums({ version, artifactsDir }) {
  const output = [];
  const singleSidecars = new Map();
  for (const artifact of artifactsFor(version)) {
    const sidecar = join(artifactsDir, artifact.sidecar);
    let lines = singleSidecars.get(sidecar);
    if (!lines) {
      let contents;
      try {
        contents = readFileSync(sidecar, "utf8");
      } catch (error) {
        if (error.code === "ENOENT") throw new Error(`missing producer checksum sidecar for ${artifact.name}: ${sidecar}`);
        throw error;
      }
      lines = contents.split(/\r?\n/).filter((line) => line.length > 0);
      singleSidecars.set(sidecar, lines);
    }
    if (artifact.line === "single") {
      if (lines.length !== 1) throw new Error(`malformed checksum line for ${artifact.name} in ${sidecar}`);
      output.push(parseSidecarLine(lines[0], artifact.name, sidecar));
    } else {
      const matches = lines.filter((line) => {
        const fields = /^(\S+)\s+\*?(.+)$/.exec(line);
        return fields?.[2] === artifact.name;
      });
      if (matches.length === 0) throw new Error(`checksum absent for ${artifact.name} in ${sidecar}`);
      if (matches.length !== 1) throw new Error(`malformed checksum line for ${artifact.name} in ${sidecar}`);
      output.push(parseSidecarLine(matches[0], artifact.name, sidecar));
    }
  }
  return `${output.join("\n")}\n`;
}

function main(argv) {
  let version;
  let artifactsDir;
  let output;
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    const value = argv[++index];
    if (!value || !["--version", "--artifacts-dir", "--output"].includes(key)) {
      process.stderr.write(`${USAGE}\n`);
      return 2;
    }
    if (key === "--version") version = value;
    else if (key === "--artifacts-dir") artifactsDir = value;
    else output = value;
  }
  if (!version || !artifactsDir || !output || basename(output) !== `Nexus-${version}-SHA256SUMS`) {
    process.stderr.write(`${USAGE}\n`);
    return 2;
  }
  try {
    writeFileSync(output, assembleSha256sums({ version, artifactsDir }), "utf8");
    return 0;
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    return 1;
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
