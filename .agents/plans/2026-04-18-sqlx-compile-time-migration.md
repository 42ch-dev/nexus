# sqlx Compile-Time Macro Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate all ~79 runtime `sqlx::query()` calls across 5 crates to compile-time checked macros (`sqlx::query!()`, `sqlx::query_as!()`, `sqlx::query_scalar!()`), establish the `.sqlx/` offline workflow, and codify conventions in AGENTS.md.

**Architecture:** Bottom-up migration following crate dependency graph: `nexus-local-db` → `nexus-sync` → `nexus-orchestration` → `nexus42` + `nexus42d`. Each crate is migrated independently with full verification before proceeding. The `.sqlx/` directory stores query metadata for offline CI builds.

**Tech Stack:** Rust, sqlx (compile-time macros), SQLite, cargo-sqlx CLI

**Spec:** `.agents/plans/knowledge/sqlx-compile-time-migration-v1.md` (authoritative technical spec — read before implementing)

---

## Pre-Work

- [ ] **Step 1: Install cargo-sqlx CLI**

Run: `cargo install sqlx-cli --no-default-features --features sqlite`

- [ ] **Step 2: Create `.sqlx/` directory and reference database**

```bash
mkdir -p .sqlx
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx database create
cargo sqlx migrate run --source crates/nexus-local-db/migrations
```

- [ ] **Step 3: Apply nexus-sync outbox schema to reference DB**

```bash
sqlite3 .sqlx/state.db < .sqlx/sync-schema.sql
```

(Note: `.sqlx/sync-schema.sql` DDL is in the spec Appendix B.)

- [ ] **Step 4: Add `.gitignore` entries**

Add to `.gitignore`:
```
# sqlx offline reference database (only metadata is committed)
.sqlx/state.db
.sqlx/state.db-wal
.sqlx/state.db-shm
```

- [ ] **Step 5: Generate initial `.sqlx/` metadata**

```bash
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx prepare --workspace --all -- --all-targets
```

Verify: `ls .sqlx/query-*.json | wc -l` should show files (initially 0 since no macros exist yet — this will populate as we migrate).

- [ ] **Step 6: Commit `.gitignore` + `.sqlx/` setup**

```bash
git add .gitignore .sqlx/
git commit -m "chore(sqlx): initialize .sqlx offline workflow and reference database setup"
```

---

## Task 1: Migrate `nexus-local-db` (Batch 1 — Foundational)

**Files:**
- Modify: `crates/nexus-local-db/src/lib.rs` — `seed_versions`, `read_versions`; keep PRAGMA as runtime
- Modify: `crates/nexus-local-db/src/identity.rs` — all 7 functions
- Modify: `crates/nexus-local-db/src/memory_fragment.rs` — all 5 functions
- Modify: `crates/nexus-local-db/src/soul_meta.rs` — all 3 functions
- Modify: `crates/nexus-local-db/src/pending_review.rs` — all 6 functions

**Conversion rules (from spec §1.2):**
- `sqlx::query("SQL").bind(x).bind(y)` → `sqlx::query!("SQL", x, y)` — remove `.bind()` chains, use `?` instead of `?1, ?2`
- `sqlx::query_as::<T>("SQL")` → `sqlx::query_as!(T, "SQL")` — type as macro arg
- `sqlx::query_as::<_, (T,)>("SQL")` for single scalar → `sqlx::query_scalar!("SQL")` where appropriate
- Keep PRAGMA and DDL as runtime with `// SAFETY:` comment
- Handle bool ↔ INTEGER mismatch: struct fields use `i64`, convert with `!= 0` in `.map()`
- Nullable columns: use `Option<T>` matching macro inference

- [ ] **Step 1: Convert `lib.rs`**

Keep PRAGMAs as runtime (add `// SAFETY: PRAGMA statement` comments). Convert `seed_versions` and `read_versions` to macros.

- [ ] **Step 2: Convert `identity.rs`**

All 7 functions (create, get, list, link, unlink, delete). Watch for `platform_linked` bool field.

