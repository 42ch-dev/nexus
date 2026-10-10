---
module: apps/desktop-electron packaging (@electron/packager + @electron/get fetch chain)
date: 2026-10-09
problem_type: build_error
category: build-errors
severity: high
plan_id: 2026-10-09-v1.209-p2-desktop-launch-integrity
symptoms:
  - "Could not parse checksum file at line 1: PK!…" — a local mirror serving ZIP bytes for the SHASUMS256.txt path
  - "AssertionError [ERR_ASSERTION]: assert(!this.paused)" raised inside the client's undici stream parser when the mirror is a plain `python3 -m http.server` (HTTP/1.0 semantics, no Range support)
  - "Generated checksum for electron-v<version>-<platform>-<arch>.zip did not match expected checksum" with a *deterministic, short* download (observed 130,351,104 bytes for a 130,418,529-byte artifact) even against a curl-verified HTTP/1.1 Range-capable node mirror
  - a cache-hit copy attempt failing at stream `prefinish`, followed by "falling back to re-download" and the truncated-download failure above
root_cause: "On hosts where GitHub egress is blocked, the @electron/get 5.x FetchDownloader path proved unreliable across every local-serving variant tried (checksum routing, HTTP/1.0/no-Range semantics, deterministic truncation); the artifact-cache read path is an independent second failure surface. None of these are application defects."
resolution_type: environment_setup
applies_when:
  - packaging Electron apps on hosts with blocked or unreliable GitHub egress
  - debugging "checksum mismatch" / undici assertion crashes inside @electron/get downloads
  - needing a fully offline Electron fetch for CI or local evidence runs
related_components:
  - apps/desktop-electron/scripts/package.mjs
  - .github/workflows/desktop-electron-build.yml
  - ~/Library/Caches/electron (artifact cache)
tags:
  - electron
  - packaging
  - offline
  - mirror
  - checksum
  - undici
  - electron-zip-dir
  - provenance
---

# Electron packaging on blocked-egress hosts: the fetch chain fails four ways; bypass it with electronZipDir

## Problem

`pnpm --dir apps/desktop-electron run package` downloads the Electron distribution through `@electron/packager` → `@electron/get`. On a host where GitHub egress is blocked, the download must be served locally. Every local-serving variant tried produced a *different* failure, several of them deceptive (`open`-style successes, deterministic truncation).

## Symptoms

| Serving method | Observed failure |
|---|---|
| Legacy one-line node script on a fixed port | Requested `/v<version>/SHASUMS256.txt` fell through to the zip branch → `Could not parse checksum file at line 1: PK!…` |
| `python3 -m http.server` (HTTP/1.0, no Range) | Client-side `assert(!this.paused)` inside the bundled undici parser; the zip stream aborts (`BrokenPipeError` server-side) |
| Purpose-built node server (HTTP/1.1 + byte-range, curl-verified 206/200, correct Content-Length) | Download still completes short **deterministically** (130,351,104 vs 130,418,529 bytes, identical sha across runs) → `Generated checksum … did not match expected checksum` |
| Artifact-cache injection (pre-placed zip) | `prefinish` writable error on the cache-copy path → automatic re-download → truncation again |

The same host downloads the identical URL correctly via `curl`, `node http.get`, and `node fetch` — the failure is specific to the `@electron/get` download path, on both Node 22 and Node 24.

## What Didn't Work

- **Fixing the mirror routing alone** (8731-style prefix bug): moved the failure one layer down.
- **Fixing the transport semantics alone** (HTTP/1.1 + Range server): still truncated in the client.
- **Injecting the Electron zip into the artifact cache**: cache keys are `sha256(dirname(url))` (note: the *directory*, not the file URL — `Cache.getCacheDirectory` sets `pathname = dirname(pathname)` before hashing). A correctly keyed cache entry hit a second client-side failure (`prefinish`), then fell back to re-download and truncated.
- **Switching Node versions** (22.x → 24.x): byte-identical failure behavior.

## Solution

Use the packager's own offline input instead of the download chain: `@electron/packager` supports `electronZipDir`, which makes `getElectronZipPath` resolve `electron-v<version>-<platform>-<arch>.zip` directly from a directory — no download, no checksum round-trip.

For a driver whose options are code-fixed (this repo's `scripts/package.mjs` does not pass `electronZipDir`), a temporary local pass-through gets the same effect:

```sh
# Environment pattern (local-only; restore the patched dependency afterwards):
#   @electron/packager dist/packager.js — inside getElectronZipPath():
#     if (!this.opts.electronZipDir && process.env.ELECTRON_ZIP_DIR) {
#       this.opts.electronZipDir = process.env.ELECTRON_ZIP_DIR;
#     }
ELECTRON_ZIP_DIR=/tmp/electron-mirror/v<version> pnpm --dir apps/desktop-electron run package -- --arch arm64
```

The zip itself came from the local artifact cache (`~/Library/Caches/electron/<key>/electron-v<version>-darwin-arm64.zip`); the local mirror's `SHASUMS256.txt` (double-space `<sha>  <filename>` form is accepted; `*` also parses) was generated from that file's own sha256, so the pairing is self-consistent.

## Why This Works

`electronZipDir` bypasses `downloadElectronZip` entirely: the packager extracts the given zip as if it had downloaded it. Every transport-specific failure mode above disappears because no HTTP request is issued.

## Prevention

- A local mirror for Electron must serve the standard layout — `<mirror>/v<version>/electron-v<version>-<platform>-<arch>.zip` plus a parseable `SHASUMS256.txt` in the same directory — over HTTP/1.1 with byte-range support.
- Treat a truncated download with a **deterministic** size as a client-path defect, not network noise; verify the same URL with an independent client before debugging the server.
- **Provenance duty**: when the Electron zip is supplied from a local cache, its official upstream provenance cannot be re-verified offline — record that limitation in the evidence (this repo's runs record it in the plan's evidence and close notes).
- The acceptance of a packaged app is the launch probe, not the build exit code — see `testing-patterns/gui-app-launch-probe-and-process-hygiene.md` and `build-errors/packed-app-symlink-materialization.md`.
- Related lane policy and receipt expectations: `tooling-decisions/unsigned-macos-packaging-lane.md`.
