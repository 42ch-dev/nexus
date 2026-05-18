# Crate Selection Research Log — 2026-04-17

**Scope**: Third-party Rust crate evaluation for seven `nexus` OSS modules (plus one optional dev-experience topic).
**Companion doc (SSOT for decisions)**: [`../../knowledge/crate-selection-best-practices.md`](../../knowledge/crate-selection-best-practices.md)
**Research method**: Side-by-side comparison produced by an external LLM (anonymised; no account, URL, or user-identifying metadata recorded per repo reachability rules).
**Reachability note**: This file is fully in-repo; external links have been stripped. Readers who freshly `git clone` this repo can open every referenced path.

---

## 0. How to read this log

- Each section below captures the **technical substance** of the external comparison for one module.
- Pleasantries, offers to generate code snippets, and marketing language have been trimmed.
- Each section ends with a **"Final decision (this repo)"** box that **mirrors** the corresponding row in `crate-selection-best-practices.md §2`. If the two ever drift, the knowledge doc wins (it is the SSOT; this log is archival context).

---

## 1. JSON-RPC: `jsonrpsee` vs hand-rolled framing

**Context asked**: Rust + Tokio, CLI worker subprocess ↔ local daemon over stdin/stdout JSON-RPC 2.0, method count growing from single digits to tens, needs cancellation/timeouts, clean error objects, testable transport abstraction.

**Key finding**: `jsonrpsee` has **no built-in stdio/IPC transport** (only HTTP/WS/WASM officially). "Choosing jsonrpsee" here means **`jsonrpsee-core` + `RpcModule` + a custom framing layer**. The common hand-rolled alternative is `serde_json` + `tokio::io` + `tokio-util::codec` (newline-delimited JSON or length-prefixed).

### Comparison

| Dimension | `jsonrpsee-core` + custom transport | Hand-rolled `serde_json` framing |
| --------- | ----------------------------------- | -------------------------------- |
| **Dep footprint** | Medium (controllable via `server-core`/`client-core`/`macros` features; binary grows a few hundred KB–1 MB depending on features) | Minimal (`serde_json` + `tokio` + optional `tokio-util`) |
| **Partial-read / backpressure** | Excellent: core owns JSON-RPC protocol (request/response/batch/error); you only need to supply framed JSON via `LinesCodec` or `LengthDelimitedCodec`. Backpressure flows through Tokio Sink/Stream naturally. | Manual: `BufReader` + codec for partial reads, `write_all` + `flush` for backpressure. Fully controllable but more surface for bugs. |
| **Tokio cancellation integration** | Excellent: RpcModule methods are `async fn`; works with `tokio::select!`, `timeout`, `CancellationToken`, `AbortHandle`. Pending requests drop cleanly. | Good: you own the loop, can `select!` around each request future, but must manage request-ID mapping and cancellation manually. |
| **Version maintenance risk** | Low-to-medium: Parity maintains actively; APIs stable, occasional breaking changes between majors (feature flags help). | Zero dep risk; but bug risk grows as method count / edge-case surface grows. |
| **Unit + integration test cost** | Low: `RpcModule` callable in-memory (no real IO), proc-macro generates client/server, transport can be a trait with an in-memory duplex mock. Subprocess + pipe tests remain straightforward. | Moderate: you write your own transport trait + mock, plus serialization-layer tests. Scales poorly with method count. |
| **Future batch request support** | Native: core handles `[]` batches, parallelisation, aggregated errors. | Manual: deserialize into `Vec<Request>` vs single, aggregate responses. |

### Alternative crates considered (and rejected)

- **`json-rpc-rs`** (BYO transport): framework-agnostic protocol layer. **Rejected**: much smaller community than `jsonrpsee`, no proc-macro / declarative method registration, weak ecosystem. Maintenance cost scales badly with method count.
- **`tokio-jrpc`** (newline-delimited JSON-RPC over any `AsyncRead + AsyncWrite`): Tokio-native, typed client/server, framing built-in. **Rejected**: limited batch support, less complete error-object handling, low update cadence, no proc macro — struggles as methods grow.
- **`karyon-jsonrpc`** (lightweight async JSON-RPC over TCP/WS/Unix): **Rejected**: primarily network-transport oriented; stdin/stdout adaptation is not first-class; small community; weaker ergonomics than `jsonrpsee` for dozens of methods.

### Final decision (this repo)

