# Nexus AGENTS.md

This file provides decision rules, invariants, and indexes for agents working in the **nexus** open-source monorepo.
Domain-specific rules live in subdirectory AGENTS.md files listed below.

## Repository Identity

This is the **public open-source monorepo** containing the `nexus42` CLI and the headless `nexus-runtime` Connect host (both Rust), JSON Schema wire contracts (truth source for TypeScript/Rust codegen), and published package `@42ch/nexus-contracts` (npm). Rust `nexus-contracts` crate is monorepo-internal only.

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

- **CLI/runtime:** Rust-first (aligns with ACP official SDK availability)
- **Protocol:** ACP-first, skills-second — CLI is an ACP client, not an ACP agent/server
- **Wire format:** JSON Schema as truth source — generates both TypeScript and Rust types

## Key Naming (Frozen)

- Product: **Nexus**
- CLI executable: **nexus42**
- Headless runtime: **`nexus-runtime`** (Connect-only binary of the same crate as the CLI). The integrated `nexus42 daemon` group and the `nexus-daemon-runtime` composition were retired in v1.193 P2 — no `nexus42d`, no `nexus42 service` alias, no CLI service launcher
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
| `apps/web/` | Web SPA — Control Room + canvas (React; served by the Electron/TS host) | [`apps/web/AGENTS.md`](apps/web/AGENTS.md) |
| `apps/desktop-electron/` | Electron desktop host (unsigned macOS arm64 + x64) wrapping `apps/web` | [`apps/desktop-electron/AGENTS.md`](apps/desktop-electron/AGENTS.md) |
| `apps/design-studio/` | Design-system gallery (daemon-independent Vite SPA) | [`apps/design-studio/AGENTS.md`](apps/design-studio/AGENTS.md) |
| `crates/nexus-acp-host/` | ACP client adapter | [`crates/nexus-acp-host/AGENTS.md`](crates/nexus-acp-host/AGENTS.md) |
| `crates/nexus-agent-host/` | Agent host adapter | [`crates/nexus-agent-host/AGENTS.md`](crates/nexus-agent-host/AGENTS.md) |
| `crates/nexus-contracts/` | Generated Rust wire types | [`crates/nexus-contracts/AGENTS.md`](crates/nexus-contracts/AGENTS.md) |
| `crates/nexus-embedding/` | Embedding readiness contract (RN-OGA-3) — provider trait seam, identity tuple, fail-closed derived-index protocol; no OSS execution | [`crates/nexus-embedding/AGENTS.md`](crates/nexus-embedding/AGENTS.md) |
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

**`apps/` is the polyglot product-surfaces directory.** Any product surface — regardless of language (Rust CLI, Electron desktop host, web SPA, etc.) — lives under `apps/`. Reusable Rust libraries live under `crates/`. See [`apps/AGENTS.md`](apps/AGENTS.md) for the durable placement rule.

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

