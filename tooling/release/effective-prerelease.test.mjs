import assert from "node:assert/strict";
import { test } from "node:test";
import {
  effectivePrerelease,
  parsePrereleaseTrailer,
  PRERELEASE_TRAILER,
} from "./effective-prerelease.mjs";
import { signingImplemented } from "./release-config.mjs";
import { cleanupTempRepo, createTempRepo, runReleaseScript, writeRepoFile } from "./test-harness.mjs";

test("PRERELEASE_TRAILER is the pinned trailer name", () => {
  assert.equal(PRERELEASE_TRAILER, "Nexus-Prerelease");
});

test("signingImplemented is false — the unsigned floor SSOT", () => {
  assert.equal(typeof signingImplemented, "boolean");
  assert.equal(signingImplemented, false);
});

test("parsePrereleaseTrailer reads the trailer value", () => {
  assert.equal(
    parsePrereleaseTrailer("chore(release): bump version to 0.2.0\n\nNexus-Prerelease: true\n"),
    true,
  );
  assert.equal(parsePrereleaseTrailer("msg\n\nNexus-Prerelease: false\n"), false);
  assert.equal(parsePrereleaseTrailer("msg\n\nnexus-prerelease: TRUE\n"), true);
  assert.equal(parsePrereleaseTrailer("msg\n\n  Nexus-Prerelease:   false  \n"), false);
});

test("parsePrereleaseTrailer fails closed on absent, invalid or conflicting values", () => {
  assert.equal(parsePrereleaseTrailer("chore(release): bump version to 0.2.0\n"), null);
  assert.equal(parsePrereleaseTrailer("msg\n\nNexus-Prerelease: yes\n"), null);
  assert.equal(
    parsePrereleaseTrailer("msg\n\nNexus-Prerelease: true\nNexus-Prerelease: false\n"),
    null,
  );
  assert.equal(parsePrereleaseTrailer(undefined), null);
});

test("parsePrereleaseTrailer fails closed when a readable value has a garbled duplicate", () => {
  for (const garbled of [
    "Nexus-Prerelease:",
    "Nexus-Prerelease: ",
    "Nexus-Prerelease:: false",
    "Nexus-Prerelease: false extra",
  ]) {
    assert.equal(
      parsePrereleaseTrailer(`msg\n\nNexus-Prerelease: false\n${garbled}\n`),
      null,
      garbled,
    );
    assert.equal(
      parsePrereleaseTrailer(`msg\n\n${garbled}\nNexus-Prerelease: true\n`),
      null,
      garbled,
    );
  }
});

test("garbled duplicates force a prerelease even with signing implemented", () => {
  /** Mirrors the release pipeline: an unreadable trailer means prerelease. */
  const effectiveFromMessage = (message, signing) => {
    const trailer = parsePrereleaseTrailer(message);
    return effectivePrerelease(trailer ?? true, signing);
  };

  // Readable `false` alone: signing implemented publishes a real Release.
  assert.equal(effectiveFromMessage("msg\n\nNexus-Prerelease: false\n", true), false);
  // Same message plus any garbled duplicate: fail closed back to prerelease.
  for (const garbled of [
    "Nexus-Prerelease:",
    "Nexus-Prerelease: false extra",
    "Nexus-Prerelease: yes",
  ]) {
    assert.equal(
      effectiveFromMessage(`msg\n\nNexus-Prerelease: false\n${garbled}\n`, true),
      true,
      garbled,
    );
  }
});

test("effectivePrerelease is toggle OR 'signing unimplemented'", () => {
  // Signing unimplemented (today): every Release is a prerelease.
  assert.equal(effectivePrerelease(true, false), true);
  assert.equal(effectivePrerelease(false, false), true);
  // Signing implemented: the dispatch toggle decides.
  assert.equal(effectivePrerelease(true, true), true);
  assert.equal(effectivePrerelease(false, true), false);
  // Default argument is the release-config SSOT.
  assert.equal(effectivePrerelease(false), true);
});

test("a suffixed release version forces a prerelease regardless of toggle or signing", () => {
  // The suffix disjunct dominates the full-release path: even with signing
  // implemented and an explicit `prerelease: false`, alpha/rc stays prerelease.
  assert.equal(effectivePrerelease(false, true, "0.2.0-alpha.1"), true);
  assert.equal(effectivePrerelease(false, true, "0.2.0-rc.1"), true);
  assert.equal(effectivePrerelease(false, false, "0.2.0-rc.1"), true);
  assert.equal(effectivePrerelease(true, true, "0.2.0-alpha.3"), true);
});

test("a clean release version keeps today's matrix", () => {
  // Signing unimplemented (today): every Release is a prerelease.
  assert.equal(effectivePrerelease(false, false, "0.2.0"), true);
  // Signing implemented: the dispatch toggle decides.
  assert.equal(effectivePrerelease(true, true, "0.2.0"), true);
  assert.equal(effectivePrerelease(false, true, "0.2.0"), false);
  // An absent version leaves the suffix disjunct inert.
  assert.equal(effectivePrerelease(false, true), false);
  assert.equal(effectivePrerelease(false, true, null), false);
});

test("an unreadable release version fails closed to prerelease", () => {
  // Not a well-formed release version: refuse to publish it as a full release.
  assert.equal(effectivePrerelease(false, true, "0.2.0-beta.1"), true);
  assert.equal(effectivePrerelease(false, true, "not-a-version"), true);
});

