# Contributing to Nexus

Thank you for helping improve Nexus. This document is the contributor guide: setup, day-to-day workflows, and the pre-PR checklist that mirrors CI.

- **Command cheat sheet** (dev servers, builds, tests): root [`README.md`](../README.md) → **Development**
- **Repository layout, naming, agent rules**: root [`AGENTS.md`](../AGENTS.md)
- **Codegen details**: [`docs/CODEGEN.md`](CODEGEN.md)

## Code of conduct

This project follows the [Contributor Covenant Code of Conduct v2.1](../.github/CODE_OF_CONDUCT.md), which defines expected behavior, the enforcement ladder, and how to report unacceptable behavior privately.

## Prerequisites

- **Node.js** 22 or newer (`engines.node` in root `package.json`)
- **pnpm** 11 or newer (CI uses pnpm 11)
- **Rust** stable with `clippy` (and `rustfmt` component on stable is not sufficient — see below)
- **Pinned nightly `rustfmt`** — required so local formatting matches CI. Current pin: **`nightly-2026-06-26`** (see `FMT_NIGHTLY` in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml)).

  ```bash
  rustup toolchain install nightly-2026-06-26 --component rustfmt
  ```

  Stable `cargo fmt` ignores workspace `.rustfmt.toml` `ignore` rules and can incorrectly reformat generated code under `crates/nexus-contracts/src/generated/`.

- **Optional — desktop shell:** macOS when working on `apps/desktop-electron` (Electron host; unsigned arm64/x64 packaging)
- **Optional — WASM host crate:** `rustup target add wasm32-unknown-unknown` when touching `nexus-wasm-host`

## Getting started

```bash
git clone --filter=blob:none --recurse-submodules https://github.com/42ch/nexus.git
cd nexus
pnpm install --frozen-lockfile
```

`--recurse-submodules` initializes [`.agents/skills/`](../.agents/skills/) (ACP skill root). After a plain clone or pull, if skill dirs are empty:

```bash
git submodule update --init --recursive
```

### Shared Rust build cache (recommended)

