# Local FS layout (creator → workspace) — SSOT lock + implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align nexus OSS CLI/daemon with v1-spec **ADR-014** (`{v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md}`) and synchronized cli-spec / local-db-schema / data-model prose: operational state under `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`, creative roots without DB, **immutable** `(creator_id, workspace_slug)` registration, **active context** via **`creator use` + `creator workspace use`** (default slug **`default`**).

**Architecture:** Introduce a small **path resolver** that maps `(creator_id, workspace_slug)` → DB path; persist **active `creator_id`** and **per-creator active `workspace_slug`** (fallback **`default`**); implement **`nexus42 creator workspace`** subcommands per `{v1-spec/cli-sync/cli-spec-v1.md}` §6.2C; keep **nexus-local-db** as SQLite owner. (No legacy flat `state.db` or migration command — product pre-release.)

**Tech Stack:** Rust (`nexus42`, `nexus42d`, `nexus-local-db`), SQLite, existing integration tests under `crates/nexus42` / `crates/nexus42d`.

---

## SSOT path variables (no private absolute paths)

In this plan, **`{v1-spec/…}`** denotes a path **relative to** `specs_root.v1-spec` from **`.agents/local-paths.json`** (copy from `.agents/local-paths.json.example` if missing).

| Variable | Resolves to (relative) |
|----------|-------------------------|
| `{v1-spec/cli-sync/cli-spec-v1.md}` | CLI + directory layout SSOT |
| `{v1-spec/cli-sync/local-db-schema-v1.md}` | Local SQLite filename + module boundaries |
| `{v1-spec/domain/data-model-v1.md}` | `WorkspaceBinding` and domain prose |
| `{v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md}` | **Normative ADR** — local FS layout, binding, commands |

**OSS companion (reachable without platform checkout):** `.agents/plans/knowledge/local-fs-layout-creator-workspace-v1.md` (non-normative recap + pointer)

---

## File / ownership map (implementation)

| Area | Responsibility |
|------|------------------|
| `.agents/plans/knowledge/local-fs-layout-creator-workspace-v1.md` | OSS companion — pointer to ADR-014 |
| `{v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md}` | Normative ADR |
| `{v1-spec/cli-sync/cli-spec-v1.md}` | User-facing command SSOT (`creator use` + `creator workspace *`) |
| `{v1-spec/cli-sync/local-db-schema-v1.md}` | Canonical **on-disk** path for `state.db` |
| `{v1-spec/domain/data-model-v1.md}` | §5.14 immutability + narrative |
| `crates/nexus42/src/config.rs` | `nexus_home`, **active workspace** pointer, DB path builder |
| `crates/nexus42d/src/workspace/mod.rs` | Daemon DB path + workspace discovery |
| `crates/nexus42/src/commands/init.rs` | Init creates creative tree + registers `meta` under `creators/.../workspaces/...` |
| `crates/nexus-local-db/src/lib.rs` | Document expected path; open DB by path only (no layout logic duplication if moved to config) |

---

### Task 1: Verify SSOT bundle (docs only)

**Files:**

- Read: `{v1-spec/adr/adr-014-local-fs-creator-workspace-layout-v1.md}` (normative)
- Read: `.agents/plans/knowledge/local-fs-layout-creator-workspace-v1.md` (OSS companion)
- Read: `{v1-spec/cli-sync/cli-spec-v1.md}` §6.2B–§6.3, §13.2
- Read: `{v1-spec/cli-sync/local-db-schema-v1.md}` §0
- Read: `{v1-spec/domain/data-model-v1.md}` §5.14

- [ ] **Step 1:** Open `.agents/local-paths.json` (or create from `.agents/local-paths.json.example`) and confirm `specs_root.v1-spec` resolves.
- [ ] **Step 2:** Confirm v1-spec sections match **ADR-014** D1–D6 (paths, **`workspace_slug`**, `creator use` + `creator workspace use`, `default` workspace, immutable binding).
- [ ] **Step 3:** Commit (platform repo) any v1-spec deltas and (nexus repo) ADR + knowledge index + this plan together in coordinated PRs if two remotes; else single-repo commit scope as applicable.

**Evidence:** `git diff` shows only intended spec/ADR/plan files.

---

### Task 2: Path resolution API — `nexus42` crate

**Files:**

- Modify: `crates/nexus42/src/config.rs`
- Create: `crates/nexus42/src/paths.rs` (or equivalent module name matching crate style)

- [x] **Step 1: Write failing unit test** for path composition (no network, no DB):

```rust
#[test]
fn operational_dir_follows_creator_then_workspace_slug() {
    let home = std::path::PathBuf::from("/fake/home");
    let creator_id = "ctr_test";
    let workspace_slug = "default";
    let got = nexus42::paths::operational_workspace_dir(&home, creator_id, workspace_slug);
    assert_eq!(
        got,
        std::path::PathBuf::from("/fake/home/.nexus42/creators/ctr_test/workspaces/default")
    );
}
```

- [x] **Step 2:** Run `cargo test -p nexus42 paths::` — expect **fail** (module missing).

- [x] **Step 3:** Implement `operational_workspace_dir`, `state_db_path` → `.../state.db`, and `shared_global_db_path` → `.../shared/global_state.db` per **ADR-014**.

- [x] **Step 4:** Run `cargo test -p nexus42 paths::` — expect **pass**.

- [x] **Step 5:** Commit `feat(nexus42): add operational path helpers for creator/workspace layout`

---

### Task 3: Active workspace pointer

**Files:**

