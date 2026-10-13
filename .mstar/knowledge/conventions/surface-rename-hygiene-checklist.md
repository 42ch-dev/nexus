---
module: contracts-surface
problem_type: convention
category: conventions
severity: medium
date: 2026-07-05
last_updated: 2026-10-12
tags:
  - rename
  - hygiene
  - grep
  - local-api
  - daemon-api
  - codegen
  - docs
  - anchor
---

# Surface-Rename Hygiene Checklist

A mechanical checklist for renaming a cross-language HTTP/contract surface (e.g., `local-api` → `daemon-api`) in the Nexus monorepo.

## When this applies

- Renaming a JSON Schema subtree under `schemas/`
- Renaming a generated Rust module under `crates/nexus-contracts/src/generated/`
- Renaming a generated TypeScript module under `packages/nexus-contracts/src/generated/`
- Changing the public HTTP path prefix served by the daemon runtime
- Updating consumer-facing docs/specs/AGENTS that refer to the surface by name

## Why this matters

A surface rename touches schemas, codegen, generated code in two languages, the daemon router, multiple clients (CLI, web, desktop), and normative docs. Automated moves and generated-code regeneration handle the bulk, but **doc comments, prompt templates, embedded presets, and cross-crate prose are easy to miss**. Leftover references silently undermine the canonical name and break grep-based navigation.

## Checklist

### 1. Machine-renamable tree
- [ ] Rename schema folder and update `$id`/`$ref` paths.
- [ ] Update codegen config (`tooling/codegen/src/schema-prep.ts`, `ts-gen.ts`, `rust-gen/`).
- [ ] Regenerate Rust and TypeScript outputs; delete the old generated directories.
- [ ] Update package version if the contract surface is breaking.
- [ ] Run `pnpm run codegen` and verify `git status` is clean.
- [ ] Run `cargo test -p nexus-contracts --test schema_drift_detection`.

### 2. Runtime route and clients
- [ ] Update daemon router path prefix (`apps/nexus-service/src/routes.ts`).
- [ ] Update CLI client base URL and path builders (`apps/nexus42/src/api/daemon_client.rs`).
- [ ] Update web client base URL constant (`apps/web/src/lib/nexus/browser-client.ts`).
- [ ] Update Vite dev proxy (`apps/web/vite.config.ts`).
- [ ] Update any desktop/sidecar path references.

### 3. Prose and doc-comment sweep
Run the following regexes over `apps crates docs schemas packages tooling` (excluding `node_modules`, `target/`, `.worktrees/`, `dist/`):

```sh
rg -nE 'local[-_]api|Local API'  # old surface name
rg -nE 'daemon[- ]daemon|daemon Daemon API|Daemon daemon API'  # artifacts of naive s/local/daemon/
rg -nE '/v1/local/'             # old route prefix
```

**These greps are a pre-commit self-check for the sweep executor, not an "available" reference** — run them and resolve every hit before committing the sweep. Where a construction reads "renaming from X to Y", the *X* side is the historical name and must stay — the replace must be context-aware, not a blind token swap. A stutter can also arise where the surrounding text already referenced the new name; the platform greps above catch it.

**Anchor-link verification (run after any heading-text rename):** when the rename touches Markdown heading text, markdown anchor slugs derive from the heading. Grep TOC entries and cross-reference links for slugs built from the *old* heading text and update them to the new slug:

```sh
# after renaming a heading, find TOC/inline links whose slug still reflects the old name
rg -n '#local-api-contract|<old-heading-slug>' docs/ .mstar/specs/
```

A renamed heading whose TOC entry text was updated but whose `#slug` was not produces a silently-broken in-page link.

Review every hit. Allowed exceptions:
- CHANGELOG historical entries
- AGENTS.md explanatory notes that cite the rename itself
- Pre-rename historical comments that reference retired plan IDs

### 4. Embedded content
- [ ] Check `embedded-presets/**` prompt templates for old route strings.
- [ ] Check `.md` knowledge/spec docs that quote request/response examples.
- [ ] Check smoke scripts (`scripts/*.sh`) for hard-coded paths.

### 5. Verification gates
- [ ] `cargo clippy --all -- -D warnings`
- [ ] `cargo test --all`
- [ ] `pnpm --filter web run typecheck && pnpm --filter web run test`
- [ ] `pnpm --filter @42ch/nexus-contracts run build`
- [ ] `pnpm run validate-schemas`

## Retiring a surface (not renaming)

Deleting a surface is not a rename: there is no new name to point at, so the sweep must additionally clear four classes that a rename leaves alone.

1. **Static guardrail rules that name the surface.** A `forbid`/assertion that blocks the retired name protects an object that no longer exists and has no reintroduction path. First-principles test before keeping one: *is the protected object still a live threat, and is the invariant already covered by a stronger assertion in the same file?* If not, delete the rule — do not keep an "anti-X" literal for sentimental coverage. When the underlying invariant **is** still live (e.g. "a browser-only app must not import desktop-shell internals"), rewrite the rule against the *current* implementation, not the retired one.
2. **Docs that still present the surface as live.** Durable knowledge/spec prose often ends with a clause like "the web-side `<surface>` contract still holds". That clause is a claim about the present and must be retired with the surface; keep the historical record, drop the present-tense claim.
3. **Test assertions of the surface's markers.** Assertions on a deleted attribute/flag/global must be removed in the same commit as the marker. If removing the only case that proved a *general* property (e.g. "detection ignores an unrelated global"), restore that property with a neutral fixture name rather than losing the coverage.
4. **Dead runtime attributes and keys.** An attribute with no reader (grep both JS/CSS consumers and the current host) is dead code even if a previous host used it. Where a *serialized key* literally contains the retired name, that is an output-shape change, not a pure deletion: authorize it explicitly and record the consumer disposition (who reads it, and that no in-repo reader keys on it).

**Acceptance for a retirement:** `git grep -i <retired-name>` over **tracked** files returns nothing — the gitignored process/history tree is out of scope by construction.

Two dispositions recur in a retirement sweep: a static rule that protects the retired object is often a **dead rule** (delete it when its object is gone and stronger assertions already cover the invariant), while a *serialized* key that literally contains the retired name is a **shape change**, not a pure deletion — rename it under explicit authorization and record the consumer disposition (who reads it, and that no in-repo reader keys on it).
