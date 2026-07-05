# Nexus AGENTS.md

This file provides decision rules, invariants, and indexes for agents working in the **nexus** open-source monorepo.
Domain-specific rules live in subdirectory AGENTS.md files listed below.

## Repository Identity

This is the **public open-source monorepo** containing `nexus42` CLI (Rust, with integrated daemon runtime), JSON Schema wire contracts (truth source for TypeScript/Rust codegen), and published package `@42ch/nexus-contracts` (npm). Rust `nexus-contracts` crate is monorepo-internal only.

- `STRATEGY.md` — project vision, guiding principles, technology direction
- `CONCEPTS.md` — core domain vocabulary for Nexus OSS

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

**Harness `status.json`:** Open QC residual rows are stored under **root** `residual_findings` in `.mstar/status.json`. Details: [`.mstar/AGENTS.md`](.mstar/AGENTS.md).

## Tech Stack & Protocol Decisions

- **CLI/daemon:** Rust-first (aligns with ACP official SDK availability)
- **Protocol:** ACP-first, skills-second — CLI is an ACP client, not an ACP agent/server
- **Wire format:** JSON Schema as truth source — generates both TypeScript and Rust types

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: **nexus42**
- Daemon runtime: integrated into **`nexus42`** binary (`nexus42 daemon start` → `nexus-daemon-runtime`; no separate `nexus42d` product binary)
- npm scope: **@42ch**
- Contracts package: **@42ch/nexus-contracts**

## Subdirectory Index

See linked AGENTS.md files for per-directory decision rules and invariants:

| Directory | Scope | AGENTS.md |
|-----------|-------|-----------|
| `schemas/` | JSON Schema wire contracts | [`schemas/AGENTS.md`](schemas/AGENTS.md) |
| `tooling/` | Codegen pipeline & CI | [`tooling/AGENTS.md`](tooling/AGENTS.md) |
| `apps/nexus42/` | CLI executable (polyglot product-surfaces dir) | [`apps/nexus42/AGENTS.md`](apps/nexus42/AGENTS.md) |
| `apps/web/` | Web SPA — Control Room + canvas (daemon-served React) | [`apps/web/AGENTS.md`](apps/web/AGENTS.md) |
| `apps/desktop/` | Tauri v2 desktop shell wrapping `apps/web` | [`apps/desktop/AGENTS.md`](apps/desktop/AGENTS.md) |
| `crates/nexus-acp-host/` | ACP client adapter | [`crates/nexus-acp-host/AGENTS.md`](crates/nexus-acp-host/AGENTS.md) |
| `crates/nexus-agent-host/` | Agent host adapter | [`crates/nexus-agent-host/AGENTS.md`](crates/nexus-agent-host/AGENTS.md) |
| `crates/nexus-contracts/` | Generated Rust wire types | [`crates/nexus-contracts/AGENTS.md`](crates/nexus-contracts/AGENTS.md) |
| `crates/nexus-daemon-runtime/` | Daemon runtime (local-only) | [`crates/nexus-daemon-runtime/AGENTS.md`](crates/nexus-daemon-runtime/AGENTS.md) |
| `crates/nexus-home-layout/` | `~/.nexus42/` path layout | [`crates/nexus-home-layout/AGENTS.md`](crates/nexus-home-layout/AGENTS.md) |
| `crates/nexus-local-db/` | Local database layer | [`crates/nexus-local-db/AGENTS.md`](crates/nexus-local-db/AGENTS.md) |
| `crates/nexus-orchestration/` | Orchestration engine | [`crates/nexus-orchestration/AGENTS.md`](crates/nexus-orchestration/AGENTS.md) |
| `crates/nexus-cloud-sync/` | Cloud sync transport | [`crates/nexus-cloud-sync/AGENTS.md`](crates/nexus-cloud-sync/AGENTS.md) |
| `crates/nexus-creator/` | Creator aggregate + local identity | [`crates/nexus-creator/AGENTS.md`](crates/nexus-creator/AGENTS.md) |
| `crates/nexus-creator-memory/` | Memory pipeline, SOUL I/O | [`crates/nexus-creator-memory/AGENTS.md`](crates/nexus-creator-memory/AGENTS.md) |
| `crates/nexus-kb/` | Key blocks + source anchors | [`crates/nexus-kb/AGENTS.md`](crates/nexus-kb/AGENTS.md) |
| `crates/nexus-knowledge/` | Reference sources | [`crates/nexus-knowledge/AGENTS.md`](crates/nexus-knowledge/AGENTS.md) |
| `crates/nexus-narrative/` | Worlds, forks, timelines, manuscripts | [`crates/nexus-narrative/AGENTS.md`](crates/nexus-narrative/AGENTS.md) |
| `crates/nexus-cloud-domain/` | User + pairing (cloud sync domain) | [`crates/nexus-cloud-domain/AGENTS.md`](crates/nexus-cloud-domain/AGENTS.md) |
| `crates/nexus-moment-context-assembly/` | Per-moment context assembly | [`crates/nexus-moment-context-assembly/AGENTS.md`](crates/nexus-moment-context-assembly/AGENTS.md) |
| `.mstar/` | Harness infrastructure | [`.mstar/AGENTS.md`](.mstar/AGENTS.md) |
| `.agents/` | Code-agent skills only (ACP workspace skill root) | [`.agents/AGENTS.md`](.agents/AGENTS.md) |

