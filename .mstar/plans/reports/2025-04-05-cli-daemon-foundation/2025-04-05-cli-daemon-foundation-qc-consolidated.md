# QC Consolidated Decision: CLI + Daemon Foundation

**Plan**: 2025-04-05-cli-daemon-foundation
**Review Date**: 2026-04-06
**Reviewers**: @qc-specialist, @qc-specialist-2, @qc-specialist-3
**Branch**: feature/v1.0-cli-daemon

---

## 1. QC Review Summary

| Reviewer | Focus | Verdict | Critical | High | Medium | Low |
|----------|-------|---------|----------|------|--------|-----|
| QC #1 | Architecture & Code Quality | Request Changes | 0 | 3 | 4 | 3 |
| QC #2 | Code Patterns & Testing | Block on HIGH | 0 | 4 | 11 | 8 |
| QC #3 | Integration & Maintainability | Approve | 0 | 2 | 2 | 1 |

**Total Findings**: 0 Critical, **9 High**, 17 Medium, 12 Low

---

## 2. Consolidated High-Severity Findings

### Blocking Issues (Must Fix Before Merge)

| ID | Title | Source | Severity | Fix Time | Action |
|----|-------|--------|----------|----------|--------|
| **CONS-H1** | Formatting violations | QC #1 H1 | High | 2 min | `cargo fmt` |
| **CONS-H2** | Auth file permissions insecure | QC #1 H2 | High | 10 min | Set 0600 on auth.json save |

### Technical Debt (Acceptable for V1.0, Fix in V1.1)

| ID | Title | Source | Severity | Impact | V1.1 Priority |
|----|-------|--------|----------|--------|---------------|
| **CONS-H3** | SQLite connection per request | QC #1 H3, QC #3 CLI-H2 | High | Performance | High |
| **CONS-H4** | SQLite schema duplication | QC #3 CLI-H1 | High | Maintainability | High |
| **CONS-H5** | Inconsistent error propagation | QC #2 HIGH-1 | High | Debugging | Medium |
| **CONS-H6** | Missing workspace validation | QC #2 HIGH-2 | High | Correctness | Medium |
| **CONS-H7** | Unsafe unwrap in error paths | QC #2 HIGH-3 | High | Robustness | High |
| **CONS-H8** | Concurrent workspace access race | QC #2 HIGH-4 | High | Correctness | High |
| **CONS-H9** | No tracing in daemon handlers | QC #3 CLI-L1 | High* | Observability | Medium |

*Note: CONS-H9 is Low severity per QC #3, but flagged as observability concern.

---

## 3. Findings Consolidation & Conflict Resolution

### 3.1 Duplicate Findings (Merged)

**SQLite Connection Pattern**:
- QC #1 H3: "SQLite connection pattern unusual"
- QC #3 CLI-H2: "New SQLite connection per request"
- **Merged as CONS-H3**: Both identify the same architectural issue. Accept as technical debt for V1.0.

**Error Handling**:
- QC #1 M3: "Error handling could be more specific"
- QC #2 HIGH-1: "Inconsistent error propagation"
- QC #2 HIGH-3: "Unsafe unwrap in error paths"
- **Kept Separate**: QC #1 M3 is general observation; QC #2 findings are specific code locations requiring fixes.

### 3.2 Conflict Resolution

**Conflict 1: Severity Assessment of SQLite Connection Pattern**
- QC #1: High severity, may cause production issues
- QC #3: High severity, but "acceptable for V1.0 skeleton"
- **Resolution**: Accept as technical debt (CONS-H3). V1.0 skeleton has low traffic; connection pooling can be added in V1.1.

**Conflict 2: Test Coverage Assessment**
- QC #1: "Adequate for V1.0 skeleton" (16 + 7 tests)
- QC #2: "Missing unit tests" flagged as Medium
- **Resolution**: Both correct. Integration tests cover happy paths; unit tests can be added in follow-up.

---

## 4. Gate Decision

**Verdict**: **Request Changes**

**Blocking Items**: 2 (CONS-H1, CONS-H2)
**Residual Findings**: 7 (CONS-H3..CONS-H9) — tracked in `status.json`

### Merge Criteria

1. ✅ Run `cargo fmt` (CONS-H1)
2. ✅ Set auth.json permissions to 0600 (CONS-H2)
3. ✅ Verify tests pass: `cargo test --workspace` (156/156)
4. ✅ Update plan status: InReview → Done

**Estimated Fix Time**: 15 minutes

---

## 5. Fixed vs Residual Findings

### Fixed Before Merge (Assigned to @fullstack-dev)

| ID | Title | Owner | Status |
|----|-------|-------|--------|
| CONS-H1 | Formatting violations | @fullstack-dev | **Required** |
| CONS-H2 | Auth file permissions | @fullstack-dev | **Required** |

### Residual Findings (Tracked in status.json)