test("CLI computes the effective value and prints it alone on stdout", () => {
  const dir = createTempRepo();
  try {
    const requested = runReleaseScript("effective-prerelease.mjs", ["--toggle", "false"], dir);
    assert.equal(requested.status, 0, requested.stderr);
    assert.equal(requested.stdout, "true\n");
    assert.match(requested.stderr, /effective prerelease: true \(toggle=false, signingImplemented=false\)/);

    const forced = runReleaseScript("effective-prerelease.mjs", ["--toggle", "true"], dir);
    assert.equal(forced.status, 0);
    assert.equal(forced.stdout, "true\n");
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI reads the trailer from a message file and fails closed when absent", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, "message.txt", "chore(release): bump version to 0.2.0\n\nNexus-Prerelease: true\n");
    const withTrailer = runReleaseScript(
      "effective-prerelease.mjs",
      ["--message-file", "message.txt"],
      dir,
    );
    assert.equal(withTrailer.status, 0, withTrailer.stderr);
    assert.equal(withTrailer.stdout, "true\n");
    assert.match(withTrailer.stderr, /\(trailer=true,/);

    writeRepoFile(dir, "bare.txt", "chore(release): bump version to 0.2.0\n");
    const withoutTrailer = runReleaseScript(
      "effective-prerelease.mjs",
      ["--message-file", "bare.txt"],
      dir,
    );
    assert.equal(withoutTrailer.status, 0);
    assert.equal(withoutTrailer.stdout, "true\n");
    assert.match(withoutTrailer.stderr, /failing closed to prerelease/);

    writeRepoFile(
      dir,
      "garbled.txt",
      "chore(release): bump version to 0.2.0\n\nNexus-Prerelease: false\nNexus-Prerelease: false extra\n",
    );
    const garbled = runReleaseScript(
      "effective-prerelease.mjs",
      ["--message-file", "garbled.txt"],
      dir,
    );
    assert.equal(garbled.status, 0);
    assert.equal(garbled.stdout, "true\n");
    // A readable `false` must not win over the garbled duplicate next to it.
    assert.match(garbled.stderr, /failing closed to prerelease/);
    assert.match(garbled.stderr, /\(trailer-missing/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI reads the trailer from stdin", () => {
  const dir = createTempRepo();
  try {
    const run = runReleaseScript("effective-prerelease.mjs", ["--message-file", "-"], dir, {
      input: "chore(release): bump\n\nNexus-Prerelease: false\n",
    });
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout, "true\n");
    assert.match(run.stderr, /\(trailer=false,/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI --version forces a prerelease for a suffixed version", () => {
  const dir = createTempRepo();
  try {
    const suffixed = runReleaseScript("effective-prerelease.mjs", ["--version", "0.2.0-rc.1"], dir);
    assert.equal(suffixed.status, 0, suffixed.stderr);
    assert.equal(suffixed.stdout, "true\n");
    assert.match(suffixed.stderr, /version=0\.2\.0-rc\.1/);

    const clean = runReleaseScript("effective-prerelease.mjs", ["--version", "0.2.0"], dir);
    assert.equal(clean.status, 0, clean.stderr);
    // Signing is unimplemented, so even the clean version is a prerelease today.
    assert.equal(clean.stdout, "true\n");
    assert.match(clean.stderr, /version=0\.2\.0/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI combines the trailer toggle with a suffixed --version", () => {
  const dir = createTempRepo();
  try {
    writeRepoFile(dir, "message.txt", "chore(release): bump version to 0.2.0-rc.1\n\nNexus-Prerelease: false\n");
    const run = runReleaseScript(
      "effective-prerelease.mjs",
      ["--message-file", "message.txt", "--version", "0.2.0-rc.1"],
      dir,
    );
    assert.equal(run.status, 0, run.stderr);
    assert.equal(run.stdout, "true\n");
    assert.match(run.stderr, /\(trailer=false,/);
    assert.match(run.stderr, /version=0\.2\.0-rc\.1/);
  } finally {
    cleanupTempRepo(dir);
  }
});

test("CLI rejects missing and invalid arguments", () => {
  const dir = createTempRepo();
  try {
    const noArgs = runReleaseScript("effective-prerelease.mjs", [], dir);
    assert.equal(noArgs.status, 1);
    assert.match(noArgs.stderr, /Usage: node tooling\/release\/effective-prerelease\.mjs/);

    const badToggle = runReleaseScript("effective-prerelease.mjs", ["--toggle", "maybe"], dir);
    assert.equal(badToggle.status, 1);
    assert.match(badToggle.stderr, /Invalid --toggle value: maybe/);

    const unknown = runReleaseScript("effective-prerelease.mjs", ["--nope"], dir);
    assert.equal(unknown.status, 1);
    assert.match(unknown.stderr, /Unknown argument: --nope/);

    const missingVersion = runReleaseScript("effective-prerelease.mjs", ["--version"], dir);
    assert.equal(missingVersion.status, 1);
    assert.match(missingVersion.stderr, /--version requires a value/);

    const help = runReleaseScript("effective-prerelease.mjs", ["--help"], dir);
    assert.equal(help.status, 0);
    assert.match(help.stdout, /Usage: node tooling\/release\/effective-prerelease\.mjs/);
  } finally {
    cleanupTempRepo(dir);
  }
});
