---
module: apps/desktop-electron packaging driver + CI (unsigned macOS .app/.dmg lane)
date: 2026-09-20
problem_type: tooling_decision
category: tooling-decisions
severity: medium
plan_id: 2026-09-19-v1.192-p1-unsigned-packaging-distribution
applies_when:
  - building or reviewing an unsigned macOS packaging lane (Electron or similar) that must succeed with zero Apple credentials
  - adding preflight/verification to a build driver whose failure modes must be named, not implicit
  - deciding how to prove "no signing tool ran" rather than asserting it
  - writing or consuming a package receipt for provenance
related_components:
  - apps/desktop-electron
  - packages/nexus-native
  - apps/nexus-service
tags:
  - packaging
  - electron
  - unsigned
  - fail-closed
  - receipt
  - provenance
  - sentinel
  - atomic-publish
---

# Unsigned macOS packaging lane — zero-credential success, fail-closed preflight, signed-shape rejection

## Context

v1.192's RFT-10 needed a maintainer with **no Apple credentials** to produce versioned unsigned `.app` + `.dmg` for both macOS architectures (arm64 and x86_64) from a documented command and a CI entry, while any request that would sign, notarize or staple must fail closed. Before this lane existed, two hidden signing surfaces were found:

- The installed Electron packager (20.3) mutates the framework integrity digest and **invokes ad-hoc `codesign` even with no `osxSign` configured** (`asarIntegrityDigest` path).
- The internal native build helper (`packages/nexus-native/scripts/build.mjs`) invoked `codesign` unconditionally on macOS and had a stale `compatibility.json` fallback that could mask a failed load.

The lane that shipped separates four concerns that tend to be conflated: **the driver** (stage + build artifacts), **preflight** (named prerequisite checks), **publication** (atomic output), and **verification** (read-only, separate tool). The reusable decisions are below.

## Guidance

### 1. Make zero-credential success the default path and signed requests fail before effects

- Command surface is closed (`--arch`, `--out`, `--help`); unknown flags fail before mutation. A retired `--signed-required`-style request is rejected at parse/preflight time (`package.args.unknown`, exit 1) with **no output directory created**.
- Signing-related environment variables (`APPLE_*`, `CSC_*`, `SIGNING_*`, `NOTARIZE_*`) are rejected by the driver's `assertNoSigningEnvironment` — ambient credentials must not change behavior.
- There is deliberately **no** "would sign if credentials existed" lane and no dormant signing branch: future signing is a new, separately authorized implementation, not a config flag.
- Keep the two failure classes distinct: fail-closed applies to (a) unsupported signed/release requests and (b) missing *build* prerequisites. An ordinary unsigned build must **succeed** when no credentials are present — that is the acceptance test, not an exception to it.

### 2. Disable the packager's hidden signing mutations and prove it with process-spawn sentinels

- Set `asarIntegrityDigest: false`; omit `osxSign`/`osxNotarize`; do not patch Electron fuses or Mach-O binaries; never strip/re-sign bundled vendor binaries.
- Do not assert "no signing ran" from config inspection alone. Put **fake executables first on `PATH`** for `codesign`, `notarytool`, `stapler` (each records its name and exits 97) and fail the run if any is invoked; the CI job applies the same sentinel around the real packaging command and fails on a non-zero sentinel-call count. Narrow a coarse sentinel when it over-blocks: an `xcrun` sentinel must pass through non-signing subcommands and block only signing subcommands.
- The native prerequisite build script must lose its signing mutation too (not hide it behind a flag), and its load step must **load the freshly built `.node` and derive compatibility metadata from the binding** — remove any stale-manifest fallback, because it can turn a failed load into a "passing" manifest.

### 3. Preflight before output creation, with named errors and exact remediation

Every required input is validated before staging, and each failure returns a **named code plus the command that fixes it** (non-zero exit; no final receipt, no success-shaped artifact, no leftover output directory):

| Missing input | Named error | Remediation text |
| --- | --- | --- |
| web dist | `package.preflight.missing_web_dist` | `run pnpm run build:web` |
| dependency closure (root/workspace `node_modules`, virtual-store lock equality, packager root) | `package.preflight.missing_dependency_closure` | `run pnpm install --frozen-lockfile` |
| host/preload/service outputs | named per input | exact build command |
| compatible native payload | named per input | separate native build instruction (never a silent fetch) |
| target arch vs runner | `package.preflight.arch` (e.g. x64 target on arm64 runner) | use a native runner |
| icons / DMG tools | named per input | icon generation / platform requirement |

