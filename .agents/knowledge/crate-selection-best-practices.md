# Crate Selection Best Practices v1

**Status**: Active — authoritative guidance for third-party Rust crate selection across `nexus` OSS crates.
**Scope**: Rust workspace only (`crates/nexus*`). TypeScript packages (`packages/nexus-contracts`, `tooling/`) are out of scope.
**Date**: 2026-04-17
**Supersedes**: — (new topic)

---

## 0. Purpose & SSOT boundary

This document governs:

- **Dependency hygiene** for the Rust workspace (version pinning, feature flags, workspace vs local).
- **Current crate selection decisions** for modules where a PM-level choice has been made (§2 table).
- **Upgrade / replacement process** for future crate swaps (§4).

This document does **not** override:

- `AGENTS.md` — release discipline, codegen rules, reachability rules.
- `v1-spec/` (ADRs / codegen strategy / wire schemas) — wire and protocol decisions (e.g. `agent-client-protocol` SDK pin) remain owned there.
- Architecture SSOTs — `orchestration-engine.md`, `daemon-lifecycle-api.md`, `creator-schedule-and-core-context.md`, `acp-client-tech-spec.md`, `schemas-boundary.md`, `local-db-refactor.md`, `architecture-alignment-review.md`.

**Conflict order**: `AGENTS.md` > `v1-spec` / ADR > architecture SSOTs > this document > other knowledge.

---

## 1. Dependency conventions (the six rules)

### 1.1 Prefer `workspace.dependencies`

Any dependency used by **two or more** workspace crates MUST be declared once under `[workspace.dependencies]` in the root `Cargo.toml`, then referenced by members via `{ workspace = true }`. Single-crate use MAY stay local.

**Rationale**: single source of truth for versions and features; CI-wide upgrade is one commit.

### 1.2 Version pinning

- **Protocol / ABI sensitive crates** (`agent-client-protocol`, `graph-flow`): pin exactly with `=X.Y.Z`. Rationale: wire-level breakage is silent until runtime.
- **Other crates**: caret `X.Y` (e.g. `"1.0"`, `"0.12"`). Cargo resolves the latest compatible patch.
- **Forbidden**: bare `*`, unqualified git dependencies in production crates, path dependencies to directories outside the workspace.

### 1.3 Feature-flag whitelist

- New dependency MUST be added with `default-features = false` **unless** defaults are explicitly reviewed.
- Required features MUST be listed explicitly. Reviewer/author SHOULD be able to answer: *"why this feature and not the next smaller set?"*
- Prefer small composable features (`serde`, `chrono`, `macros`) over mega-flags (`full` in `tokio` is an accepted exception because it is already pervasive here).

### 1.4 Dependency-footprint discipline

Any new crate that pulls **>10 transitive dependencies** or adds **>1 MB** to the release binary MUST be justified in the PR description: what problem it solves, what alternatives were rejected, and why. Reviewer MAY block on missing justification.

### 1.5 Introduction gate

- New `[workspace.dependencies]` entries require PM approval (or explicit authorisation in the sponsoring plan).
- Crates sneaking in only at leaf-crate level are still subject to the justification rule (§1.4) but do not need a workspace-level PR.
- **This document is the ledger**: every current PM decision lives in §2. Undocumented workspace additions should be refused in review.

### 1.6 Replacement / deprecation

Changing a decision in §2 follows the flow in §4. Do not silently swap crates, even drop-in ones — the decision table and research log exist specifically to prevent context loss.

---

## 2. Decisions ledger (single-page quick reference)