> **Use `jsonrpsee-core` + proc macros + a custom `RpcTransport` trait.** Framing = newline-delimited JSON via `tokio_util::codec::LinesCodec` (stdio pipes guarantee ordering). Transport abstracted so unit tests use `tokio::io::duplex` or an in-memory channel.
> **Batch / cancellation / timeouts are free** via Tokio + jsonrpsee's existing primitives.
> If, later, dep footprint becomes a real problem, the framing code is ~100 lines and can be extracted without touching the `RpcTransport` trait — the abstraction is the insurance policy.

---

## 2. Orchestration SessionStorage: `sqlx` vs `rusqlite` + `deadpool-sqlite`

**Context asked**: Rust workspace with Axum + Tokio multi-thread runtime. Existing local DB uses `rusqlite` + `deadpool-sqlite` + bespoke sequential migrations. New requirement: a **separate SQLite file** for orchestration `SessionStorage`, read/write intensive, needs a clean pool + migrations.

### Comparison

| Dimension | `sqlx` (+ migrate + offline query metadata) | `rusqlite` + `deadpool-sqlite` |
| --------- | ------------------------------------------- | ------------------------------- |
| **CI complexity** | Moderate: needs offline mode (`cargo sqlx prepare` → `.sqlx/` committed; CI sets `SQLX_OFFLINE=true` and `--check`). One-time setup. | Very low: `cargo build`/`test` works as-is. |
| **Compile-time SQL validation** | **High value**: `query!` / `query_as!` / `query_scalar!` macros check tables/columns/types/params against the live schema. For a read/write-heavy `SessionStorage` whose queries will iterate as the engine evolves, this catches ≥90% of SQL errors before runtime. | None: fully runtime + test-covered. |
| **Async consistency** | Native async: `SqlitePool` runs directly on Tokio multi-thread runtime. Zero-cost from async handlers/tasks. | Indirect: `deadpool-sqlite` marshals `rusqlite` blocking calls via `spawn_blocking`; extra context switch + scheduler pressure. |
| **Migration strategy alignment** | `sqlx migrate` with `migrations/` + timestamped `.sql` files. New DB can use its own sqlx migration track; existing `state.db` untouched. Up/down migrations, tooling richer. | Reuse bespoke migration code verbatim for the new file → maximum consistency. |
| **Ops / debug ergonomics** | Excellent: built-in query log (`RUST_LOG=sqlx=debug`), typed `SqliteError`, pool metrics via `SqlitePool::acquire`, strong `tracing` integration. `cargo sqlx prepare` validates schema locally. | Simple and direct, but debugging relies on manual `println!` or `rusqlite` trace. Pool behaviour is whatever you configure in `deadpool`. |

### Final decision (this repo) — revised 2026-04-17 after PM SSOT conflict review

> **Adopt `sqlx` across the entire workspace.** `state.db` (owned by `nexus-local-db`) is fully migrated from `rusqlite` + `deadpool-sqlite` + bespoke sequential migrations to `sqlx` (sqlite, runtime-tokio, macros, migrate). Orchestration SessionStorage shares the same pool and the same `sqlx migrate` pipeline — it only adds its own `.sql` migration files (one physical file, one engine, one migration runner).
>
> This is a **larger architectural pivot** than the original external comparison contemplated. It is tracked as V1.4 **WS8** (`2026-04-17-v1.4-ws8-local-db-sqlx-migration.md`), and WS2 Task 3 (`SqliteSessionStorage`) is a hard-dependent downstream consumer.
>
> **Minimum viable CI delta (delivered by WS8)**:
>
> 1. Contributors run `cargo sqlx prepare --workspace --all -- --all-targets` locally after changing SQL; commit `.sqlx/`.
> 2. CI sets `SQLX_OFFLINE=true` and runs `cargo sqlx prepare --workspace --all -- --all-targets --check` to detect stale metadata.
> 3. Optional test job: `DATABASE_URL=sqlite::memory:` + `sqlx migrate run` for integration tests that want a live DB.
>
> **Feature flags** (workspace-level, added in WS8 T1): `sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono", "uuid"] }`.
> **Migrations**: unified under `crates/nexus-local-db/migrations/` — the four existing code-form migrations (v2/v3/v4) are ported to timestamped `.sql` files there, and orchestration adds one more migration in WS2 T3.

### Post-decision note (2026-04-17, afternoon)