Two structural rules make this work:

- **The packaging driver never builds its own inputs.** Absence of an input is a preflight failure, not an implicit build: a fallback `pnpm run build` inside the driver would silently blur "the inputs were ready" with "the driver made them".
- **Imports stay deferred until after preflight.** The packager module is imported only after the closure checks pass, so a missing dependency closure surfaces as the named check rather than a module-resolution stack trace.

### 4. Publish atomically; never destroy a previous good output

Stage into a temporary directory, verify completeness, then publish the per-arch output directory with a single rename; clean stale staging/backup dirs and **fsync files before the publication rename**. A failed rebuild leaves a previously completed output untouched. Output layout is versioned and architecture-keyed: `artifacts/desktop/<version>/darwin-<arch>/{Nexus.app, Nexus-<version>-darwin-<arch>-unsigned.dmg, …app.zip, receipt.json, SHA256SUMS}`. An explicit relative `--out` resolves against the caller's cwd (repo-root resolution writes to an unexpected location for callers running from elsewhere); absolute paths pass through.

### 5. The receipt is a closed, versioned provenance record — and the app manifest is separate

`receipt.json` is a closed version-1 object: `schema_version`, product name/bundle id/version, `git_revision`, `dirty`, arch/platform/minimum macOS, tool versions (Node/pnpm/Electron/packager), native contract hash + target, **input digests** (lockfile/web-dist/service/native), **artifact entries** (relative path, bytes, SHA256), `signing_performed:false`, `notarization_performed:false`, `inherited_signature_metadata`, and named check results. It carries no environment dump and no secrets.

- Record the **dirty delta truthfully** — a receipt that says `dirty:true` is valid evidence; a clean-looking receipt over a dirty tree is not.
- A stable sorted **app-file manifest** of the raw `.app` contents is required: zip/DMG SHA256 values are archive digests, not app content manifests.
- Distinguish *recorded* from *verified*: a SHA recorded by the builder is not independent verification. Extraction/identity/inputs verification is a separate run (see §6) and must be stated as such.
- "Reproducible" here means pinned/recorded inputs + repeatable layout + provenance — **not** an unproved bit-identical DMG (timestamps differ). Say which one you mean.

### 6. Verify with a separate read-only tool that does not build, mount, sign or launch

`scripts/verify-package.mjs` consumes a published arch directory and checks: receipt schema/identity consistency, app metadata (name/id/version/minimum OS/arch), the app-file manifest, archive digests, Mach-O architecture and **minimum-OS load commands**, the unpacked native `.node` closure, and compiled-host policy (no `autoUpdater`/update route). It writes nothing, mounts nothing and invokes no signing tool, and it reports GUI qualification as **not claimed**.

A verifier must fail on absence, not default: a Mach-O with no `minos`/`LC_VERSION_MIN_MACOSX` load command must exit 1 with the named minimum-macOS failure (a fixture-based CLI regression covers exactly this — a `|| 'Nexus'`-style truthiness fallback had swallowed the missing value).

### 7. CI is the authority for cross-architecture evidence

- A native-runner matrix (e.g. `macos-15` for arm64, `macos-15-intel` for x64), each row asserting the actual `process.arch` matches its declared target before packaging.
- Per-arch artifacts (raw `.app` ZIP, `.dmg`, receipt, `SHA256SUMS`) are all required uploads; a missing any one is a job failure.
- When the development host cannot exercise the other architecture, that cell stays `[UNVERIFIED] — CI-only` in the evidence, with the CI job named. Never infer x64 behavior from arm64 construction, and never substitute Rosetta/cross-package for native evidence.
- Bound the child processes: packaging shells out to `hdiutil`/`ditto`/`git`/`pnpm`; give `spawnSync` a timeout, a `maxBuffer`, and explicit signal/error surfacing.

### 8. Say "unsigned" precisely

