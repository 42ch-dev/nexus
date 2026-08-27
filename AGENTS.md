# Nexus AGENTS.md

This file provides decision rules, invariants, and indexes for agents working in the **nexus** open-source monorepo.
Domain-specific rules live in subdirectory AGENTS.md files listed below.

## Repository Identity

This is the **public open-source monorepo** containing `nexus42` CLI (Rust, with integrated daemon runtime), JSON Schema wire contracts (truth source for TypeScript/Rust codegen), and published package `@42ch/nexus-contracts` (npm). Rust `nexus-contracts` crate is monorepo-internal only.

- `STRATEGY.md` — project vision, guiding principles, technology direction
- `CONCEPTS.md` — core domain vocabulary for Nexus OSS

**Not in this repo:** `nexus-platform` (private TypeScript monorepo for web/API/services) — do not reference its tech stack here.

**Harness coordination:** Shared results are [`.mstar/knowledge/`](.mstar/knowledge/), [`.mstar/specs/`](.mstar/specs/), and [`.mstar/AGENTS.md`](.mstar/AGENTS.md). Delivery state is local process (gitignored) — not clone SSOT. Runtime: upstream **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** `mstar-*` skills.

## Morning Star harness (layering)

This repo is a **consumer** of Morning Star, not the harness maintenance repo.

| Layer | File | Holds |
|-------|------|-------|
| Project | Root `AGENTS.md` (this file) | Repo identity, tech stack, build/test policy, git hygiene, crate index |
| Harness | [`.mstar/AGENTS.md`](.mstar/AGENTS.md) | Process vs results; `specs/` / `knowledge/` / `docs/` / `schemas/` boundaries |
| Runtime | `mstar-*` skills | State machine, phase gates, dispatch, QC/QA, SDD, iteration |

**Do not** duplicate `mstar-*` runtime rules here. **Do not** put plan progress, residual detail, or QC conclusions in this file — share durable outcomes via `.mstar/knowledge/` and `.mstar/specs/`.

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
| `tooling/design-tokens/` | Shared `@nexus/design-tokens` Tailwind preset + tokens.css | [`tooling/design-tokens/AGENTS.md`](tooling/design-tokens/AGENTS.md) |
| `apps/nexus42/` | CLI executable (polyglot product-surfaces dir) | [`apps/nexus42/AGENTS.md`](apps/nexus42/AGENTS.md) |
| `apps/web/` | Web SPA — Control Room + canvas (daemon-served React) | [`apps/web/AGENTS.md`](apps/web/AGENTS.md) |
| `apps/desktop/` | Tauri v2 desktop shell wrapping `apps/web` | [`apps/desktop/AGENTS.md`](apps/desktop/AGENTS.md) |
| `apps/design-studio/` | Design-system gallery (daemon-independent Vite SPA) | [`apps/design-studio/AGENTS.md`](apps/design-studio/AGENTS.md) |
| `crates/nexus-acp-host/` | ACP client adapter | [`crates/nexus-acp-host/AGENTS.md`](crates/nexus-acp-host/AGENTS.md) |
| `crates/nexus-agent-host/` | Agent host adapter | [`crates/nexus-agent-host/AGENTS.md`](crates/nexus-agent-host/AGENTS.md) |
| `crates/nexus-contracts/` | Generated Rust wire types | [`crates/nexus-contracts/AGENTS.md`](crates/nexus-contracts/AGENTS.md) |
| `crates/nexus-daemon-runtime/` | Daemon runtime (local-only) | [`crates/nexus-daemon-runtime/AGENTS.md`](crates/nexus-daemon-runtime/AGENTS.md) |
| `crates/nexus-home-layout/` | `~/.nexus42/` path layout | [`crates/nexus-home-layout/AGENTS.md`](crates/nexus-home-layout/AGENTS.md) |
| `crates/nexus-local-db/` | Local database layer | [`crates/nexus-local-db/AGENTS.md`](crates/nexus-local-db/AGENTS.md) |
| `crates/nexus-orchestration/` | Orchestration engine | [`crates/nexus-orchestration/AGENTS.md`](crates/nexus-orchestration/AGENTS.md) |
| `crates/nexus-spoke-adapter/` | SPOKE boundary — extensions.nexus accessors + spoke-operations delegation | [`crates/nexus-spoke-adapter/AGENTS.md`](crates/nexus-spoke-adapter/AGENTS.md) |
| `crates/nexus-cloud-sync/` | Cloud sync transport | [`crates/nexus-cloud-sync/AGENTS.md`](crates/nexus-cloud-sync/AGENTS.md) |
| `crates/nexus-creator/` | Creator aggregate + local identity | [`crates/nexus-creator/AGENTS.md`](crates/nexus-creator/AGENTS.md) |
| `crates/nexus-creator-memory/` | Memory pipeline, SOUL I/O | [`crates/nexus-creator-memory/AGENTS.md`](crates/nexus-creator-memory/AGENTS.md) |
| `crates/nexus-knowledge/` | Knowledge entries (World KB + User) + reference sources | [`crates/nexus-knowledge/AGENTS.md`](crates/nexus-knowledge/AGENTS.md) |
| `crates/nexus-narrative/` | Worlds, forks, timelines, manuscripts | [`crates/nexus-narrative/AGENTS.md`](crates/nexus-narrative/AGENTS.md) |
| `crates/nexus-cloud-domain/` | User + pairing (cloud sync domain) | [`crates/nexus-cloud-domain/AGENTS.md`](crates/nexus-cloud-domain/AGENTS.md) |
| `crates/nexus-moment-context-assembly/` | Per-moment context assembly | [`crates/nexus-moment-context-assembly/AGENTS.md`](crates/nexus-moment-context-assembly/AGENTS.md) |
| `.mstar/` | Harness infrastructure | [`.mstar/AGENTS.md`](.mstar/AGENTS.md) |
| `.agents/` | Code-agent skills only (ACP workspace skill root) | [`.agents/AGENTS.md`](.agents/AGENTS.md) |