The original external comparison framed this as "new separate DB file for orchestration, keep rusqlite for state.db" — the cheapest path in the short term. In reviewing how it would intersect with `orchestration-engine.md` §4.3 (which says the orchestration pool *reuses* the nexus-local-db pool), PM flagged that two engines at the same file is technically unstable (rusqlite sync + sqlx async write-lock contention), and two files at two engines leaves permanent operational complexity.

PM pivoted to full sqlx unification. This log preserves the original comparison for archival completeness; the decision above reflects the revised final state. `crate-selection-best-practices.md` §2.3 and §3.3 are the forward-facing SSOT.

---

## 3. Platform user login / OAuth

**Context asked**: Rust implementation of platform user auth. External LLM recommended `oauth2` (v5.x) covering RFC 6749 + Device Authorization Grant (RFC 8628), paired with `jsonwebtoken` for JWT parsing.

**Current repo state**: `crates/nexus42d/src/auth/device_flow.rs` is a deliberate **stub** (`verify_device_code` returns `Ok(false)` unconditionally) per `device-flow-oauth-scope-v1.md` — TD-10 is an intentional deferral until real platform endpoints and CI stubs exist.

### Final decision (this repo)

> **V1.x: use `jsonwebtoken` only** to validate access tokens issued by the platform.
> **Do not introduce `oauth2` crate in V1.x.**
> TD-10 Device Flow remains **deferred** per `device-flow-oauth-scope-v1.md`. When a real implementation is scheduled, `oauth2` v5.x is the first candidate to re-evaluate (status: **deferred**, not rejected).
>
> **Implication for dependency policy**: no OAuth crate enters `[workspace.dependencies]` during V1.4.

---

## 4. Challenge arithmetic evaluation (keep obfuscation layer)

**Context asked**: Evaluate a purely arithmetic expression (digits + `+ - * / ( )`) derived from untrusted, obfuscated input. Must strictly reject function calls, variables, scientific-notation abuse, and DoS via deep nesting.

### Comparison summary

| Dimension | `meval` 0.2.0 | `evalexpr` (~1.6.x) | Hand-rolled shunting-yard |
| --------- | ------------- | -------------------- | -------------------------- |
| **Safety under untrusted input** | Low (built-in functions/variables; scientific notation slips through) | Medium (README states "not built with untrusted input in mind"; `EmptyContext` + `set_builtin_functions_disabled(true)` helps but not bulletproof) | Highest (whitelist lexer rejects any letter, any function/variable name; DoS guards live inside the parser) |
| **Error quality** | Simple `EvalError` (string) | Rich `EvalexprError` with positions | Fully custom; can emit structured positions and domain-specific errors |
| **Perf** | Good | Good (can pre-compile) | Best (zero overhead, non-recursive possible) |
| **Dep footprint** | Zero | Zero | Zero |

**External LLM recommendation**: hand-rolled shunting-yard for strict-untrusted-input scenarios.

### Current repo state

`crates/nexus42/src/challenge/` already ships a hand-written pipeline: `parser.rs` + `eval.rs` + `noise.rs` + `numbers.rs`. The direction is correct.

### Final decision (this repo)

> **Keep the hand-written challenge evaluator.** No new crate introduced.
>
> **Three DoS guards are registered as a follow-up TODO** (documented in `crate-selection-best-practices.md §3 Challenge`; **not** opened as a `residual_finding` in `status.json`, per PM directive 3c=iii):
>
> 1. `max_input_len` (≤ 512–1024 chars)
> 2. `max_paren_depth` (≤ 20–30, tracked inside parser via a depth counter — iterative, not recursive)
> 3. `eval_timeout` (wrap in `tokio::time::timeout` or equivalent)
>
> Surface for future escalation: if challenge abuse is ever reported, promote the TODO to a real residual.

---

## 5. File watching (`notify`)

**Context asked**: Daemon-side change detection on user workspace directories (macOS + Linux; not distributed). Used to invalidate local caches. Full watcher was previously deferred (see `crates/nexus42d/src/workspace/mod.rs` comments).

### Comparison summary