- [ ] **Step 3: Convert `memory_fragment.rs`**

All 5 functions (create, list, list_by_session, delete, get_all_keywords).

- [ ] **Step 4: Convert `soul_meta.rs`**

3 functions (upsert, get, delete).

- [ ] **Step 5: Convert `pending_review.rs`**

6 functions (create, list, get, delete, count). `count_pending_reviews` should use `query_scalar!`.

- [ ] **Step 6: Verify**

```bash
cargo check -p nexus-local-db
cargo test -p nexus-local-db
cargo clippy -p nexus-local-db -- -D warnings
```

- [ ] **Step 7: Generate .sqlx/ metadata and commit**

```bash
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx prepare --workspace --all -- --all-targets
git add crates/nexus-local-db/ .sqlx/
git commit -m "refactor(sqlx): migrate nexus-local-db to compile-time checked macros"
```

---

## Task 2: Migrate `nexus-sync` (Batch 2)

**Files:**
- Modify: `crates/nexus-sync/src/outbox.rs` — ~17 query calls; keep DDL + PRAGMAs as runtime
- Modify: `crates/nexus-sync/src/pool.rs` — ~2 PRAGMA calls (keep runtime)

**Key challenges (from spec):**
- DDL in `init_pool_with_schema` stays runtime with `// SAFETY:` comments
- `replay()` and `get()` return 9-tuples — convert to `sqlx::query_as!` with anonymous record struct
- `stage_if_absent` has a conditional pattern — keep runtime for the conditional branch if needed

- [x] **Step 1: Convert `outbox.rs` — DML queries**

Convert all INSERT/UPDATE/DELETE/SELECT DML queries to macros. Keep DDL and PRAGMAs as runtime.

- [x] **Step 2: Convert `outbox.rs` — tuple queries**

`replay()` and `get()` return 9-tuples. Use `sqlx::query_as!` returning an anonymous record struct, then map to the tuple type.

- [x] **Step 3: Verify**

```bash
cargo check -p nexus-sync
cargo test -p nexus-sync
cargo clippy -p nexus-sync -- -D warnings
```

- [x] **Step 4: Generate .sqlx/ metadata and commit**

```bash
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx prepare --workspace --all -- --all-targets
git add crates/nexus-sync/ .sqlx/
git commit -m "refactor(sqlx): migrate nexus-sync to compile-time checked macros"
```

---

## Task 3: Migrate `nexus-orchestration` (Batch 3)

**Files:**
- Modify: `crates/nexus-orchestration/src/storage/sqlite.rs` — 3 queries
- Modify: `crates/nexus-orchestration/src/schedule/supervisor.rs` — ~8 queries
- Modify: `crates/nexus-orchestration/src/schedule/derivation.rs` — ~7 queries
- Modify: `crates/nexus-orchestration/tests/schedule_regression_r11_r14.rs` — 1 query (test-only, acceptable to keep runtime with SAFETY comment)

- [ ] **Step 1: Convert `storage/sqlite.rs`**

3 queries (save, get, delete session).

- [ ] **Step 2: Convert `schedule/supervisor.rs`**

~8 queries. Watch for `ScheduleRow` type mapping.

- [ ] **Step 3: Convert `schedule/derivation.rs`**

~7 queries. Follow same pattern as supervisor.

- [ ] **Step 4: Verify**

```bash
cargo check -p nexus-orchestration
cargo test -p nexus-orchestration
cargo clippy -p nexus-orchestration -- -D warnings
```

- [ ] **Step 5: Generate .sqlx/ metadata and commit**

```bash
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx prepare --workspace --all -- --all-targets
git add crates/nexus-orchestration/ .sqlx/
git commit -m "refactor(sqlx): migrate nexus-orchestration to compile-time checked macros"
```

---

## Task 4: Migrate `nexus42` + `nexus42d` (Batch 4)

