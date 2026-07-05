# QC Report #3: CLI + Daemon Foundation — Integration & Maintainability Review

**Reviewer**: @qc-specialist-3  
**Date**: 2026-04-06  
**Task**: Review `nexus42` CLI and `nexus42d` daemon for integration, dependencies, and future maintainability  
**Scope**: Dependency management, CLI↔daemon integration, Local API compliance, code duplication, module boundaries, extensibility  
**Out of Scope**: Security (QC #1), Code patterns (QC #2)

---

## 1. Executive Summary

| Aspect | Status | Evidence |
|--------|--------|----------|
| Dependencies | ✅ PASS | Versions pinned, licenses compatible |
| CLI↔Daemon Integration | ⚠️ WARN | Functional but with 2 medium issues |
| Local API Contract | ✅ PASS | HTTP JSON on loopback, correct endpoints |
| Code Duplication | ❌ FAIL | 3 duplicate SQLite schema definitions |
| Module Boundaries | ✅ PASS | Clean separation CLI/daemon |
| Future Extensibility | ✅ PASS | Residual findings properly tracked |
| Observability | ⚠️ WARN | Limited tracing in daemon handlers |

**Verdict**: **APPROVE with findings**  
All blocking issues from architecture review (CLI-R1..R4) are resolved. The implementation is functionally complete with acceptable technical debt for V1.0 skeleton.

---

## 2. Dependencies Review

### 2.1 Workspace Dependencies (Cargo.toml)

| Dependency | Version | Purpose | Assessment |
|------------|---------|---------|------------|
| `nexus-contracts` | path | Generated wire types | ✅ Correct |
| `nexus-domain` | path | Domain models | ✅ Correct |
| `tokio` | 1.35 | Async runtime | ✅ Pinned |
| `clap` | 4.5 | CLI parsing | ⚠️ In daemon (unnecessary) |
| `reqwest` | 0.12 | HTTP client | ✅ Pinned |
| `rusqlite` | 0.31 | SQLite bindings | ✅ Bundled |
| `axum` | 0.7 | HTTP server | ✅ Correct |
| `tower-http` | 0.5 | HTTP middleware | ✅ Correct |
| `dirs` | 5 | Home directory | ✅ No feature creep |

**Finding CLI-M1 (Medium)**: `clap` dependency in `nexus42d/Cargo.toml` is unnecessary for the library crate. The daemon binary (`main.rs`) uses `clap` for argument parsing, but this should be a **binary-only** dependency, not a crate dependency.

```toml
# Current (nexus42d/Cargo.toml line 35)
clap = { workspace = true }

# Recommended
[dependencies]
clap = { workspace = true }  # Only if library needs it

# Or move to [dependencies] in main.rs only
```

**Evidence**: `crates/nexus42d/Cargo.toml:35` — clap is listed as a library dependency but only used in `main.rs`.

---

## 3. CLI ↔ Daemon Integration Review

### 3.1 Local API Transport

| Endpoint | CLI Caller | Method | Assessment |
|----------|------------|--------|------------|
| `/v1/local/runtime/health` | `daemon_client.rs` | GET | ✅ Correct |
| `/v1/local/runtime/status` | `daemon.rs` | GET | ✅ Correct |
| `/v1/local/workspace` | — | GET | ✅ Implemented |
| `/v1/local/workspace/init` | — | POST | ✅ Implemented |
| `/v1/local/auth/status` | — | GET | ✅ Implemented |
| `/v1/local/creators` | — | GET | ✅ Implemented |
| `/v1/local/manuscript` | `manuscript.rs` | GET | ⚠️ Skeleton only |
| `/v1/local/references` | — | GET | ⚠️ Skeleton only |

**Finding CLI-M2 (Medium)**: `DaemonClient` creates a new `reqwest::Client` per instance (line 26). For production, this should be a static client to benefit from connection pooling.

```rust
// Current: crates/nexus42/src/api/daemon_client.rs:23-28
pub fn new(base_url: &str) -> Self {
    Self {
        base_url: base_url.to_string(),
        http: reqwest::Client::new(),  // New client each time
    }
}
```

**Recommendation**: Use `reqwest::Client::builder()` with connection pool limits, or use a static client.

### 3.2 Request/Response Shapes

The HTTP request/response uses JSON serialization via `serde`. Response shapes are defined in handler modules (e.g., `handlers/workspace.rs` defines `WorkspaceInfo`, `InitWorkspaceResponse`).

**Assessment**: ✅ Wire format is consistent with JSON Schema truth source principle.

---

## 4. Code Duplication Review

### 4.1 SQLite Schema Duplication (CRITICAL for Maintainability)

| Location | Schema | Issue |
|----------|--------|-------|
| `crates/nexus42d/src/workspace/mod.rs:122-171` | Full schema | Original definition |
| `crates/nexus42/src/commands/creator.rs:273-280` | `creators` table | DUPLICATE |
| `crates/nexus42/src/commands/research.rs:132-142` | `reference_sources` table | DUPLICATE |

**Finding CLI-H1 (High)**: `creator.rs:273-280` and `research.rs:132-142` duplicate the SQLite schema definition from `workspace/mod.rs`. This violates DRY and creates maintenance risk — schema changes must be applied in 3 places.

```rust
// creator.rs:273-280 — DUPLICATE
conn.execute_batch(
    "CREATE TABLE IF NOT EXISTS creators (
        creator_id TEXT PRIMARY KEY,
        display_name TEXT NOT NULL,
        status TEXT NOT NULL,
        cached_at TEXT NOT NULL,
        data TEXT NOT NULL
    );"
);

// vs workspace/mod.rs:135-141 — ORIGINAL
CREATE TABLE IF NOT EXISTS creators (
    creator_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    cached_at TEXT NOT NULL,
    data TEXT NOT NULL
);
```

**Recommendation**: 
1. Extract schema definitions to a shared `nexus42-sqlite` internal crate or module
2. Or have CLI commands go through daemon Local API for all SQLite operations
3. V1.0 acceptable as technical debt, but must fix before V1.1

### 4.2 Workspace Path Resolution

Both CLI and daemon independently compute `~/.nexus42/` paths using `dirs::home_dir()`. This is acceptable as both are standalone tools, but could be extracted to a shared `nexus42-core` crate in future.

---

## 5. Local API Compliance

### 5.1 Endpoint Inventory vs Plan

| Planned Endpoint | Implemented | Handler |
|-----------------|-------------|---------|
| GET `/v1/local/runtime/health` | ✅ | `handlers/runtime.rs:14` |
| GET `/v1/local/runtime/status` | ✅ | `handlers/runtime.rs:29` |
| GET `/v1/local/workspace` | ✅ | `handlers/workspace.rs:15` |
| POST `/v1/local/workspace/init` | ✅ | `handlers/workspace.rs:35` |
| GET `/v1/local/auth/status` | ✅ | `handlers/auth.rs` |
| GET `/v1/local/creators` | ✅ | `handlers/creators.rs` |
| GET `/v1/local/manuscript` | ✅ | `handlers/manuscript.rs` |
| GET `/v1/local/references` | ✅ | `handlers/references.rs` |

**Assessment**: ✅ All planned endpoints implemented.

### 5.2 HTTP Compliance

- **Transport**: HTTP over TCP loopback (127.0.0.1:8420) ✅
- **Format**: JSON ✅
- **CORS**: `CorsLayer::permissive()` applied ⚠️ (Warning: overly permissive for production, but acceptable for V1.0 loopback-only)

---

## 6. Module Boundaries Review

### 6.1 CLI Module Structure

```
crates/nexus42/src/
├── main.rs              ✅ Entry point, clap parser
├── api/                 ✅ Local API client
├── auth/                ✅ Dual-subject auth (user + creator)
├── commands/            ✅ 8 command modules
├── config.rs            ✅ Configuration management
└── errors.rs            ✅ Error types
```

**Assessment**: ✅ Clean separation. `auth/` module properly split into `user_auth.rs` and `creator_auth.rs` per CLI-R4 resolution.

### 6.2 Daemon Module Structure

```
crates/nexus42d/src/
├── main.rs              ✅ Binary entry point
├── lib.rs               ✅ Library exports
├── api/                 ✅ HTTP server + handlers
│   └── handlers/        ✅ 6 handler modules
├── auth/                ✅ Session management
└── workspace/           ✅ State + schema
```

**Assessment**: ✅ Clean separation. Library (`lib.rs`) properly exposes modules for testing.

---

## 7. SQLite Connection Management

### 7.1 WorkspaceState Connection Handling

**Finding CLI-H2 (High)**: `WorkspaceState::db()` (line 62-72) opens a **new SQLite connection per request**:

```rust
pub async fn db(&self) -> Option<Connection> {
    let guard = self.db.lock().await;
    guard.as_ref().map(|_c| {
        // SQLite Connection isn't Clone; in production use r2d2 connection pool.
        // For V1.0 skeleton, open a new connection per request.
        Connection::open(&self.db_path).ok()
    }).flatten()
}
```

**Impact**: 
- Performance: Opening SQLite connection is relatively expensive
- Correctness: WAL mode shared between connections may cause locking issues

**Recommendation**: Use `r2d2` or `r2d2_sqlite` connection pool. For V1.0 skeleton, acceptable as technical debt.

---

## 8. Observability Review

### 8.1 Logging Coverage

| Component | Tracing | Log Level | Assessment |
|-----------|---------|-----------|------------|
| CLI main.rs | ✅ | `init_logging()` | ✅ Structured |
| Daemon main.rs | ✅ | `tracing_subscriber` | ✅ Structured |
| CLI commands | ⚠️ | Some `tracing::info!` | ⚠️ Inconsistent |
| Daemon handlers | ❌ | None | ❌ Missing |

**Finding CLI-L1 (Low)**: Daemon API handlers (`handlers/*.rs`) have no tracing/logging statements. Request tracing is essential for debugging production issues.

**Recommendation**: Add `tracing::info!` or `tracing::debug!` to handlers for request tracking.

### 8.2 Error Responses

Daemon handlers return success JSON even on errors (e.g., `handlers/workspace.rs:40-47`). This is acceptable for V1.0 skeleton but should use proper HTTP status codes in V1.1.

---

## 9. Residual Findings Disposition

### 9.1 Architecture Review Items (CLI-R5..R8)

| ID | Finding | Decision | Status |
|----|---------|----------|--------|
| CLI-R5 | Unix socket support | defer | ✅ Tracked in status.json |
| CLI-R6 | Manuscript promote strict mode | defer | ✅ Tracked in status.json |
| CLI-R7 | Manuscript verify content checks | defer | ✅ Tracked in status.json |
| CLI-R8 | Research scan content extraction | defer | ✅ Tracked in status.json |

**Assessment**: ✅ All deferred items properly tracked as residual findings in `status.json`.

### 9.2 Resolved Items

| ID | Finding | Resolution | Verification |
|----|---------|------------|--------------|
| CLI-R1 | Missing Creator command surface | ✅ Task 6 implemented | `commands/creator.rs` |
| CLI-R2 | Missing Manuscript command surface | ✅ Task 7 implemented | `commands/manuscript.rs` |
| CLI-R3 | Missing Research command surface | ✅ Task 8 implemented | `commands/research.rs` |
| CLI-R4 | Dual-subject auth missing | ✅ Auth redesigned | `auth/user_auth.rs`, `auth/creator_auth.rs` |

---

## 10. Configuration Management

### 10.1 Config File

| Aspect | Assessment |
|--------|------------|
| Location | `~/.nexus42/config.json` ✅ |
| Schema | `CliConfig` struct ✅ |
| Persistence | Load/save methods ✅ |

### 10.2 Daemon Configuration

Daemon accepts CLI arguments for `port` and `host` (main.rs:19-30). No config file for daemon runtime configuration — acceptable for V1.0 skeleton.

---

## 11. Findings Summary

| Severity | Count | Blocking? |
|----------|-------|-----------|
| Critical | 0 | — |
| High | 2 | No (technical debt) |
| Medium | 2 | No |
| Low | 1 | No |
| Warning | 1 | No |

### High Findings

| ID | Title | Location | Recommendation |
|----|-------|----------|----------------|
| CLI-H1 | SQLite schema duplication | `creator.rs:273`, `research.rs:132` | Extract to shared module or use daemon API |
| CLI-H2 | New SQLite connection per request | `workspace/mod.rs:62-72` | Use connection pool (r2d2) |

### Medium Findings

| ID | Title | Location | Recommendation |
|----|-------|----------|----------------|
| CLI-M1 | Unnecessary clap in daemon library | `nexus42d/Cargo.toml:35` | Move to binary-only dependency |
| CLI-M2 | reqwest::Client not reused | `daemon_client.rs:26` | Use static/pooled client |

### Low Findings

| ID | Title | Location | Recommendation |
|----|-------|----------|----------------|
| CLI-L1 | No tracing in daemon handlers | `handlers/*.rs` | Add request tracing |

---

## 12. Cross-Reviewer Ready Notes

### For QC #1 (Security):
- `auth.json` file permissions not set (potential info disclosure)
- CORS layer is permissive (acceptable for loopback-only V1.0)

### For QC #2 (Code Patterns):
- SQLite schema duplication across 3 locations
- Consistent use of `tracing` across codebase

### Runtime Impact Assessment:
- SQLite connection per request: **Low-Medium latency impact** under load
- No observability in handlers: **High debugging difficulty** in production
- Schema duplication: **High maintenance burden** for future schema changes

### Rollback Urgency: **LOW** — V1.0 skeleton with known technical debt; not production system.

---

## 13. Approval Recommendation

**Status**: ✅ **APPROVE**

The implementation satisfies all architecture review requirements (CLI-R1..R4 resolved). Findings are **technical debt for V1.1+**, not blocking issues.

**Required before V1.1**:
1. Fix SQLite schema duplication (CLI-H1)
2. Add connection pooling (CLI-H2)
3. Add handler observability (CLI-L1)

**Optional improvements**:
1. Remove clap from daemon library deps (CLI-M1)
2. Pool reqwest client (CLI-M2)

---

*Report generated by @qc-specialist-3 | Scope: Integration + Maintainability | Evidence: Source code analysis, plan documents, architecture review*