**`apps/` is the polyglot product-surfaces directory.** Any product surface — regardless of language (Rust CLI, Tauri desktop shell, web SPA, etc.) — lives under `apps/`. Reusable Rust libraries live under `crates/`. See [`apps/AGENTS.md`](apps/AGENTS.md) for the durable placement rule.

**Directory split:** `{HARNESS_DIR}` = `.mstar/`. `.agents/` holds optional `.agents/skills/` for IDE/ACP — not harness SSOT.

**New crate policy:** when adding a new package or crate to the monorepo, create an `AGENTS.md` in that directory — even if minimal — documenting its purpose, key rules, and dependencies.

## UI Component Policy (Studio-first)

UI work in this repo follows a **studio-first** routing rule. The visual proving ground is `apps/design-studio` (daemon-free Vite gallery); reusable presentational primitives live in `packages/nexus-ui`. Agents must **not** land a new visual system directly in `apps/web` and call it done — that is how V1.122/V1.123 Timeline visuals ended up with tokens in `@nexus/design-tokens` and implementations in `apps/web`, but no Studio gallery and no `@42ch/nexus-ui` representation. Visuals are tuned in Studio first; componentize so promotion stays cheap.

### Decision rule

| If the UI work… | Land it in… | Then |
|-----------------|-------------|------|
| Has any visual surface to review (new node, surface, layer, state, or token in use) | **`apps/design-studio`** — presentational fixture under `src/fixtures/` or `src/pages/surfaces.tsx`, light + dark, all variants | Visual acceptance here, before App wiring |
| Is reusable across `apps/web` + Studio (or future external consumers) **and** pure presentational (no daemon / routing / product state) | **`packages/nexus-ui`** | Promote **after** Studio acceptance; record in plan/spec promotion list |
| Is coupled to daemon / product state / app routing | **`apps/web/src/components/**`** | Mirror in Studio via a `@web-*` presentational extract alias when visual review is needed |

### Workflow

1. **Studio fixture first** — for any new UI surface, visual variant, or token-consuming component, add (or extend) a fixture in `apps/design-studio` that renders it in both themes with all variants visible. No App wiring claim before the fixture exists.
2. **Componentize by default** — extract reusable presentational pieces into `@42ch/nexus-ui` rather than leaving them inline. "App-only for now" drifts; promotion is cheap, refactor-out is not. When in doubt, extract.
3. **Tokens need a gallery** — new `--color-*` tokens landed in `tooling/design-tokens/src/tokens.css` must also appear in Studio's Tokens gallery in the same iteration. A token that exists in CSS but is not visible in Studio is a defect — file a residual.
4. **Promotion requires a plan entry** — every primitive promotion into `@42ch/nexus-ui` is recorded in the active plan/spec's promotion list (see [`packages/nexus-ui/AGENTS.md`](packages/nexus-ui/AGENTS.md)). Do not silently promote.
5. **App integration last** — once Studio visuals are accepted, wire real data/behavior in `apps/web` via a thin re-export wrapper (promoted primitive) or the `@web-*` alias (kept app-local).