**Files:**
- Modify: `crates/nexus42/src/manuscript/manager.rs`, `crates/nexus42/src/commands/*.rs`, `crates/nexus42/src/db/mod.rs`, `crates/nexus42/src/test_utils.rs`, `crates/nexus42/src/workspace/mod.rs`
- Modify: `crates/nexus42d/src/db/schema.rs` (DDL — keep runtime), `crates/nexus42d/src/db/pool.rs` (PRAGMAs — keep runtime), `crates/nexus42d/src/api/handlers/*.rs`, `crates/nexus42d/src/auth/token_manager.rs`, `crates/nexus42d/src/api/auth_middleware.rs`
- Modify: `crates/nexus42d/tests/integration.rs` — test queries

- [ ] **Step 1: Convert `nexus42`**

~9 queries across CLI commands, db layer, test utilities. Test helpers that mirror production queries should be converted; DDL/PRAGMA tests stay runtime.

- [ ] **Step 2: Convert `nexus42d` — handler queries**

~18 queries across handlers, auth, db layer. `db/schema.rs` DDL and `db/pool.rs` PRAGMAs stay runtime.

- [ ] **Step 3: Convert `nexus42d` — test files**

Integration test queries. Convert data-setup queries; keep schema-validation queries as runtime.

- [ ] **Step 4: Full workspace verification**

```bash
cargo check --all
cargo test --all
cargo clippy --all -- -D warnings
cargo +nightly fmt --all -- --check
```

- [ ] **Step 5: Generate .sqlx/ metadata and commit**

```bash
export DATABASE_URL="sqlite:.sqlx/state.db?mode=rwc"
cargo sqlx prepare --workspace --all -- --all-targets
git add crates/nexus42/ crates/nexus42d/ .sqlx/
git commit -m "refactor(sqlx): migrate nexus42 and nexus42d to compile-time checked macros"
```

---

## Task 5: CI Re-enable, AGENTS.md Convention, Final Verification (Batch 5)

**Files:**
- Modify: `.github/workflows/ci.yml` — uncomment `verify-sqlx-offline` job
- Modify: `AGENTS.md` — add sqlx compile-time macro convention section (draft in spec §3)
- Modify: `.agents/plans/knowledge/README.md` — spec already updated

- [ ] **Step 1: Re-enable `verify-sqlx-offline` CI job**

Uncomment the job in `.github/workflows/ci.yml`. It should look like the YAML in spec §2.4.

- [ ] **Step 2: Add sqlx convention to `AGENTS.md`**

Add the "sqlx Compile-Time Macros (Mandatory)" section from spec §3 to `AGENTS.md` after the "Rust development" section.

- [ ] **Step 3: Full final verification**

```bash
# Offline mode (simulates CI)
SQLX_OFFLINE=true cargo check --all

# Full test suite
cargo test --all

# Clippy + fmt
cargo clippy --all -- -D warnings
cargo +nightly fmt --all -- --check

# Verify .sqlx/ is up-to-date
cargo sqlx prepare --workspace --all -- --all-targets
git diff --exit-code .sqlx/
```

- [ ] **Step 4: Commit and push**

```bash
git add .github/workflows/ci.yml AGENTS.md
git commit -m "chore(ci,docs): re-enable verify-sqlx-offline and codify sqlx compile-time macro convention"
```

---

## Acceptance Criteria

- [ ] Zero runtime `sqlx::query()` / `sqlx::query_as()` calls for static SQL (only DDL, PRAGMAs, and truly dynamic queries remain with `// SAFETY:` comments)
- [ ] `.sqlx/query-*.json` metadata files committed (~79 files expected)
- [ ] `.sqlx/state.db` in `.gitignore` (not committed)
- [ ] `AGENTS.md` has mandatory sqlx compile-time macro convention
- [ ] `verify-sqlx-offline` CI job re-enabled and passing
- [ ] All tests pass: `cargo test --all`
- [ ] Clippy clean: `cargo clippy --all -- -D warnings`
- [ ] `SQLX_OFFLINE=true cargo check --all` succeeds