| # | Module | Decision | Alternative(s) considered | Plan / SSOT linkage |
|---|--------|----------|---------------------------|---------------------|
| 2.1 | **JSON-RPC** (daemon ↔ `acp-worker` IPC) | `jsonrpsee-core` + proc macros + custom `RpcTransport` trait + NDJSON via `tokio_util::codec::LinesCodec` | Hand-rolled `serde_json` framing; `json-rpc-rs`; `tokio-jrpc`; `karyon-jsonrpc` | `2026-04-17-v1.4-ws2-orchestration-skeleton.md` Task 5; `orchestration-engine.md` §6 |
| 2.2 | **Orchestration SessionStorage** | `sqlx` (sqlite + runtime-tokio + macros + migrate + chrono + uuid) on the unified **`state.db`** via the `Arc<SqlitePool>` exposed by `nexus-local-db` (post-WS8). `orchestration_sessions` table added through the same `sqlx migrate` pipeline. | Keep `rusqlite` + `deadpool-sqlite`; separate `.db` file; `sea-orm` | `2026-04-17-v1.4-ws2-orchestration-skeleton.md` Task 3 (depends on WS8) |
| 2.3 | **`nexus-local-db` / `state.db` engine** | **Migrated from `rusqlite` + `deadpool-sqlite` + bespoke sequential migrations → to `sqlx` (sqlite + runtime-tokio + macros + migrate)** as **V1.4 WS8** (done). Bespoke `Migration` registry replaced by `sqlx::migrate!`-driven `.sql` files. | Keep rusqlite (A-4 rejected by PM 2026-04-17) | `2026-04-17-v1.4-ws8-local-db-sqlx-migration.md` (new plan row); `local-db-refactor.md` revised at WS8 T9 |
| 2.4 | **Platform user auth / JWT** | `jsonwebtoken` only | `oauth2` v5.x (deferred — not rejected) | `device-flow-oauth-scope-v1.md` (TD-10 deferral) |
| 2.5 | **Challenge arithmetic evaluator** | Hand-rolled shunting-yard (current implementation under `crates/nexus42/src/challenge/`) | `meval`; `evalexpr` | §3.5 below (DoS guard TODOs tracked here, not in `status.json`) |
| 2.6 | **File watcher** (deferred) | Recommended stack when implemented: `notify` 8 + `notify-debouncer-full` + async mpsc | Raw `RecommendedWatcher` only | `crates/nexus42d/src/workspace/mod.rs` (deferral note) |
| 2.7 | **Cron / scheduler** (V1.5 — implemented) | **V1.5 WS-D implemented** a hand-rolled clock poller in `crates/nexus-orchestration/src/scheduler/` using `cron` + `chrono-tz`. The four constraints from §3.7 are satisfied. See `creator-schedule-and-core-context.md` for the full design. | `tokio-cron-scheduler` 0.15.x (rejected); hand-rolled `cron` + `chrono-tz` + `sleep_until` (**selected & shipped**) | `creator-schedule-and-core-context.md` |
| 2.8 | **Layered config** (future) | `figment` + `secrecy` for redaction (when needed) | `config-rs`; hand-rolled | — (not yet scheduled) |
| 2.9 | **Snapshot testing** (dev) | Recommended: `insta` + redactions for new CLI/HTTP integration tests | Hand-rolled golden files | — (optional; per-test-author discretion) |

> Full per-decision details in §3. Original external research captured in [`../reports/2026-04-17-crate-selection-research/research-log.md`](../reports/2026-04-17-crate-selection-research/research-log.md).

---

## 3. Per-decision details

### 3.1 JSON-RPC (daemon ↔ worker IPC)

**Decision**: `jsonrpsee-core` + proc macros + custom `RpcTransport` trait; framing = newline-delimited JSON via `tokio_util::codec::LinesCodec` over stdin/stdout pipes.

**Why not hand-rolled**: method count will grow from single digits to tens; cancellation, timeouts, batch, and typed error objects are free with `jsonrpsee`, while a hand-rolled layer becomes a maintenance sink as methods proliferate.

**Why `jsonrpsee-core` specifically**: `jsonrpsee` has no built-in stdio transport, but its core exposes `RpcModule` + trait-based transport so we can adapt it to stdio with a thin shim. We retain the option to replace core later (see §4); the `RpcTransport` trait is our insurance.

**Implementation sketch** (to be realised in `crates/nexus-orchestration` per WS2 Task 5):

```rust
#[async_trait::async_trait]
pub trait RpcTransport: Send + 'static {
    async fn recv(&mut self) -> Option<String>;
    async fn send(&mut self, line: String) -> std::io::Result<()>;
}
```

Stdio impl uses `tokio_util::codec::FramedRead<_, LinesCodec>` + `FramedWrite<_, LinesCodec>` on the child's stdin/stdout. Unit tests use `tokio::io::duplex` or a channel-backed fake.

**Cancellation / timeouts**: `tokio::select!` + `CancellationToken` at the call site; jsonrpsee's async method signature carries it naturally.

**Feature flags** (exact version resolved at introduction time per §1.2 — **do not fabricate a version in this document**; the authoritative version lives in `Cargo.toml` when WS2 Task 5 lands):

```toml
# Shape only — author MUST pin an actual version from crates.io when adding.
jsonrpsee = { version = "<pin-at-intro>", default-features = false, features = ["server", "client", "macros"] }
# pick matching minor + server-core/client-core feature split per current jsonrpsee layout
```