- Modify: `crates/nexus42/src/config.rs`
- Modify: tests under `crates/nexus42/tests/` as needed

- [x] **Step 1:** Define persistent pointers: **global active `creator_id`** + **per-creator map** `last_workspace_slug` (fallback **`default`**). Optionally mirror wire **`workspace_id`** inside each `meta.json` only.
- [x] **Step 2:** Add **`nexus42 creator workspace use`** (and `list` / `create` stubs if phased) that validate `workspace_slug` exists under `creators/<creator_id>/workspaces/<workspace_slug>/` before updating the per-creator active slug.
- [x] **Step 3:** `cargo test -p nexus42` — pass.
- [x] **Step 4:** Commit `feat(nexus42): track active workspace pointer`

---

### Task 4: `init` writes registration + creative default

**Files:**

- Modify: `crates/nexus42/src/commands/init.rs`
- Modify: `crates/nexus42/tests/integration.rs`

- [x] **Step 1:** Change `init` so **creative root** default matches configurable template **documented in ADR** (e.g. `Documents/nexus/<creator_slug>/<workspace_slug>/`), still creating `Stories/` and `References/` per `{v1-spec/cli-sync/cli-spec-v1.md}` §13.1.
- [x] **Step 2:** Write **workspace meta** under `creators/<creator_id>/workspaces/<workspace_slug>/meta.json` (or TOML) with **immutable** `creator_id`, **`workspace_slug`**, `local_root`, optional wire **`workspace_id`**, `created_at`.
- [x] **Step 3:** Create empty `state.db` via `nexus-local-db` at `.../workspaces/<workspace_slug>/state.db` (not under `<workspace>/`).
- [x] **Step 4:** Update integration test that currently expects `.nexus42` **inside** project dir — align with ADR (operational under home; creative root may still contain a marker file **without** DB if needed for cwd discovery).
- [x] **Step 5:** `cargo test -p nexus42` — pass.
- [x] **Step 6:** Commit `feat(nexus42): init registers creator/workspace operational tree`

---

### Task 5: Daemon `nexus42d` DB path

**Files:**

- Modify: `crates/nexus42d/src/workspace/mod.rs`
- Modify: `crates/nexus42d/src/test_utils.rs`
- Modify: `crates/nexus42d/tests/integration.rs`

- [x] **Step 1:** Replace `home.join("state.db")` default with resolver that matches Task 2–4 (same path rules).
- [x] **Step 2:** Ensure HTTP handlers still obtain a `Connection` or pool from the **active** workspace context (may require passing workspace id from CLI client in early iterations).
- [x] **Step 3:** `cargo test -p nexus42d` — pass.
- [x] **Step 4:** Commit `feat(nexus42d): open state.db under creators/.../workspaces/...`

---

### Task 6: One-shot migration from global `state.db` — **removed**

**Decision:** Pre-release product; **no** legacy flat `$HOME/.nexus42/state.db` compatibility and **no** `migrate local-fs` command. CLI/daemon require `config.json` active creator + ADR-014 paths only.

---

### Task 7: CLI alignment — `creator use` + `creator workspace`

**Files:**

- Modify: command router under `crates/nexus42/src/`
- Cross-check: `{v1-spec/cli-sync/cli-spec-v1.md}` §6.2B–§6.2C

- [x] **Step 1:** Implement **`creator workspace` {list, create, use}** per spec; ensure **`creator use`** updates active Creator and resets slug per §6.2B table note.
- [x] **Step 2:** `cargo clippy --all -- -D warnings` — pass.
- [x] **Step 3:** Commit `feat(nexus42): creator workspace subcommands and creator use context`

---

### Task 8: CI + docs sweep

**Files:**

- Modify: `docs/` only where end-user install paths mention old layout (grep `.nexus42/state.db`)
- Modify: `AGENTS.md` only if version table or workflow bullets need a one-line pointer to new ADR

- [x] **Step 1:** `rg 'state\.db|\.nexus42/workspaces' docs crates` — update stale statements.
- [x] **Step 2:** Run full CI-equivalent per root `AGENTS.md` (`pnpm run validate-schemas`, `pnpm run codegen` if schemas touched, `cargo +nightly fmt --all -- --check`, `cargo clippy --all -- -D warnings`, `pnpm run typecheck`).
- [x] **Step 3:** Commit `docs: align local state paths with creator/workspace ADR`

---

## Self-review (plan author)

1. **Spec coverage:** ADR-014 D1–D6 map to v1-spec §13.2, §6.2C, §6.3 C2, `local-db-schema-v1` §0 path, `data-model` §5.14 immutability — **Task 1** verifies.
2. **Placeholder scan:** No `TBD` steps; tests name concrete files.
3. **Type consistency:** **`workspace_slug`** (path segment) vs Wire **`workspace_id`** (optional in `meta`) vs `creator_id` match cli-spec / contracts; `state.db` path matches `{v1-spec/cli-sync/local-db-schema-v1.md}`.

**Gaps (explicit):** `nexus42` ↔ `nexus42d` **active workspace handshake** (env var, config file, or IPC) is left to Task 5–7 implementation detail — if absent today, add **Task 5b** for “daemon reads same active pointer file as CLI” before merge.

---

**Plan complete** (nexus repo): `.agents/plans/2026-04-10-local-fs-layout-ssot-and-implementation.md`

**Execution options:**

1. **Subagent-driven (recommended)** — fresh subagent per task, review between tasks. **Superpowers:** `subagent-driven-development`.
2. **Inline execution** — same session, checkpoints per task. **Superpowers:** `executing-plans`.

Which approach do you want for implementation?