### Anti-patterns

- Landing a new visual system (Timeline, Layer switcher, Story beats, Canvas surfaces, World/Work/Global Timeline, etc.) directly in `apps/web` with no Studio fixture.
- Adding a token to `tokens.css` without showing it in Studio's Tokens gallery.
- Promoting a primitive to `@42ch/nexus-ui` without a plan/spec entry, or before Studio visual acceptance.
- Treating "App needs it now" as a reason to skip Studio — Studio fixtures are cheap; visual rework against wired App data is not.

### Authority

- Studio boundaries + `@web-*` aliases: [`apps/design-studio/AGENTS.md`](apps/design-studio/AGENTS.md)
- Promotion rules + package boundary: [`packages/nexus-ui/AGENTS.md`](packages/nexus-ui/AGENTS.md)
- Canonical workflow + classification labels (`promoted primitive` / `studio-local fixture` / `web-only wrapper` / `future web product component`): [`.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md`](.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md)
- Studio spec: [`.mstar/specs/design-studio.md`](.mstar/specs/design-studio.md)

## Development Policy

**Formatting:** `cargo fmt` must use a **pinned nightly** toolchain so local matches CI exactly (rustfmt formatting rules drift across nightly versions; CI's `Rust fmt & clippy` job pins `FMT_NIGHTLY` in `.github/workflows/ci.yml`). Current pin: **`nightly-2026-06-26`**. Install + use it: `rustup toolchain install nightly-2026-06-26 --component rustfmt` then `cargo +nightly-2026-06-26 fmt --all` (and `--check` to verify). Stable `cargo fmt` ignores `.rustfmt.toml`'s `ignore` field and will **incorrectly reformat** generated code under `crates/nexus-contracts/src/generated/`. When bumping the pin, update both CI and this line.

**Clippy:** Workspace-level config in root `Cargo.toml` enables `pedantic` + `nursery` as `warn`, inherited by all crates. CI runs `cargo clippy --all -- -D warnings`. When fixing clippy errors, auto-fix first (`cargo clippy --fix --allow-dirty --allow-staged`), then handle residual manually. **Do not suppress** with `#[allow(...)]` without a brief justification comment. **Do not change runtime behavior** when fixing lint errors.

**TypeScript / Oxlint:** Workspace TypeScript packages pin **`typescript@7.0.2`**. Lint SSOT is root **Oxlint** (`.oxlintrc.json`, type-aware via `oxlint-tsgolint`) — run `pnpm lint` locally; CI `typescript-checks` runs it alongside `pnpm run typecheck`. **`pnpm run lint` must exit 0 with zero warnings** (warning-clean is enforced in CI). No ESLint in this repo.

**Rust `target/` disk hygiene:** `target/debug` is gitignored but grows without bound on macOS/Linux when the workspace is rebuilt often. Stale `.o` files under `target/debug/deps` and old `target/debug/incremental/*` hashes (e.g. after `pnpm run codegen`, crate renames, or repeated `cargo * --all`) are the usual cause — not a single bug. CI uses ephemeral runners + `rust-cache`; **local developers and agents must not mirror CI’s `--all` cadence during iteration.**

**Preferred layout — repo [`.envrc`](.envrc) + [direnv](https://direnv.net/):** this is the supported way to relocate and share the Rust build cache. It exports `CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"` **only inside this repository**, so the main checkout and every `.worktrees/*` worktree reuse one cache instead of each growing a local `target/`.

```bash
# After clone or `git worktree add` (once per checkout root):
direnv allow
# Confirm cargo sees the shared dir (should print under ~/.cache/nexus-target):
cargo metadata --no-deps --format-version 1 | jq -r .target_directory
```

Without direnv, export the same variable for the shell session. **Do not** set `build.target-dir` in root `Cargo.toml` (not a valid package/workspace key), project `.cargo/config.toml` (cannot expand `$HOME` / XDG; absolute paths are not portable for this OSS repo), or user-level `~/.cargo/config.toml` (pollutes every Rust project on the machine). Env (`CARGO_TARGET_DIR`) already overrides config when both are set — keep the single SSOT in [`.envrc`](.envrc).

Workspace `Cargo.toml` keeps the `dev` profile lean: `debug = 1` (line-limited) for workspace crates, `debug = false` for non-member dependencies, and `split-debuginfo = "unpacked"`. That reduces **per-artifact** size; it does **not** remove orphan hashes — still use scoped builds + sweep/clean below.

| Phase | Command scope |
|-------|----------------|
| **Daily iteration** (default) | `cargo check -p <crate>`, `cargo test -p <crate>`, `cargo clippy -p <crate> -- -D warnings` for the crate you are editing |
| **Pre-commit / gate** | `cargo clippy --all -- -D warnings`, `cargo test --all` (matches CI) |
| **After codegen or large contract/workspace graph changes** | Prefer `cargo clean` once, then rebuild scoped or `--all` as needed — avoids piling orphan artifacts (including legacy `nexus42d` names) |

**Cleanup (repo root; with direnv this is `$CARGO_TARGET_DIR` → `~/.cache/nexus-target`):**

- **Reclaim disk immediately:** `cargo clean` (next full build is slow; expected). If it errors on `target/debug/incremental` (“Directory not empty”), remove the heavy subtrees then retry: `rm -rf "$CARGO_TARGET_DIR"/debug/{deps,incremental}` && `cargo clean` (fallback: `target/debug/...` only if direnv/`CARGO_TARGET_DIR` is unset).
- **Periodic maintenance (recommended every ~5 iterations or monthly):** `cargo install cargo-sweep` once, then from the repo root (direnv on so `CARGO_TARGET_DIR` is set):

```bash
# Drop artifacts built by toolchains no longer installed via rustup
cargo sweep --installed
# Drop artifacts unused for 30+ days (incremental + old dep hashes)
cargo sweep --time 30
```

  Optional dry-run: append `-d`. Do **not** use `cargo sweep -i N` for age-based cleanup — `-i` is `--installed` (boolean); age uses `--time` / `-t`.
- **When to clean:** `$CARGO_TARGET_DIR/debug` (or local `target/debug` if unset) over ~50 GiB, filesystem slowness under the target dir, end of a large plan slice, or after deleting/renaming crates.

**Anti-patterns:** Building without `CARGO_TARGET_DIR` / direnv (fills a per-checkout `target/` and breaks worktree sharing); running `cargo test --all` / `cargo clippy --all` on every small edit; skipping cleanup for months while agents run full-workspace builds; treating `target/` bloat as safe to commit (it is always gitignored — clean locally instead).

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
| Iteration / hotfix landing | GitHub PR → `target_branch` only (never local `git merge` onto the protected branch). **Merge method by PR commit count** (commits on the PR head vs base): **≤30 → merge commit** (`gh pr merge --merge`); **>30 → squash** (`gh pr merge --squash`). Rationale: harness process noise stays local, so most PRs stay small enough for a merge commit; squash only when the history is too long to keep. |
| Harness **results** | Commit `.mstar/knowledge/`, `.mstar/specs/`, `.mstar/AGENTS.md` when shared; do **not** commit ignored process under `.mstar/` |
| Codegen | Schema changes and generated output in the **same commit** (see [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md)) |
| Never commit | `target/`, `.worktrees/`, `node_modules/`, `.mstar` process paths above (gitignored — agents must self-check) |

Harness process paths are **local** (see [`.mstar/AGENTS.md`](.mstar/AGENTS.md)).

**Periodic maintenance (monthly or every ~5 iterations):**

```bash
git count-objects -vH
git maintenance run --task=gc --task=incremental-repack
cargo sweep --installed   # requires cargo-sweep; drop uninstalled-toolchain artifacts
cargo sweep --time 30     # drop artifacts unused for 30+ days
```

If `.git` exceeds ~100 MiB or clone slows again: consider `git filter-repo` or an orphan history squash (solo maintainer only; see team before force-push on a shared default branch).

**Anti-patterns:** committing ignored `.mstar/` process paths; per-worktree `target/` without cleanup; developer clone with `--no-recurse-submodules` and no follow-up `submodule update`; `cargo build --all` inside every worktree during daily iteration.

**Merge discipline:** All PRs to the protected branch (`target_branch`, usually `main`) land via **GitHub PR** only; never local `git merge` onto the protected branch. Merge method by **PR commit count** (head vs base): **≤30 → merge commit**; **>30 → squash**. Branch naming → upstream `mstar-iteration` / `mstar-branch-worktree`.

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