| Dimension | `notify-debouncer-full` (+ `notify` 8) | Raw `RecommendedWatcher` + `Event` |
| --------- | --------------------------------------- | ----------------------------------- |
| **Cross-platform consistency** | Better: merges Rename events, suppresses Modify-after-Create, single Remove for directories. FSEvents/inotify differences mostly hidden. | Significant: FSEvents batches events and has higher latency; inotify is more immediate but sensitive to editor patterns (truncate vs create-replace). Rename often splits into Delete+Create. |
| **Event-loss risk** | Lower: debounce absorbs transient events; file-ID cache (macOS/Windows) reduces Rename loss. Still inherits underlying-OS limits. | Higher: Linux inotify drops silently when queue fills (large trees, many watches); FSEvents has security-policy drops; NFS / network FS often deliver no events at all. |
| **Tokio integration** | Same for both: use `flume` / `crossbeam-channel` / `tokio::sync::mpsc` as `EventHandler`, `select!` or `spawn` on the receiver. `spawn_blocking` is **only** needed for watcher construction (rare), never for event reception. | Same. |
| **Test strategy** | Easy: lower debounce delay (e.g. 100 ms), use `tempfile::TempDir` + std fs ops, assert single debounced event per logical change. | Harder: need explicit filter/dedup test harness. |

### Final decision (this repo)

> **Recommended stack** (when the deferred watcher work lands): `notify 8` + `notify-debouncer-full ~0.4` + async `mpsc` channel.
>
> **V1.4 does not force this to land**; this is guidance for whoever implements the feature. Operational guardrails when it lands:
>
> - **Never watch `$HOME`** — whitelist explicit workspace roots from config; recursive `RecursiveMode::Recursive` only under 10–50 top-level dirs.
> - **Linux**: inspect `fs.inotify.max_user_watches` on start; fall back to `PollWatcher` (with `compare_contents`) above a threshold (e.g. 8192).
> - **Filter early**: ignore `.git/`, `node_modules/`, `target/` at the handler level.
> - **Debounce window**: 500 ms – 2 s.
> - **Backup poll**: run a `tokio::time::interval` (~30 s) to `metadata().modified()`-check critical files, covering event loss.
> - **Restart recovery**: daemon re-registers all watches on startup (no persistence).

---

## 6. Cron / scheduler (far-future)

**Context asked**: Tokio-internal wall-clock scheduler with timezone awareness, task cancellation, graceful shutdown, per-key serialisation.

### Repo constraint (predating the research)

`orchestration-engine.md` **explicitly defers** wall-clock cron to **V1.5+**; V1.4 reserves only the schema. `creator-schedule-and-core-context.md` (WS7) specifies the Schedule state machine and data model but does not implement time triggers.

### Comparison summary (deferred, recorded for later)