**`apps/` is the polyglot product-surfaces directory.** Any product surface — regardless of language (Rust CLI, Tauri desktop shell, web SPA, etc.) — lives under `apps/`. Reusable Rust libraries live under `crates/`. See [`apps/AGENTS.md`](apps/AGENTS.md) for the durable placement rule.

**Directory split:** `{HARNESS_DIR}` = `.mstar/`. `.agents/` holds optional `.agents/skills/` for IDE/ACP — not harness SSOT.

**New crate policy:** when adding a new package or crate to the monorepo, create an `AGENTS.md` in that directory — even if minimal — documenting its purpose, key rules, and dependencies.

## Development Policy

**Formatting:** `cargo fmt` must use a **pinned nightly** toolchain so local matches CI exactly (rustfmt formatting rules drift across nightly versions; CI's `Rust fmt & clippy` job pins `FMT_NIGHTLY` in `.github/workflows/ci.yml`). Current pin: **`nightly-2026-06-26`**. Install + use it: `rustup toolchain install nightly-2026-06-26 --component rustfmt` then `cargo +nightly-2026-06-26 fmt --all` (and `--check` to verify). Stable `cargo fmt` ignores `.rustfmt.toml`'s `ignore` field and will **incorrectly reformat** generated code under `crates/nexus-contracts/src/generated/`. When bumping the pin, update both CI and this line.

**Clippy:** Workspace-level config in root `Cargo.toml` enables `pedantic` + `nursery` as `warn`, inherited by all crates. CI runs `cargo clippy --all -- -D warnings`. When fixing clippy errors, auto-fix first (`cargo clippy --fix --allow-dirty --allow-staged`), then handle residual manually. **Do not suppress** with `#[allow(...)]` without a brief justification comment. **Do not change runtime behavior** when fixing lint errors.

**Rust `target/` disk hygiene:** `target/debug` is gitignored but grows without bound on macOS/Linux when the workspace is rebuilt often. Stale `.o` files under `target/debug/deps` and old `target/debug/incremental/*` hashes (e.g. after `pnpm run codegen`, crate renames, or repeated `cargo * --all`) are the usual cause — not a single bug. CI uses ephemeral runners + `rust-cache`; **local developers and agents must not mirror CI’s `--all` cadence during iteration.**

| Phase | Command scope |
|-------|----------------|
| **Daily iteration** (default) | `cargo check -p <crate>`, `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings` for the crate you are editing |
| **Pre-commit / gate** | `cargo clippy --all -- -D warnings`, `cargo test --all` (matches CI) |
| **After codegen or large contract/workspace graph changes** | Prefer `cargo clean` once, then rebuild scoped or `--all` as needed — avoids piling orphan artifacts (including legacy `nexus42d` names) |

**Cleanup (repo root):**

- **Reclaim disk immediately:** `cargo clean` (next full build is slow; expected). If it errors on `target/debug/incremental` (“Directory not empty”), remove the heavy subtrees then retry: `rm -rf target/debug/{deps,incremental}` && `cargo clean`.
- **Periodic maintenance (optional):** `cargo install cargo-sweep` then `cargo sweep -i 14` (remove artifacts unused for 14+ days) when a full clean is too disruptive.
- **When to clean:** `target/debug` over ~50 GiB, filesystem slowness under `target/`, end of a large plan slice, or after deleting/renaming crates.

**Anti-patterns:** Running `cargo test --all` / `cargo clippy --all` on every small edit; skipping cleanup for months while agents run full-workspace builds; treating `target/` bloat as safe to commit (it is always gitignored — clean locally instead).

**Shared build artifacts (worktrees):** Use the repo-root [`.envrc`](.envrc) ( [direnv](https://direnv.net/) ) so `CARGO_TARGET_DIR` points at `~/.cache/nexus-target` **only while you are in this repository** — main checkout and every `.worktrees/*` worktree share one cache. After clone or `git worktree add`, run `direnv allow` once in that checkout root. Without direnv, set the same variable in your shell for the session: `export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"`. **Do not** put `target-dir` in user-level `~/.cargo/config.toml`; that applies to every Rust crate on the machine. This relocates growth; the hygiene rules above still apply.

### Git & repository hygiene

**Clone & fetch (developers):**

```bash
git clone --filter=blob:none --recurse-submodules <url>
cd nexus
git submodule update --init --recursive   # after pull if skill dirs are empty
```

- **`--filter=blob:none`:** faster first clone (blobs fetched on checkout as needed).
- **`--recurse-submodules`:** required for developers — [`.agents/skills/`](.agents/skills/) (ACP skill root) must be present. Two submodules (~272 KB total) are not a clone bottleneck.
- Optional user `~/.gitconfig`: `[clone] filter = blob:none`, `[fetch] prune = true`, `[maintenance] auto = true`. Do **not** set `recurseSubmodules = false` globally.

**Submodule policy:**

| Context | Submodule | Notes |
|---------|-----------|-------|
| Developer clone / worktree | **Full** init | Always `--recurse-submodules` + `git submodule update --init --recursive` after new worktree |
| CI default jobs | **Off** | `actions/checkout` without `submodules: true` (Rust/TS builds do not read skills) |
| CI job needing skills | **On demand** | Add `submodules: true` only when the job touches `.agents/skills/` |

**Worktrees:**

- Path: `.worktrees/<name>/` only (`.worktrees/` is gitignored).
- Share the main repo object store — worktrees do not re-download packs; slowness is checkout (~4k files), not network.
- Parallel dual-track: at most **two** worktrees; remove with `git worktree remove` + `git worktree prune` when the iteration slice ends.
- After `git worktree add`, run `git submodule update --init --recursive`.
- Optional sparse-checkout when editing a subtree only: `git sparse-checkout init --cone` then `git sparse-checkout set apps/web …`.

**Commit discipline (controls object growth):**

| Rule | Practice |
|------|----------|
| Iteration landing | Merge `iteration/{ver}` → `main` via PR with **squash merge** (target: one commit per iteration) |
| Harness updates | Batch PM/status changes into **one commit** per closeout — no micro-commits on `status.json` |
| `status.json` size | Before P-last: `wc -c .mstar/status.json` **< 20_000** bytes |
| Resolved residuals | `lifecycle: resolved` findings **must** move to `.mstar/archived/residuals/<plan-id>.json`; only open/deferred stay in `status.json` |
| Codegen | Schema changes and generated output in the **same commit** (see [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)) |
| Never commit | `target/`, `.worktrees/`, `node_modules/` (gitignored — agents must self-check) |

Harness field rules (`plans[]` non-Done only, `notes.json` for narrative): [`.mstar/AGENTS.md`](.mstar/AGENTS.md) § Plan compaction / `status.json` field discipline.

**Periodic maintenance (monthly or every ~5 iterations):**

```bash
wc -c .mstar/status.json
git count-objects -vH
git maintenance run --task=gc --task=incremental-repack
cargo sweep -i 14   # optional; requires cargo-sweep
```

If `.git` exceeds ~100 MiB or clone slows again: consider `git filter-repo` or an orphan history squash (solo maintainer only; see team before force-push on a shared default branch).

**Anti-patterns:** micro-commits on bloated `status.json`; leaving resolved rows in `residual_findings`; per-worktree `target/` without cleanup; developer clone with `--no-recurse-submodules` and no follow-up `submodule update`; `cargo build --all` inside every worktree during daily iteration.

**Merge discipline:** All integration branches **must** be merged into `main` via a GitHub Pull Request — never by local `git merge` directly to `main`. This applies regardless of whether the change is agent-authored or human-authored. Rationale: this is a public open-source repo; PRs provide review trail, CI gate, and merge commit provenance that local merges cannot.

## Versioning Policy

- Schema contracts use `schema_version` field aligned with bundle envelope
- CLI / daemon SemVer must reflect breaking wire changes
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- npm and Rust workspace versions may differ; `schema_version` is the cross-language lock

## Pre-release Development (Version < 1.0)

Breaking changes are expected and allowed — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period. Local persistence may be wiped rather than migrated. After first release, follow SemVer.

## Constraints & Pitfalls

- **Do not treat the daemon runtime as an ACP Agent/Server** — it's a local supervisor, client-only
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript

## TypeScript Contract Package (cross-repo)

`nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock. **No handwritten second DTO source** in platform — all wire types come from this repo's schemas.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **nexus** (19696 symbols, 51713 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/nexus/context` | Codebase overview, check index freshness |
| `gitnexus://repo/nexus/clusters` | All functional areas |
| `gitnexus://repo/nexus/processes` | All execution flows |
| `gitnexus://repo/nexus/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