**Preferred layout — repo [`.envrc`](.envrc) + [direnv](https://direnv.net/):** this is the supported way to relocate and share the Rust build cache. The `.envrc` auto-detects context: the **main checkout** and the **integration worktree** share the canonical `~/.cache/nexus-target` (no suffix); each **feature worktree** (`.worktrees/v1190-*`) gets an **isolated** `~/.cache/nexus-target-<dirname>` that is cleaned after merge.

```bash
# After clone or `git worktree add` (once per checkout root):
direnv allow
# Confirm cargo sees the correct scoped dir:
#
#   main checkout:            ~/.cache/nexus-target
#   integration worktree:     ~/.cache/nexus-target
#   feature worktree <name>:  ~/.cache/nexus-target-<name>
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
**Merge gate — feature-branch target cleanup (HARD):** before merging a feature branch into the integration branch, that feature's scoped `CARGO_TARGET_DIR` **must be removed** — a feature target never outlives its own merge. With the scoped `.envrc` layout the removal is precise, by exact name, and never a wildcard: `rm -rf ~/.cache/nexus-target-*` would delete a peer's target while that peer is still building. This is not optional housekeeping — parallel worktree target dirs compound silently (v1.190: six concurrent targets consumed 98 GiB of `/tmp` in a single iteration) and degraded the host. The recorded conclusion is **immediate per-slice reclamation, not a cap on concurrent development**. Concretely:

```bash
# Before merging: remove THIS feature's scoped build cache (exact name only)
rm -rf ~/.cache/nexus-target-<dirname>
```

Then reclaim the worktree itself — non-forcibly, after the reviewed merge and the ownership release. See **Worktrees** below and [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) → **Worktree lifecycle and reclamation** for the full ordering and the two measured non-forced refusal routes.

Integration verification (`cargo check --workspace`) runs from the integration worktree with the canonical `~/.cache/nexus-target` — it does not depend on any feature's cache.

**Quick stats:** `du -sh ~/.cache/nexus-target-*` shows every feature's cache size at a glance.


- **Cleanup (repo root; with direnv this is `$CARGO_TARGET_DIR` → `~/.cache/nexus-target`):**
- **Anti-patterns:** Building without `CARGO_TARGET_DIR` / direnv (fills a per-checkout `target/` and breaks worktree sharing); running `cargo test --all` / `cargo clippy --all` on every small edit; skipping cleanup for months while agents run full-workspace builds; treating `target/` bloat as safe to commit (it is always gitignored — clean locally instead); merging a feature branch without cleaning its scoped target dir first; storing feature target dirs inside the worktree itself (they belong in `~/.cache/nexus-target-<name>` for centralized cleanup and statistics).

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
| Developer clone | **Full** init | `--recurse-submodules` at clone; `git submodule update --init --recursive` after a pull only when the skill dirs are empty |
| New worktree | **Full** init | **Required** after every `git worktree add`: run the checked-in initializer (see **Worktrees** below). The raw `git submodule update --init --recursive` is not an alternative step — it is only what the initializer runs internally for submodules that are still missing |
| CI default jobs | **Off** | `actions/checkout` without `submodules: true` (Rust/TS builds do not read skills) |
| CI job needing skills | **On demand** | Add `submodules: true` only when the job touches `.agents/skills/` |

Running the raw command in place of the initializer is not equivalent: the initializer also validates already-initialized metadata, refuses a copied or out-of-subtree gitdir and an unmerged index state, and reports a deliberately different submodule HEAD instead of resetting it.

**Worktrees:**

- Path: `.worktrees/<name>/` only (`.worktrees/` is gitignored). Feature tracks use feature names; the **integration worktree** is `.worktrees/iteration-<id>/` (currently `.worktrees/iteration-v1.197/` on `iteration/v1.197`). Integration is a merge and final-verification location, **not** a development slot: no feature is developed there, and it does not build in parallel with feature writers.
- Share the main repo object store — worktrees do not re-download packs; slowness is checkout (~4k files), not network.
- After **every** `git worktree add`, initialize that checkout's submodules with the checked-in initializer — never by copying `.git` metadata:

  ```bash
  node scripts/init-worktree-submodules.mjs --worktree "$PWD/.worktrees/<name>"
  ```

  It runs native `git submodule update --init --recursive` for the submodules that are missing and validates the ones that already exist, so a repeated call keeps each submodule's own gitdir, index, config and HEAD (an intentional pin difference is reported, never reset) and shares nothing with main. Every initialized path is validated first, and the command writes exactly one JSON object to stdout with exit `0` (valid or initialized), `1` (Git/safety refusal — state unchanged, reason on stderr) or `2` (invalid invocation). Keep full recursive submodules; never disable recursion globally.
- **Size concurrency by resources, not by a fixed count.** Recompute each iteration:

  `K = min(ready independent tasks, floor(disk budget / per-track target estimate), max(1, cores / 2))`

  and round the available tracks down; zero ready tasks means zero development tracks. Measured 2026-09-25 example: 10 cores, 32 GiB RAM, a ~120 GiB budget and 20 GiB per track gave **K=2** with two ready plans and K=4 once four independent tasks were ready — the number follows ready work and its dependencies and is **not** a standing worktree limit.
- **Watermark gate before another track opens:** root free ≥ **90 GiB** and total feature targets ≤ **120 GiB**. If either fails, reclaim and re-measure before scheduling — a full disk is answered by reclamation, never by refusing concurrency.
- Activate each feature's `.envrc`/direnv and confirm the isolated cache before building:

  | Checkout | `CARGO_TARGET_DIR` |
  |----------|--------------------|
  | main / integration worktree | `${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target` — canonical, shared, and **never** a feature's removal target |
  | feature worktree `<name>` | `${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target-<dirname>` |

- **Reclaim inside your own slice.** Remove only the owned feature target before merging (with its temporary build products, in the same task/track slice); after the reviewed merge and the lawful release, remove that owned worktree non-forcibly and prune safely. Do not park a finished track for a later task. Six concurrent targets once consumed 98 GiB — that mandates immediate reclamation, not suppressed concurrency.
- **Exit gates.** Each completed slice proves its own worktree/target/temp footprint is absent with raw command output; failed reclamation fails that exit gate. At final development convergence, when no peer feature remains, `git worktree list` shows only main + integration; after authorized integration cleanup, only main.
- **Sweeper checkpoints (PM).** At every reschedule the PM runs `node scripts/worktree-sweep.mjs` against the current iteration's active-plan inventory. A dry-run proposal is evidence, never authorization to delete. Exact argv, checks and guard semantics → [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) → **Worktree lifecycle and reclamation**.
- **Guards are not obstacles.** Active, dirty and unmerged refusals exist to protect other people's work — resolve the fact behind them, never force past them. `git worktree remove --force` is not an accepted cleanup route, a wildcard `rm -rf` is never a cleanup route, and `git submodule deinit` is not a cleanup step (it unregisters the submodule in the shared superproject config). For the two measured non-forced refusals, use the documented exact-path route in the lifecycle checklist.
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
- CLI / runtime SemVer must reflect breaking wire changes
- `@42ch/nexus-contracts` major bump → coordinated update across CLI + platform API + npm package
- npm and Rust workspace versions may differ; `schema_version` is the cross-language lock

## Pre-release Development (Version < 1.0)

Breaking changes are expected and allowed — API shapes, CLI flags, on-disk paths, config file layout, and behavior may change without a deprecation period. Local persistence may be wiped rather than migrated. After first release, follow SemVer.

## Constraints & Pitfalls

- **Do not treat the Electron-owned TS service as an ACP Agent/Server** — it is the local HTTP host for browser/desktop and hosts the provider adapters; Nexus' own surfaces are ACP clients (the retired `nexus42 daemon` group is not an ACP server either)
- **Do not sync full manuscript text by default** — only structured deltas/bundles
- **World history is immutable** — changes go through Fork, not in-place mutation
- **Wire contracts must match schemas** — no drift between `schemas/` and generated types
- **Single truth source for DTOs** — avoid parallel handwritten types in Rust or TypeScript

## TypeScript Contract Package (cross-repo)

`nexus-platform` (private repo) consumes `@42ch/nexus-contracts` via npm semver lock. **No handwritten second DTO source** in platform — all wire types come from this repo's schemas.