**Testing**: in-memory duplex transport MUST be provided as a dev-dep shim in `nexus-orchestration`'s tests; every registered method should at minimum have a round-trip test.

---

### 3.2 Orchestration SessionStorage — `sqlx` on unified `state.db`

**Decision**: `sqlx` against the **single** `state.db` file owned by `nexus-local-db`. `crates/nexus-orchestration` takes an `Arc<sqlx::SqlitePool>` from `nexus-local-db` and registers its `orchestration_sessions` table through the same `sqlx migrate` pipeline.

**Dependency**: this decision **depends on WS8** (§2.3) completing first. Until WS8 lands, WS2's Task 3 is blocked (per PM directive 2026-04-17, B-1: WS8 → WS2 serial).

**Rationale for unified file (pivot from 2026-04-17)**:

- Single DB engine (`sqlx`) across the whole workspace → one async model, one migration runner, one pool implementation.
- Eliminates the "two pools at the same file" correctness risk (SQLite write-lock contention between `rusqlite` and `sqlx` pools).
- Eliminates the "two files, two migration stories" operational complexity.
- Makes the `// reuses nexus-local-db pool` comment in `orchestration-engine.md` §4.3 technically correct (the two pools become **one** pool).

**Feature flags** (shared with `nexus-local-db` at the workspace level):

```toml
# [workspace.dependencies] — added by WS8 T1
sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio",
    "sqlite",
    "macros",
    "migrate",
    "chrono",
    "uuid",
] }
```

**Migrations**: unified under `crates/nexus-local-db/migrations/` with timestamped `.sql` files; `sqlx::migrate!().run(&pool).await` executed once at `nexus-local-db` init. `orchestration_sessions` migration is one more `.sql` file in that directory (added in WS2 T3 after WS8 T1 lands).

**CI changes (minimum viable)** — delivered by **WS8**:

1. Contributors run `cargo sqlx prepare --workspace --all -- --all-targets` after schema or query changes, commit `.sqlx/` directory.
2. CI sets `SQLX_OFFLINE=true` and adds `cargo sqlx prepare --workspace --all -- --all-targets --check` to the existing Rust job.
3. Optional: an integration-test job sets `DATABASE_URL=sqlite::memory:` and runs `sqlx migrate run` before `cargo test`.

**Boundary**: `nexus-orchestration` depends on `nexus-local-db` for the pool and the migration registry, but **not** for any domain types. Orchestration tables remain distinct from identity / outbox / workspace tables; only the physical file and pool are shared.

---

### 3.3 `nexus-local-db` / `state.db` — sqlx unification (V1.4 WS8)

**Decision**: fully migrated `nexus-local-db` from `rusqlite` + `deadpool-sqlite` + the hand-rolled `Migration` registry **to `sqlx` + `sqlx migrate`**. This was V1.4 workstream **WS8**, completed 2026-04-18; see the dedicated plan row `2026-04-17-v1.4-ws8-local-db-sqlx-migration.md` for the ordered task breakdown.

**What changes**:

- `nexus-local-db` exposes `Arc<sqlx::SqlitePool>` (and typed `&mut sqlx::SqliteConnection` helpers) instead of `rusqlite::Connection` / `deadpool::Object`.
- All 34 files (~193 references) in `nexus42` / `nexus42d` / `nexus-sync` / `nexus-local-db` that currently import `rusqlite` or `deadpool_sqlite` become async sqlx callers.
- The four existing migrations (v2 `local_identities`, v3 `soul_meta`, v4 `memory_pending_review` + `memory_fragments`) are ported to timestamped `.sql` files under `crates/nexus-local-db/migrations/`.
- The `db_schema_version` key in `workspace_meta` is **preserved** across the port (rebaselined to sqlx's migration version tracking).

**Impact on other SSOTs** (handled in WS8 T9 — done):

- `local-db-refactor.md` → updated (engine choice, migration strategy section, API shape) at WS8 T9.
- `orchestration-engine.md` §4.3 → the "reuses nexus-local-db pool" comment became literal (both sides now `sqlx::SqlitePool`); the "new tables in existing state.db" phrasing is reinforced.

**What does NOT change**:

- Table classifications (shared vs daemon-only — per `local-db-refactor.md` §3) are preserved.
- On-disk file path, `workspace_meta` key semantics, and row-level migration content are preserved.
- V1.2/V1.3 features (`local_identities`, `soul_meta`, memory pipeline, challenge solver) **must** still pass regression after the port — WS8 acceptance gates enforce this explicitly.

**No new rusqlite code** SHOULD be added to this repo from 2026-04-17 forward. Reviewers MAY block additions referencing `rusqlite` or `deadpool-sqlite` unless they are part of WS8's porting sequence.

---

### 3.4 Platform user auth — `jsonwebtoken` only

**Decision**: V1.x uses `jsonwebtoken` to validate access tokens issued by the platform. **No `oauth2` crate in V1.x.**

**Rationale**: TD-10 (real Device Authorization Grant) is intentionally deferred per `device-flow-oauth-scope-v1.md`; the current stub `verify_device_code` returns `Ok(false)` on purpose. Pulling in a full OAuth client now would add surface area without unlocking any scheduled functionality.

**When TD-10 is scheduled**: `oauth2` v5.x is the first candidate (RFC 8628 support is first-class); evaluate at that time and update this document to v2. Candidate is **deferred**, not rejected.

**Feature flags** (jsonwebtoken): keep defaults; no special features required.

---

### 3.5 Challenge arithmetic evaluator

**Decision**: retain the hand-written evaluator at `crates/nexus42/src/challenge/` (`parser.rs`, `eval.rs`, `noise.rs`, `numbers.rs`). No crate added.

**Rationale**: inputs are post-de-obfuscation, still adversarial. A whitelisting lexer is the only mechanism that can guarantee no letters, no function names, no scientific notation. Both `meval` and `evalexpr` require careful outer guards and are weaker along those dimensions.

**Follow-up TODO (NOT a `residual_finding`)** — tracked here and here only, per PM directive:

1. **`max_input_len`** — reject inputs over 512–1024 chars at the earliest preprocessing step (`sanitize_expr`).
2. **`max_paren_depth`** — iterative depth counter inside the parser; cap at 20–30.
3. **`eval_timeout`** — wrap the evaluation in `tokio::time::timeout` (or equivalent) at call sites that accept external input.

**Escalation path**: if challenge abuse is reported, promote to a real residual under the appropriate plan row.

---

### 3.6 File watcher (deferred)

**Current state**: `crates/nexus42d/src/workspace/mod.rs` explicitly defers a real watcher, using a best-effort staleness check. This section is guidance for whoever lands the real thing.

**Recommended stack**: `notify` 8 + `notify-debouncer-full` (~0.4) + async `mpsc` channel (`tokio::sync::mpsc` or `flume`).

**Operational rules (mandatory when implemented)**:

- **Never watch `$HOME`**. Watch only explicit workspace roots, read from config at daemon start.
- Recursive mode is allowed but cap top-level roots (10–50).
- **Linux**: check `fs.inotify.max_user_watches` on startup; above a threshold (~8192) fall back to `PollWatcher` with `compare_contents`.
- Filter early in the event handler: ignore `.git/`, `node_modules/`, `target/`, OS temp patterns.
- Debounce delay 500 ms – 2 s.
- **Backup poll**: a `tokio::time::interval` (~30 s) re-checks `metadata().modified()` on critical files to cover silent event loss (especially on network FS).
- On daemon restart, re-register all watches (no persistence).

**Integration pattern**: `EventHandler` pushes into `mpsc::Sender<DebouncedEvent>`; a dedicated Tokio task `select!`s on the receiver and invokes cache invalidation. `spawn_blocking` is **only** for watcher construction (if needed), never for event reception.

---

### 3.7 Cron / scheduler — V1.4 defers; V1.5 decides

**Decision**: V1.4 does **not** select a scheduler crate. `orchestration-engine.md` defers wall-clock cron to V1.5+; `creator-schedule-and-core-context.md` (WS7) delivers only the data model + state machine.

**Constraints any future implementation must satisfy** (binding on V1.5 design):

1. **Wall-clock + tz-aware**: use `chrono::DateTime<Tz>` (or equivalent) — not `Instant`.
2. **Per-key serialisation**: same creator + same Schedule MUST NOT double-run.
3. **Graceful shutdown**: shutdown path waits for the currently-running orchestration node to finish (or forces abort after a configured timeout).
4. **DST / clock-jump safety**: recompute next-run timestamps when wall-clock discontinuities are detected (compare `SystemTime` vs `Instant` elapsed deltas).

**V1.4 artefact**: a `trait Scheduler` placeholder (or, minimally, a module doc-comment enumerating the four constraints) in `nexus-orchestration` so V1.5 implementors have a stable call site to swap into.

**V1.5 candidates**:

- `tokio-cron-scheduler` 0.15.x — quickest to integrate; DST risk must be mitigated with external recompute logic; per-key serialisation and graceful shutdown require wrappers.
- Hand-rolled `cron` + `chrono-tz` + `tokio::time::sleep_until` — ~150 LOC for the scheduler loop, but full control over all four constraints.

Full comparison will be re-run in the V1.5 plan; do not pre-decide.

---

### 3.8 Layered configuration — `figment` (when needed)

**Decision**: when a real layered-config requirement arises (`defaults < TOML < env`, redacted effective-config printing, error messages naming the source), use `figment` + `secrecy`. Not required in V1.4.

**Provider chain (canonical shape)**:

```rust
use figment::{Figment, providers::{Format, Toml, Env, Serialized}};

let figment = Figment::from(Serialized::defaults(Config::default()))
    .merge(Toml::file("nexus42.toml"))
    .merge(Env::prefixed("NEXUS42_").split("_"));
let config: Config = figment.extract()?;
```

**Redaction convention**: wrap secret strings in `secrecy::SecretString` (or equivalent); add `#[serde(serialize_with = "redact")]` on sensitive fields so effective-config dumps emit `***`.

**Fallback**: `config-rs` is acceptable if a future requirement demands multi-format support (YAML/JSON5/RON) or hot-reload. Hand-rolled merging is discouraged — error diagnostics degrade sharply.

---

### 3.9 Snapshot testing — `insta` (dev, optional)

**Decision**: new CLI / HTTP integration tests that assert JSON bodies or log fragments SHOULD default to `insta` + redactions. Authors MAY opt out with an inline comment citing one of the following:

1. Outputs dominated by truly unstable data (no stable structure under redaction).
2. Semantic equality beats textual equality for the assertion.
3. Snapshot size exceeds a few MB.

**Feature flags**: at minimum `insta = { version = "<pin-at-intro>", features = ["json", "redactions"] }` (exact version resolved by the first author to adopt it, per §1.2); `cargo-insta` lives as a dev tool (not a workspace dep).

**Existing tests**: no retrofit mandate. Do not rewrite passing tests purely to adopt snapshots.

---

## 4. Upgrade / replacement flow

When a decision in §2 needs to change (new requirement, crate EOL, security advisory, etc.):

1. **Problem statement + candidates** — describe what the current decision fails to cover; enumerate at least two candidates (may include "keep current").
2. **Comparison** — evaluate across the same dimensions used in this document's research log (dep footprint, safety, async fit, error quality, test cost, maintenance risk). Record in a new entry under `.agents/plans/reports/<YYYY-MM-DD-crate-<topic>-research>/research-log.md`.
3. **Decision** — update the affected row in §2, bump this file to `v<N+1>` if the change is substantive (otherwise in-place amendment with a dated note is fine), and update consumers per §5.
4. **Impact tracking** — any code / plan changes flow through the normal plan system; they do not bypass this document.

Breaking crate upgrades (e.g. `sqlx 0.8 → 0.9` with API churn) follow the same flow.

---

## 5. Consumer map

Documents and plans that cite this best-practices file:

- `v1.4-delivery-compass-v1.md` — Tech Stack / Risk sections.
- `orchestration-engine.md` — §4 (graph-flow integration), §5 (capability store), §6 (worker IPC).
- `creator-schedule-and-core-context.md` — wall-clock deferral clause.
- `2026-04-17-v1.4-ws2-orchestration-skeleton.md` — Tech Stack + Task 1 (sqlx features) + Task 5 (IPC implementation).
- `2026-04-17-crate-selection-research/research-log.md` — the archival source of the comparisons behind §2 rows.

Before editing any of the above in a way that changes crate selection, update **this** document first.

---

## 6. References

- Research log (archival): [`../reports/2026-04-17-crate-selection-research/research-log.md`](../reports/2026-04-17-crate-selection-research/research-log.md)
- V1.4 delivery compass: [`v1.4-delivery-compass-v1.md`](../iterations/v1.4-delivery-compass-v1.md)
- Orchestration engine SSOT: [`orchestration-engine.md`](../knowledge/specs/orchestration-engine.md)
- Schedule / core context SSOT: [`creator-schedule-and-core-context.md`](creator-schedule-and-core-context.md)
- Local DB ownership: [`local-db-refactor.md`](archived/knowledge/local-db-refactor.md)
- Device flow deferral: [`device-flow-oauth-scope-v1.md`](device-flow-oauth-scope-v1.md)
- Repository-wide rules: [`AGENTS.md`](../../../AGENTS.md) — §"Documentation & plans", dependency / release discipline.

---

*Created: 2026-04-17.*