"Unsigned" here means **this product pipeline performs no Developer ID or ad-hoc signature and does not notarize** — not that every Mach-O load command lacks a signature. Upstream Electron binaries carry vendor/ad-hoc signatures, and the arm64 linker itself ad-hoc-signs native `.node` payloads; those are recorded separately in `inherited_signature_metadata`. Do not claim a signed-release/release-ready artifact, and do not treat an unsigned receipt as a Gatekeeper/installability proof.

## Why This Matters

- **Hidden signing is a real failure mode, not a theoretical one.** The packager's `asarIntegrityDigest` mutation and the native helper's unconditional `codesign` would each have violated the no-signing red line while looking like ordinary configuration.
- **Named preflight errors are the difference between a blocked maintainer and a broken artifact.** "Missing input" with the exact fix command is cheap; a half-staged `artifacts/` tree with a success-shaped exit code is expensive to audit and easy to trust incorrectly.
- **Receipts make acceptance auditable.** Without a closed schema + separate verifier, "we packaged it" degrades into an unfalsifiable claim.

## When to Apply

- Standing up or reviewing any unsigned build/package lane that must be credential-free.
- Porting packaging across hosts (Tauri → Electron here): re-inspect the new toolchain for implicit signing/distribution behavior before trusting defaults.
- Adding CI for a platform the development host cannot build natively: plan the `[UNVERIFIED] — CI-only` markers up front.
- Writing package receipts: copy the closed-schema + separate-verifier split rather than extending the builder with self-checks.

## Examples

### Signing-dispatch sentinel (concept)

```sh
# Fake tools first on PATH; each records its name and exits 97 so the run visibly fails.
mkdir -p "$SENTINEL"
for tool in codesign notarytool stapler; do
  printf '#!/bin/sh\necho %s >> "$SENTINEL/invocations"\nexit 97\n' "$tool" > "$SENTINEL/$tool"
  chmod +x "$SENTINEL/$tool"
done
PATH="$SENTINEL:$PATH" pnpm build:desktop -- --arch arm64
[ ! -f "$SENTINEL/invocations" ] || { echo "signing tool invoked: $(cat "$SENTINEL/invocations")"; exit 1; }
```

### Fail-closed rejection vs named preflight failure

```text
$ node scripts/package.mjs --signed-required --out /tmp/should-not-exist
package.args.unknown                     # exit 1; /tmp/should-not-exist never created

$ node scripts/package.mjs --arch arm64 --out /tmp/missing-web   # apps/web/dist absent
package.preflight.missing_web_dist — run pnpm run build:web       # exit 1; no output dir

$ nexus42 desktop bundle --arch x64      # arm64 host
package.preflight.arch: target x64 requires a native x64 runner, got arm64
```

## Evidence

- Driver + contract — `apps/desktop-electron/scripts/package.mjs`, `scripts/package-contract.mjs`, `scripts/compose-app-icon.mjs`; read-only verifier — `scripts/verify-package.mjs`; dev driver — `scripts/dev.mjs`.
- Tests — `apps/desktop-electron/tests/package-contract.test.mjs` (8 cases incl. missing-output/missing-closure preflight, signing-env rejection, source scan forbidding `osxSign|osxNotarize|APPLE_SIGNING_IDENTITY|entitlements`), `tests/verify-package.test.mjs` (missing-`minos` fixture), `tests/dev-driver.test.mjs` (bounded child teardown).
- Native prerequisite — `packages/nexus-native/scripts/build.mjs` (no signing call sites, no stale compatibility fallback) and its real-load evidence in the platform package `packages/nexus-native-darwin-arm64/native/`.
- CI — `.github/workflows/desktop-electron-build.yml` (native matrix, `process.arch` assertion, PATH sentinels, per-arch artifact uploads).
- CLI entry — `apps/nexus42/src/commands/desktop/mod.rs` (`nexus42 desktop bundle --arch arm64|x64`, obsolete signing request rejected at parse time).
- Related — [pnpm-toolchain-pin-and-supply-chain-age.md](../conventions/pnpm-toolchain-pin-and-supply-chain-age.md) (lockfile/CI install policy this lane depends on), [graph-pin-honesty-discipline.md](../conventions/graph-pin-honesty-discipline.md) (assert-empty false-green trap applies verbatim to sentinel checks).
