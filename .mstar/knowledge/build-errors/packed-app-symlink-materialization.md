---
module: apps/desktop-electron packaging (electron packager → .app → zip/dmg) + CI
date: 2026-10-09
problem_type: build_error
category: build-errors
severity: high
plan_id: 2026-10-08-v1.208-p2-dogfood-sweep
applies_when:
  - packaging a macOS .app whose framework/Resources aliases are symlinks (electron-packager layout)
  - zipping or dmg-ing an app bundle produced through a recursive copy (cpSync/rsync-style)
  - trusting a receipt or closure check that validates staging inputs instead of the packed artifact
  - consuming a published desktop artifact — launch it; a green pipeline is not launchability proof
symptoms:
  - "dyld: Library not loaded: @rpath/Electron Framework.framework/Electron Framework (alias targets absolute into the CI staging path)"
  - launchd OS_REASON_DYLD, process ran ~21ms; both .app.zip and .dmg affected
  - no com.apple.quarantine attribute — a broken bundle, not a Gatekeeper refusal
root_cause: "electron-packager's alias layer kept absolute staging-path symlinks in the packed .app while cpSync(dereference: true) failed to materialize them; the receipt symlink_closure check validated the workspace staging and never scanned the packed artifact"
resolution_type: code_fix
related_components:
  - apps/desktop-electron
tags:
  - packaging
  - symlinks
  - electron
  - receipt
  - artifact-fidelity
  - dogfood
---

# Packed .app ships absolute staging symlinks → consumer launch fails

## Problem

The published v0.1.0 darwin-arm64 app (both the `.app.zip` and the `.dmg`) could not launch on consumer machines, with no Gatekeeper involvement: the process died at dyld with `Library not loaded: @rpath/Electron Framework.framework/Electron Framework`, because the bundle's top-level aliases were **absolute symlinks into the CI staging path** (`/Users/runner/work/nexus/nexus/artifacts/desktop/.staging-arm64-*/packager/...`). The payload (`Versions/A/Electron Framework`, 202,847,024 bytes) was intact; only the alias layer was broken. The first real consumer of the artifact (the v1.208 dogfood sweep) found this — the pipeline itself was green.

## Symptoms

- `dyld: Library not loaded: @rpath/Electron Framework.framework/Electron Framework` referencing `Contents/MacOS/Nexus`; launchd `OS_REASON_DYLD`, process ran ~21ms; zip and dmg both affected; **no `com.apple.quarantine` attribute** (this is a broken bundle, not a quarantine refusal).
- After the symlink defect was fixed, direct exec got further and exposed a second blocker (bootstrap `ENOENT` at `Contents/Resources/resources/product.json`, registered as `R-V1208-P2-11`). Fix layers expose the next layer; budget for that when chasing launchability.

## What Didn't Work

- **`cpSync(src, dst, { recursive: true, dereference: true })` retained a symlink** for the copied alias (reproduced with a fixture: `copyIsSymlink: true`). Do not treat `dereference: true` as a materialization guarantee for this bundle layout.
- **The receipt's `symlink_closure` check was a hardcoded `pass`** validating the workspace *staging* node_modules — the packed artifact was never scanned. Green pipeline, broken product.
- Treating "packaging + read-only verifier passed" as launchability proof: the verifier is by design not a launch test, and the published artifact was the first true consumer of the packed layout.

## Solution

- Materialize the **packed** `.app` before archiving: `normalizeAppBundle(publishedApp)` + `materializeSymlinks` on the packed copy, *before* `createDmg`/`createZip` (staging-side normalization stays as-is).
- Derive receipt `checks.symlink_closure` from a **scan of the produced `.app`**; fail closed on any escaping link; keep the detail string **path-independent** (`scan found no symlink escaping packed root`) so receipts stay deterministic and comparable across staging roots.
- Pin both materializer branches at unit level: an escaping absolute **file** alias *and* a **directory** alias (`Versions/Current`-style dir link with a nested link), asserting payload preservation and a passing scan.
- Verify with a bounded **local packaging run**: assert a single-`Nexus.app` layout (no nesting), 0 remaining symlinks in the final tree, and a receipt equal to the derived scan — then attempt an actual launch and record the outcome verbatim.

## Why This Works

- The defect lives in the alias layer of the *packed copy*, so the fix must act on the packed copy after the packager writes it and before archiving freezes it; staging-side normalization can never reach it.
- Scanning the produced artifact (not its inputs) is what makes the receipt evidence rather than assurance theater — the recorded-vs-verified split from the lane decision applied to the closure check itself.

## Prevention

- Read "green packaging pipeline" as *input readiness*, not artifact launchability; consume the published artifact before claiming it works.
- Never hardcode closure/verification results — derive them from the artifact you actually ship.
- Any absolute path baked into an artifact (bundle alias, receipt detail, plist) is a packaging smell: normalize to relative/rootless, or fail.

## Evidence

- `apps/desktop-electron/scripts/package.mjs` (packed-app normalization + scan-to-receipt wiring), `scripts/package-contract.mjs` (`materializeSymlinks` / `assertNoSymlinkEscape` / path-independent detail), `tests/package-contract.test.mjs` (file-alias + directory-alias fixtures).
- Fix commits `213b5eef2` + `96a62dfb6` (v1.208 P2 W2); raw evidence: `.mstar/sdd/2026-10-08-v1.208-p2-dogfood-sweep/task-w2-evidence.md` (dyld transcript, symlink targets, final-order packaging run).
- Related: [unsigned-macos-packaging-lane.md](../tooling-decisions/unsigned-macos-packaging-lane.md) (§ recorded-vs-verified discipline); open residuals `R-V1208-P2-11` (F-16 bootstrap blocker) and the re-release roadmap row in `.mstar/projects/_default/roadmap.md`.