To share `target/` across checkouts and worktrees, use [direnv](https://direnv.net/) with the repo-root [`.envrc`](../.envrc):

```bash
direnv allow
```

Without direnv, for the current shell session:

```bash
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"
```

Do **not** set `target-dir` in `~/.cargo/config.toml` — that applies to every Rust project on the machine.

### Worktrees

Feature work must live in a linked worktree under `.worktrees/<name>/`. The integration checkout is `.worktrees/iteration-<id>/` (on an `iteration/<id>` branch) and is reserved for merges and final integration verification — it is not a feature development slot.

After every `git worktree add`, initialize that checkout's submodules with the checked-in initializer rather than copying `.git` metadata:

```bash
git worktree add .worktrees/<name> -b feat/<name>
node scripts/init-worktree-submodules.mjs --worktree "$PWD/.worktrees/<name>"
```

The initializer runs native `git submodule update --init --recursive` for the submodules that are missing and validates the ones that are already initialized, so a repeated call is a validated no-op: each submodule keeps its own administrative directory, index, config and HEAD, and an intentional pin difference is reported rather than reset. It writes exactly one JSON object to stdout and exits `0` (checkout valid, submodules validated or initialized), `1` (Git, safety or operational refusal — the reason is on stderr, and the refusal object's `code` says what the failure left behind: `init.refuse.preflight` attempted no mutation, so the checkout is exactly as it was, while `init.refuse.partial` means native Git had already initialized at least one submodule — initialization is not transactional, nothing is rolled back, and `initialized_paths` names the submodule paths that are initialized at that moment, re-observed read-only: `[]` when none completed, `null` when the checkout could not be re-read; re-run to validate that subset and finish the rest) or `2` (invalid invocation). Anything that is not a linked checkout directly under `<main>/.worktrees/<name>` — the main checkout, a plain directory, a copied `.git` pointer, a symlink escape — is refused, never repaired.

Then activate the checkout's [`.envrc`](../.envrc) (`direnv allow`) and confirm the scoped cache:

```bash
cargo metadata --no-deps --format-version 1 | jq -r .target_directory
```

The main checkout and the integration checkout share the canonical `${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target`; a feature worktree gets the isolated `${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target-<name>`. See [Worktree lifecycle and reclamation](#worktree-lifecycle-and-reclamation) for the concurrency budget and the exit checklist.

## Day-to-day development

Root [`package.json`](../package.json) exposes shortcuts for common tasks. Run from the repository root.

| Task | Command |
|------|---------|
| CLI + web dev (one command) | `pnpm run dev` → validates the prepared `nexus42` artifact, starts or attaches the standalone TS service on 127.0.0.1:8420 (detached when it is not already running), then runs the Vite dev server in the foreground (`scripts/dev-cli-web.sh`). It never runs Cargo |
| Desktop dev | `pnpm run dev:desktop` (Electron host over the built web dist) · `pnpm run dev:desktop:web` (Vite HMR + host) |
| TS workspaces build | `pnpm run build` (all TS workspaces; desktop packaging is a separate command) |
| Web / Studio build | `pnpm run build:web`, `pnpm run build:design-studio` |
| Desktop bundle | `pnpm run build:desktop -- --arch <arch>` (unsigned Electron packaging; see below) |
| CLI build | `pnpm run build:cli` or `pnpm run build:cli:release` |
| TS tests | `pnpm run test`, or `pnpm run test:web` / `pnpm run test:design-studio` |
| TS typecheck | `pnpm run typecheck` |
| Schema validate + codegen | `pnpm run validate-schemas`, `pnpm run codegen` |

Build individual npm packages when needed:

```bash
pnpm -F @42ch/nexus-contracts build
pnpm -F @42ch/nexus-ui build
```

The dev shortcut runs the **standalone TypeScript service** (`node apps/nexus-service/dist/main.js --home <home> --host 127.0.0.1 --port <port>`, started detached by `scripts/dev-cli-web.sh` when the endpoint is idle). It requires an already-built native addon plus a compatible `nexus42` artifact and manifest, and it never runs Cargo: a missing or incompatible artifact fails fast with `pnpm dev:backend:refresh`. The retired `nexus42 daemon` composition is gone, so no CLI command starts, stops, statuses or proxies the service. `pnpm run dev:backend:refresh` is the only ordinary DX path that runs Cargo or codegen, and only after Rust or contract edits; frontend-only and TS route/provider edits with an unchanged native/schema contract run zero Cargo. To exercise the whole public path against a real `dsh` runtime — Creator/workspace setup, one admitted workflow, the authorized workspace effect, cancel and restart — use the root [README Quick Start](../README.md#quick-start).

### Iteration vs pre-PR scope

During daily work, prefer **scoped** commands for the crate or app you are editing:

```bash
cargo check -p <crate>
cargo test -p <crate>
cargo clippy -p <crate> -- -D warnings
```

Before opening a PR, run the **full** gates in [Local checks (mirror CI)](#local-checks-mirror-ci) below (`cargo clippy --all`, `cargo test --all`, workspace `pnpm run typecheck`, etc.).

See [`AGENTS.md`](../AGENTS.md) for `target/` disk hygiene and when to run `cargo clean`.

### Desktop packaging (Electron)

The desktop host lives in [`apps/desktop-electron`](../apps/desktop-electron). It produces **unsigned** macOS `.app` / `.dmg` for arm64 and x64 — no signing, notarization, or auto-update lane.

```bash
pnpm run dev:desktop                     # Electron host over the built web dist
pnpm run dev:desktop:web                 # Vite HMR + Electron host
pnpm run build:desktop -- --arch arm64   # native arch is the default
```

`nexus42 desktop bundle --arch <arch>` delegates to the same driver. Dev requires a prepared native payload; the package driver fails closed when compiled prerequisites (web dist, service build, native payload) are missing. See [`apps/desktop-electron/AGENTS.md`](../apps/desktop-electron/AGENTS.md).

## Worktree lifecycle and reclamation

A feature track is opened, measured and reclaimed inside the slice that owns it: a feature's cache and worktree are never parked until the end of an iteration. Reclamation is immediate and scoped, and it is never a reason to serialize independent work — in v1.190 six concurrent targets consumed 98 GiB under `/tmp` in a single iteration and degraded the host, so each slice reclaims its own footprint as soon as it is done with it.

### Sizing concurrency (resource budget)

```
K = min(ready independent tasks, floor(disk budget / per-track target estimate), max(1, cores / 2))
```

Round the available tracks down; zero ready tasks means zero development tracks. The measured 2026-09-25 example — 10 cores, 32 GiB RAM, a ~120 GiB disk budget and 20 GiB per track — gave `K=2` with two ready plans and `K=4` once four independent tasks were ready, so the number follows ready work and its dependencies, not a standing worktree limit.

Two watermarks gate every new track:

| Watermark | Threshold |
|-----------|-----------|
| Root filesystem free space | ≥ 90 GiB |
| Sum of all feature target directories | ≤ 120 GiB |

If either fails, reclaim first and re-measure before scheduling. A failing watermark is never a reason to refuse concurrency.

### Ownership receipt (the inventory)

The sweeper is driven by a version-1 inventory: a non-authoritative ownership receipt for one scheduling checkpoint. It is read-only input — the workflow snapshot and real Git facts stay the authority for what is claimed and protected, and the receipt never authorizes a deletion.

```json
{
  "version": 1,
  "workflow_id": "<iteration-id>",
  "active_plan_ids": ["<plan-id>"],
  "scheduling": {
    "ready_independent_tasks": 2,
    "disk_budget_bytes": 128849018880,
    "per_track_target_estimate_bytes": 21474836480
  },
  "tracks": [{
    "track_id": "<track-id>",
    "plan_id": "<plan-id>",
    "worktree": "<absolute-repo>/.worktrees/<name>",
    "branch": "feat/<name>",
    "target": "<absolute-cache-root>/nexus-target-<name>",
    "temporary_paths": ["<absolute-temporary-build-path>"],
    "producer_stopped": true,
    "state": "completed"
  }]
}
```

`active_plan_ids` must name every claimed non-Done plan (and every leased plan) from the snapshot; a track's `worktree` must be the canonical `<repo>/.worktrees/<name>` linked checkout of this repository on its declared branch; `target` must be exactly the `.envrc`-derived directory for that checkout, and the shared canonical `nexus-target` is never a track's target; `temporary_paths` are exact absolute receipt paths, never globs; `state` is `active` or `completed`, and `producer_stopped` is a required boolean.

### Checklist

1. **Create and record.** Add the worktree under `.worktrees/<name>/` and initialize it (see [Worktrees](#worktrees)). Record the track's ownership receipt: owner/track id, the exact feature worktree path, the exact target to reclaim, and every temporary build path the task will create.
2. **Stop the producer.** No build, codegen step or agent may still be writing into the target or the temporary paths when reclamation starts; the receipt's `producer_stopped` must be true.
3. **Remove the owned feature target before the merge**, together with its temporary build products, in the same task/track slice — only that track's `nexus-target-<name>`, never the shared canonical cache.
4. **Merge under review, then release ownership.** The merge and the ownership release are proven separately, and the worktree is reclaimed only after both.
5. **Sweep first — dry run.** The sweeper proposes; without `--apply` it deletes nothing. Run it from the repository's **main** checkout (`--repo` must be the main worktree, exit `2` otherwise), and pass `--harness`, the control directory that holds `workflows/<id>/snapshot.json` for the workflow being swept:

   ```bash
   node scripts/worktree-sweep.mjs \
     --repo "$PWD" \
     --harness <absolute-control-harness-dir> \
     --workflow <iteration-id> \
     --inventory <absolute-inventory.json>
   ```

6. **Apply only for a completed, merged, released, producer-stopped track:**

   ```bash
   node scripts/worktree-sweep.mjs \
     --repo "$PWD" \
     --harness <absolute-control-harness-dir> \
     --workflow <iteration-id> \
     --inventory <absolute-inventory.json> \
     --apply
   ```

   `--apply` re-verifies every fact immediately before each mutating step, removes the exact scoped target/temporary paths itself, and hands each worktree and branch to the installed harness cleanup. Deletion is **engine-first**: that cleanup is the authority that decides whether a worktree or branch is removable, and the sweeper never deletes either itself — when the cleanup declines the branch after the fallback in step 7, the workflow-level cleanup checkpoint owns that release. The one bounded exception is the two measured engine refusals documented in step 7: only there does the checklist take the documented exact-path non-force route, and never as a general permission. The sweeper never forces, never uses a wildcard, and never runs `git submodule deinit`.
7. **The two measured non-forced obstacles.** A non-forced removal may legitimately refuse on this repository; both cases are reported, and both are reclaimed through the same documented exact-path route.
   - **A worktree that carries submodules.** `git worktree remove` refuses with `fatal: working trees containing submodules cannot be moved or removed`, and the refusal survives `git submodule deinit --all` — which unregisters the submodule in the shared superproject config and is therefore never a cleanup step here. The sweeper records the refusal verbatim and then takes the non-force route: `rm -rf <exact worktree path>` followed by `git worktree prune`, then re-observes.
   - **An ignored-only build footprint in a reviewed checkout.** A clean tracked tree can still refuse as dirty because build preparation left ignored outputs behind (measured example from one worktree: `node_modules/` 119 M, `packages/nexus-contracts/dist/` 3.4 M, `apps/nexus-service/dist/` 472 K — 718 M), so the harness cleanup answers `cleanup.refuse.dirty-worktree`. Once the owner is merged and released, the sweeper enumerates those exact ignored paths with their measured sizes, reports them, and reclaims through the same non-force route. Genuine tracked or untracked dirt stays a refusal that mutates nothing.

   Either way the route is exact-path only. **No `--force`, no wildcard `rm -rf`, no home-directory-wide deletion, and no `git submodule deinit`.** After the fallback the harness cleanup may also decline the branch candidate, because the worktree record it keyed on is now pruned; the branch is then left as explicit unreclaimed state and released from the workflow-level cleanup checkpoint — the sweeper reports it instead of force-deleting a branch.
8. **Prune — scoped.** A removal leaves the worktree record behind, so the sweeper itself runs `git worktree prune --dry-run` after one and prunes only when that would drop solely that track's own record; a stale entry belonging to something else is retained and reported. Run by hand, follow the same rule.
9. **Prove your own exit.** Verify the track's footprint is gone with raw command output:

   ```bash
   node scripts/worktree-sweep.mjs \
     --repo "$PWD" \
     --harness <absolute-control-harness-dir> \
     --workflow <iteration-id> \
     --inventory <absolute-inventory.json> \
     --check-exit <track-id>
   ```

   `--check-exit` is read-only: it asserts that the named completed track keeps no target or temporary path, no worktree, and no branch, and the track must be declared in the inventory it is given (an undeclared id is exit `2`). A declared branch that still exists is unreclaimed state.

### Checkpoints and exit codes

- **Per-slice exit** (`--check-exit <track-id>`) covers this track only. Other tracks may still be active: a live peer neither blocks your own proof nor gets touched by it.
- **Development convergence** (`--check-convergence`, read-only) requires that, with no feature track left, `git worktree list` contains main + integration only and no unclaimed footprint remains. An active peer blocks convergence — it is reported as protected, never reclaimed — but it never blocks your own exit.
- **Phase 6 close:** after the integrator's authorized integration cleanup, only main remains.

`--check-exit` and `--check-convergence` are mutually exclusive with each other and with `--apply`; combining them is an invalid invocation. Every invocation shares the same exit codes: `0` a valid dry run, a passing check or a fully reclaimed apply; `1` unreadable facts, a failing check, a failed reclamation or a requested completed track that still owns an artifact; `2` an invalid invocation or inventory. Failed reclamation fails the exit/close gate.

Guards are not obstacles: an `active`, `dirty` or `unmerged` refusal protects work, and the answer is to resolve the underlying fact (stop the writer, remove the build output, merge first) — never to force through it.

### Decisions recorded (evaluated, not adopted)

- **sccache — deferred.** It is not installed, and adopting it needs its own measured hit-rate and tooling decision first.
- **Per-track cache quota with `cargo clean -p` — deferred.** Partial cleanup does not establish a zero footprint, and crate graphs vary enough that a fixed quota would be guesswork; exact removal of a completed track's target remains the baseline.
- **Worktree pooling — deferred.** Reusing one worktree across tasks complicates private submodule state and per-track ownership, and buys nothing while two ready tracks fit the budget. Revisit only with setup-time evidence.

These are decisions with evidence gaps, not a backlog promise.

## Schema-first development

JSON Schemas under `schemas/` are the source of truth. TypeScript and Rust contract types are generated; do not hand-edit generated files.

1. Edit or add schemas in `schemas/`.
2. Validate: `pnpm run validate-schemas`
3. Regenerate: `pnpm run codegen` (also rebuilds `@42ch/nexus-contracts`)
4. Implement against generated types in `packages/nexus-contracts/` and `crates/nexus-contracts/`.
5. Add or update tests.
6. **Commit schema changes and all generated output together** so CI’s codegen check passes.

Generated paths checked in CI:

- `packages/nexus-contracts/src/generated/`
- `crates/nexus-contracts/src/generated/`

For wire-type changes, also run `bash tooling/check-wire-drift.sh` before pushing (see checklist below).

## Local checks (mirror CI)

Run these before requesting review. Order follows the main pipeline in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml). Stop and fix at the first failure.

### 1. Schemas and codegen

```bash
pnpm run validate-schemas
pnpm run codegen
git diff --exit-code packages/nexus-contracts/src/generated/ crates/nexus-contracts/src/generated/
```

### 2. Schema consistency and wire drift

```bash
bash tooling/check-schema-drift.sh
bash tooling/check-wire-drift.sh
```

### 3. Rust: format, lint, sqlx offline check, tests

Formatting uses the **pinned** nightly rustfmt:

```bash
cargo +nightly-2026-06-26 fmt --all -- --check
cargo clippy --all -- -D warnings
SQLX_OFFLINE=true cargo check --all --all-targets
SQLX_OFFLINE=true cargo test --all
```

To apply formatting locally (instead of check-only):

```bash
cargo +nightly-2026-06-26 fmt --all
```

### 4. TypeScript

`pnpm run codegen` already builds `@42ch/nexus-contracts`. Then:

```bash
pnpm run typecheck
pnpm run build
pnpm run test
```

When you only touch `apps/web` or UI guardrails:

```bash
pnpm run build:web
pnpm run test:web
bash tooling/check-ui-guardrails.sh
```

CI also runs dedicated `web` and `@42ch/nexus-ui` jobs; match those when your change is limited to those packages.

## Code style

- **Rust:** `cargo +nightly-2026-06-26 fmt --all`; `cargo clippy --all -- -D warnings`. Fix all clippy warnings. Do not add `#[allow(...)]` without a brief justification comment.
- **TypeScript:** Strict mode in package tsconfigs. Run `pnpm run typecheck` when you touch TS; run scoped app tests when you change `apps/web` or `apps/design-studio`.

## Testing expectations

- **Rust:** unit and integration tests for non-trivial behavior; `cargo test -p <crate>` while iterating, `cargo test --all` before PR.
- **TypeScript:** Vitest in `apps/web`, `apps/design-studio`, and `@42ch/nexus-ui`; `pnpm run test` or scoped `pnpm run test:web` / `pnpm run test:design-studio`.
- Prefer extending existing test patterns over adding trivial assertions.

## Branching and PRs

- Branch from `main`.
- Use clear branch names, for example `feature/<short-name>` or `fix/<short-name>`.
- Keep PRs focused: one feature or fix per PR when practical.
- Update contributor or user docs when workflows or behavior change.
- Ensure CI is green before requesting review.

### Documentation-only changes

CI ignores pushes that only touch paths such as `docs/**` or certain `README.md` / `AGENTS.md` files (see `paths-ignore` in `.github/workflows/ci.yml`). Doc-only PRs may not run the full pipeline automatically; note in the PR description if reviewers should run checks locally.

## Security and dependencies

- Prefer minimal, well-maintained dependencies.
- Do not commit secrets or machine-specific credentials.
- **Report vulnerabilities privately** — never open a public issue for a suspected vulnerability; see [`SECURITY.md`](../.github/SECURITY.md) for supported versions and the confidential reporting channel.

## Where to put documentation

- **Stable, clone-ready docs** (install, architecture, codegen, contributing): `docs/` and root `README.md` (**Development** section for maintainer commands; **Quick Start** is the clone-from-source first workflow, since no end-user install flow is published yet).
- **Plan-specific design notes and review artifacts**: `.mstar/knowledge/` — see [`AGENTS.md`](../AGENTS.md).

## Questions

Open an issue for bugs or feature discussion, or ask in the project’s preferred chat channel if one is listed in the repository README.
