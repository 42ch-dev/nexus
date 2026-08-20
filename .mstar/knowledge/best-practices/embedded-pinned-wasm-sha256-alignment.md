---
module: nexus-wasm-host, strategy-samples, docs/module-authoring
date: 2026-08-08
problem_type: best_practice
category: best-practices
severity: medium
tags: [wasm-sha256, manifest-json, embedded-module, build-rs, module-install, loader-verification, stat-fence, quickstart]
applies_when: copying a repo-source module manifest.json alongside a locally-built wasm; teaching module install in docs/quickstarts; authoring modules whose manifest carries a pinned artifact hash
---

# Embedded-Pinned `wasm_sha256` — Align or Delete (V1.155 P3/P4 gotcha)

## Context

Since V1.154 (Greptile P1 fix, `32587fa1`), the repo-source module manifest
(`modules/basic-combat/manifest.json`) **does** carry a `wasm_sha256` field —
but the hash is pinned to the **embedded** artifact: `build.rs` injects the
hash for the embedded copy of the wasm shipped inside the binary. An operator
who copies that manifest alongside their **locally-built** wasm (a different
hash) gets a **broken install**: the loader verifies bytes against
`wasm_sha256` BEFORE compile and rejects on mismatch. The original "no hash in
repo manifest" premise was superseded by this discovery during V1.155 P3 T3.

## Guidance

### Align-or-delete: never copy a repo-source hash as-is

When installing a locally-built wasm with a manifest copied from the repo:

1. **Align**: recompute the hash for the LOCAL build and update the manifest
   field — `shasum -a 256 <id>.wasm` (macOS) / `sha256sum <id>.wasm` (Linux) →
   set `"wasm_sha256"` to the local build's hash.
2. **Or delete**: remove the `wasm_sha256` field entirely — the loader falls
   back to the **stat fence** (size + mtime based verification) and the install
   succeeds without content pairing.

The hash step is **optional** (the module works without it via the stat-fence
fallback); it is recommended when the operator wants content pairing. The
repo-source manifest's hash is embedded-pinned and **never** valid for a
locally-built wasm.

### Teach this in every module-install walkthrough

Quickstarts and module docs must include the align-or-delete step right after
the `cp manifest.json` instruction, with the loader's verification behavior
(`crates/nexus-wasm-host/src/manifest.rs` — field name + byte source) as the
ground truth for what the docs claim. Do not write "copy the manifest and
you're done" — that exact instruction shipped a broken install path in
`strategy-samples/README.md` §5 before V1.155 P4.

## Why This Matters

- The loader's byte verification runs **before compile** — a hash mismatch is
  not a warning, it is an install failure. A third-party integrator following
  copy-the-manifest instructions gets a mysteriously broken module and no
  obvious cause.
- The embedded pin is a build-time artifact, not a distribution contract:
  repo-source manifests and shipped-binary embedded copies legitimately differ.
  Treating the repo manifest as self-contained distribution metadata is the
  trap.
- Docs that mirror loader behavior exactly (field name, byte source, fallback
  semantics) stay correct when the loader evolves; docs that invent a simpler
  contract silently rot.

## When to Apply

- Copying any repo-source `manifest.json` that carries `wasm_sha256` alongside
  a locally-built wasm (align or delete).
- Writing module-install docs / quickstart steps for third-party operators
  (include the optional hash step + the embedded-pin note).
- Reviewing module-authoring guidance: the loader (`manifest.rs`) is the
  authoritative source for what `wasm_sha256` verifies and what the fallback
  is.

## Examples

### Before (broken install path)

```bash
cp modules/basic-combat/manifest.json ~/.nexus42/modules/basic-combat/
cp target/wasm32-wasi/release/basic_combat.wasm ~/.nexus42/modules/basic-combat/
# ❌ manifest.json wasm_sha256 is the EMBEDDED artifact hash (build.rs pin)
# loader verifies bytes BEFORE compile -> mismatch -> install rejected
```

### After (align or delete)

```bash
cp modules/basic-combat/manifest.json ~/.nexus42/modules/basic-combat/
cp target/wasm32-wasi/release/basic_combat.wasm ~/.nexus42/modules/basic-combat/
# align: recompute for the LOCAL build
shasum -a 256 basic_combat.wasm   # or: sha256sum basic_combat.wasm
# -> set "wasm_sha256" in the copied manifest to this hash
# OR delete the field -> loader falls back to the stat fence (size+mtime)
```