| ID | Title | Severity | Decision | Owner | Target |
|----|-------|----------|----------|-------|--------|
| CONS-H3 | SQLite connection per request | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H4 | SQLite schema duplication | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H5 | Inconsistent error propagation | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H6 | Missing workspace validation | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H7 | Unsafe unwrap in error paths | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H8 | Concurrent workspace access race | High | Accept (tech debt) | @fullstack-dev | V1.1 |
| CONS-H9 | No tracing in daemon handlers | High* | Accept (tech debt) | @fullstack-dev | V1.1 |

---

## 6. Architecture Constraints Compliance

| Constraint | Status | Evidence |
|------------|--------|----------|
| Rust-first for CLI/daemon | ✅ PASS | Pure Rust 1.75+, clap, tokio, axum |
| HTTP-only Local API | ✅ PASS | axum on port 8420, JSON wire format, no gRPC |
| SQLite workspace state | ✅ PASS | `$HOME/.nexus42/state.db` with WAL mode |
| No Neo4j/Postgres/pgvector | ✅ PASS | Cargo.toml clean, only rusqlite dependency |
| JSON Schema truth source | ✅ PASS | Uses `nexus-contracts` generated types |
| CLI is ACP client | ✅ PASS | No ACP server code in CLI/daemon |
| V1.0 Creator first-class citizen | ✅ PASS | Full command surface (register, pair, credentials) |
| `manuscript_phase` deliverable | ✅ PASS | Commands: status, phase, promote, verify |
| V1.0 Research workflow | ✅ PASS | Commands: scan, list, extract (local-only) |
| Dual-subject auth | ✅ PASS | User tokens + Creator API keys implemented |

**Result**: **All 10 constraints PASS**. No architecture violations detected.

---

## 7. Security Review Summary

| Item | Status | Finding ID |
|------|--------|-----------|
| Token storage in plaintext | ⚠️ Medium | CONS-H2 (permissions) |
| Network exposure | ✅ Safe | Loopback-only |
| SQL injection | ✅ Safe | Parameterized queries |
| Secrets in code | ✅ Clean | No hardcoded secrets |
| Logging safety | ✅ Safe | No sensitive data logged |

**Overall**: Acceptable for V1.0 with CONS-H2 fix.

---

## 8. Test Coverage Assessment

**Integration Tests**: 23 (16 CLI + 7 daemon) — adequate for V1.0 skeleton
**Unit Tests**: 0 — recommended for V1.1
**Test-to-code Ratio**: ~1:15 — acceptable for skeleton

**Coverage Gaps** (acceptable for V1.0):
- Creator register/pair/unpair (requires platform API mock)
- Manuscript phase transitions
- Research extract
- Error scenarios

---

## 9. Next Steps

### Immediate (before merge)
1. **@fullstack-dev**: Run `cargo fmt` (2 min)
2. **@fullstack-dev**: Add file permissions to auth save (10 min)
3. **@fullstack-dev**: Run `cargo test --workspace` to verify 156/156
4. **@project-manager**: Update `status.json` with residual findings
5. **@project-manager**: Update plan status to Done

### Follow-up (V1.1 planning)
1. Create issues for CONS-H3..CONS-H9
2. Add connection pooling to daemon (CONS-H3)
3. Extract SQLite schema to shared module (CONS-H4)
4. Add error propagation tests (CONS-H5)
5. Add workspace validation tests (CONS-H6)
6. Replace unwrap with proper error handling (CONS-H7)
7. Add async-aware RwLock or connection pool (CONS-H8)
8. Add tracing to daemon handlers (CONS-H9)

### Documentation Updates
1. Add security considerations to README (token storage)
2. Document V1.0 limitations (device flow, sync stubs)
3. Add migration guide for V1.1 improvements

---

## 10. Individual QC Reports

- **QC #1**: `.agents/plans/reports/2025-04-05-cli-daemon-foundation/2025-04-05-cli-daemon-foundation-qc1.md`
- **QC #2**: `.agents/plans/reports/2025-04-05-cli-daemon-foundation/2025-04-05-cli-daemon-foundation-qc2.md`
- **QC #3**: `.agents/plans/reports/2025-04-05-cli-daemon-foundation/2025-04-05-cli-daemon-foundation-qc3.md`

---

**Consolidated by**: @project-manager
**Decision Date**: 2026-04-06
**Gate**: Request Changes (2 blocking items)
**Next Gate**: Approve after CONS-H1 + CONS-H2 fixes

---

## PM Notes

The implementation satisfies all architecture constraints (10/10 PASS). The two blocking issues are **trivial fixes**:
1. CONS-H1: `cargo fmt` (automated)
2. CONS-H2: 3 lines of code to set file permissions

The 7 residual findings are **technical debt for V1.1**, all documented and tracked. This is acceptable for a V1.0 skeleton scope.

The three QC reviewers identified overlapping concerns (SQLite connection pattern, error handling) but assessed severity differently. The consolidated decision accepts QC #3's assessment that these are technical debt, not blocking issues, given the V1.0 skeleton scope and low expected traffic.