| Dimension | `tokio-cron-scheduler` 0.15.x | Hand-rolled `cron` + `chrono-tz` + `tokio::time::sleep_until` |
| --------- | ----------------------------- | -------------------------------------------------------------- |
| **Shutdown semantics** | Good: `scheduler.shutdown().await` stops new triggers; `set_shutdown_handler` for cleanup. Running jobs keep running (no forced abort). | Best: full control via `CancellationToken` + `tokio::select!`. Can precisely wait for orchestration nodes. |
| **Clock-jump / DST** | Known risk (GitHub issue #107): spring-forward / fall-back can delay/double-fire tz-aware jobs. | Full control: recompute next-run each tick from `chrono::DateTime::<Tz>::now()`; detect system-clock jump via `SystemTime` vs `Instant` and recalculate. |
| **Cancellation** | `remove_job(id)` stops future triggers; running jobs need your own `AbortHandle`. | Per-task `CancelHandle` is native. |
| **Per-key serialisation** | Not built-in: add `HashMap<String, Mutex<JoinHandle>>` or semaphores. | Natural: `DashMap<String, Mutex<()>>` or per-key mpsc single-consumer. |
| **Integration cost** | Minimal (direct `JobScheduler::new().await`). | ~150 LOC for scheduler loop; otherwise trivial. |

### Final decision (this repo)

> **V1.4: no selection.** Record the four hard constraints any future implementation must satisfy:
>
> 1. **Wall-clock + tz-aware** correctness (not just `Instant`).
> 2. **Per-key serialisation** (same creator / same Schedule never double-runs).
> 3. **Graceful shutdown** that waits for the currently-running orchestration node to finish.
> 4. **DST / clock-jump safety** (recompute on detection).
>
> Provide a `trait Scheduler` stub in the orchestration crate (or a comment referencing these constraints) so V1.5 can swap in either candidate. Real PoC comparison belongs to a V1.5 plan, not V1.4.

---

## 7. Layered configuration (`figment` vs `config-rs`)

**Context asked**: CLI + daemon need `defaults < TOML file < env` merge, redacted effective-config printing, and error messages that name the source (file/line vs env var).

### Comparison summary

| Dimension | `figment` | `config-rs` | Hand-rolled merge |
| --------- | --------- | ----------- | ------------------ |
| **Type safety** | Best: `#[derive(Deserialize)]` + `Figment::extract::<T>()`; `Default` provider for defaults. | Good: `Config::builder()` + `deserialize`, but more boilerplate. | Good but high boilerplate. |
| **Nested keys** | First-class: env `APP_FOO_BAR=1` → `foo.bar`; TOML nested naturally. | Supported (`__` separator), less intuitive. | Must implement dot-paths yourself. |
| **Array merge semantics** | Flexible: default replace; `join()` concatenates; custom strategies possible. | Weaker: default replace, manual `set_override`, no built-in append. | Fully custom (you decide). |
| **Error diagnosis** | Strongest: errors carry `Metadata` — source file name + line:col, or env-var name + profile. Example: *"invalid type … in TOML file Config.toml:26:10"*. | Moderate: generic messages unless you add `with_source`. | Worst: you attach source at every merge point, else errors collapse to "deserialize failed". |
| **Redacted effective-config printing** | No built-in "redact", but trivial via `secrecy` crate + `#[serde(serialize_with = "redact")]`. | Same — must hand-roll. | Must hand-roll. |

### Final decision (this repo)

> **When layered config becomes needed in `nexus42` / `nexus42d`: use `figment`.**
>
> Standard Provider chain:
>
> ```rust
> figment::Figment::from(Serialized::defaults(Default::default()))
>     .merge(Toml::file("nexus42.toml"))
>     .merge(Env::prefixed("NEXUS42_").split("_"))
>     .extract()
> ```
>
> Redaction convention: wrap secrets in `secrecy::SecretString`; custom `serialize_with = "redact"` emits `***` for effective-config printing.
> **Not required in V1.4**; introduce when a real multi-source config requirement appears. `config-rs` stays a fallback if YAML/JSON5/RON or hot-reload is needed.

---

## 8. Snapshot testing (`insta`) — optional dev-experience

**Context asked**: Rust CLI integration tests asserting JSON response bodies + log fragments, tolerating dynamic fields (timestamps, UUIDs).

### Comparison summary

| Dimension | `insta` + redactions | Hand-rolled golden files |
| --------- | ---------------------- | ------------------------- |
| **Redaction** | Native: `assert_json_snapshot!` with JSONPath-like selectors; `sorted_redaction()`, `rounded_redaction()`. Logs via `assert_snapshot!` + regex filters. | Roll your own (regex / custom serializer); easy to miss fields. |
| **Update workflow** | `cargo insta test` / `cargo insta review` — interactive, diff-aware. | Manual or scripts; no diff tooling. |
| **Dep footprint** | Moderate (insta + JSON + redactions feature; `cargo-insta` dev dep). | Zero. |
| **Error diagnosis** | Excellent coloured diffs preserving structure. | Plain `assert_eq!` output. |
| **Maintenance** | Low: write redactions once. | High: every new dynamic field requires normalisation logic. |

### Final decision (this repo)

> **Recommended but not mandated.** New CLI / HTTP integration tests asserting JSON bodies or log fragments **should default to `insta` + redactions** unless a concrete reason against it (e.g. snapshot > several MB, test must have zero dev-deps) is documented inline.
>
> Three cases where snapshots are **not** appropriate:
>
> 1. Outputs dominated by dynamic data with no stable structure (random orderings, full UUIDs everywhere).
> 2. Assertions where semantic equality matters more than textual equality (e.g. graph isomorphism).
> 3. Very large outputs (> a few MB) where `.snap` files bloat the repo.

---

## 9. Notes / meta

- **Attribution**: the source of the external comparison is not named here, per the "no local privacy in committed text" clause of `AGENTS.md`. The value captured in this log is the **technical content**, not the provenance.
- **Drift rule**: if a decision here is revised, update `crate-selection-best-practices.md` first, then amend this log with a dated entry. Never silently diverge.
- **Consumers**: `crate-selection-best-practices.md`, `v1.4-delivery-compass-v1.md`, `orchestration-engine.md`, `creator-schedule-and-core-context.md`, `2026-04-17-v1.4-ws2-orchestration-skeleton.md`.

---

*Logged: 2026-04-17.*
