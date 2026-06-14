---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-14-v1.46-author-desk-status-ux"
verdict: "Request Changes"
generated_at: "2026-06-14"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-14T16:00:00+08:00

## Scope
- plan_id: `2026-06-14-v1.46-author-desk-status-ux`
- Review range / Diff basis: `merge-base: de30a702 → tip: c9fb1abb (5 commits on iteration/v1.46; equivalent to git diff de30a702..c9fb1abb or git show --stat de30a702..c9fb1abb)`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 3 (`crates/nexus42/src/commands/creator/works/mod.rs`, `.mstar/knowledge/specs/novel-author-experience.md`, `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md`)
- Commit range: `de30a702..c9fb1abb` (T1 `26a09085`, T2 `b26b4415`, T3 `6411e925`, T4 `35f5d085`, merge `a134a98f`, docs `c9fb1abb`)
- Tools run: `cargo clippy -p nexus42 -- -D warnings` (clean), `cargo test -p nexus42 --lib -- 'works::tests'` (40 passed, 0 failed)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-1: Spec §4.1 table marks `findings` "Required: yes" but code omits it on daemon unavailability

**Triggering condition**: The spec §4.1 contract table (`novel-author-experience.md` line 146) declares:

```
| `findings` | array | yes | Same element shape as findings list API; empty array if none |
```

The "Required: yes" column value promises JSON consumers that `findings` is **always present** for novel works. However, the best-effort degradation paragraph (added in the same T4 commit, lines 151–155) states:

> When that endpoint is unreachable, `findings` is **omitted** (rather than fabricated as an empty array)

The code confirms the omission behavior: `enrich_status_json` (`works/mod.rs` line 1141) only inserts `findings` when `Option<&[Value]>` is `Some(...)`, and `handle_status` (line 380–383) passes `None` when `FindingsResult::Unavailable`. Test `enrich_novel_unavailable_findings_omits_key` (line 1678) explicitly asserts the key is absent.

**Impact**: A JSON consumer building schema-validated parsing from the table's "Required: yes" declaration will encounter a missing-key failure when the daemon findings endpoint is transiently unreachable. The three-state contract (present-with-data, present-empty, omitted-when-unreachable) is not reflected in the "Required" column. This is a newly-introduced machine-readable contract (Grill #8); contract ambiguity at P0 will propagate to P1 and platform consumers.

**Suggested fix**: Amend the spec §4.1 table to reconcile the two statements. Either:
- Change "Required" from `yes` to `conditional` and expand the Notes column to describe the three-state contract, OR
- Add a note in the Notes column: "Omitted when the daemon findings endpoint is unreachable (best-effort degradation); present (possibly empty) otherwise."

This is a spec-only fix (no code change needed — the code correctly implements the degradation behavior).

#### W-2: `findings_stale` embeds creator-global scope data inside a work-scoped JSON payload without spec clarification

**Triggering condition**: In `handle_status` JSON branch (`works/mod.rs` lines 385–388), the stale payload is fetched from `/v1/local/findings/stale`:

```rust
let s = client
    .get::<serde_json::Value>("/v1/local/findings/stale")
    .await
    .ok();
```

The daemon handler (`nexus-daemon-runtime/src/api/handlers/findings.rs` line 349–362) confirms this endpoint is **creator-scoped** — it uses `read_active_creator_id()` and returns stale findings across **all** the creator's works, not scoped to the queried `work_id`. Yet `enrich_status_json` (line 1153) embeds this creator-global object directly into the work-scoped JSON response of `works status <work_id> --json`.

The spec §4.1 table (line 147) notes: "Present when 96h master-review stale banner would show (human parity)" — but does not clarify that the scope is creator-global, not work-scoped. In the human output path, the stale banner is printed as a **separate visual block** before the work details (line 414–419), making the global scope implicit. In the JSON path, `findings_stale` is nested inside the work object alongside work-scoped `findings[]`, creating a scope mismatch with no visual or structural separation.

**Impact**: A JSON consumer parsing `output.findings_stale.stale_count` from a work-scoped query would reasonably assume the count represents stale findings **for that specific work**. In reality, it represents stale findings across the creator's **entire workspace**. This could lead to incorrect automation (e.g., a per-work alerting script that fires on a global count, or a dashboard that double-counts stale findings across multiple `works status --json` calls for different works).

Note: `findings[]` itself IS correctly work-scoped (fetched via `/v1/local/works/{work_id}/findings`), which makes the mixed-scope embedding more surprising — two fields in the same JSON object have different scoping.

**Suggested fix**: At minimum, amend spec §4.1 table Notes for `findings_stale` to state: "Creator-global scope (not work-scoped); mirrors the human-path stale banner which is printed before the work block." If a stronger fix is desired in a future plan, consider either renaming to `creator_findings_stale` or filtering the stale count to the queried work (the latter requires a daemon API change and is out of P0 scope).

### 🟢 Suggestion

#### S-1: Dead `let _ = work_id;` statement at end of `print_findings_summary`

**Triggering condition**: `works/mod.rs` line 1310–1312:

```rust
// V1.46 P0 (Grill #7): per-finding routing_hint is the only remediation;
// no blanket reflection-loop footer (work_id unused when findings exist).
let _ = work_id;
```

This statement was added to suppress a presumed unused-variable warning. However, `work_id` IS used earlier in the function (line 1277: `let safe_work_id = sanitize_for_terminal(work_id);` in the empty-findings branch). Since the parameter is used in at least one code path, Rust emits no unused-parameter warning — the `let _ = work_id;` is dead code serving no purpose.

**Impact**: Minor — a future maintainer reading the comment may believe the statement is load-bearing (e.g., a borrow-checker accommodation) and hesitate to remove it. It adds noise without function.

**Suggested fix**: Remove both the comment (lines 1310–1311) and the statement (line 1312). The `work_id` parameter is legitimately used in the empty-findings early-return branch; the non-empty branch simply doesn't reference it, which is normal.

#### S-2: `capture_findings_output` test helper duplicates `print_findings_summary` formatting logic

**Triggering condition**: The test helper `capture_findings_output` (`works/mod.rs` lines 1485–1529) re-implements the same formatting logic as the production `print_findings_summary` (lines 1264–1313). This diff added the `novel-review-master` suggestion to the empty-findings branch in **both** locations (production line 1279 and test helper line 1496), requiring manual lockstep synchronization.

**Impact**: Maintainability — if `print_findings_summary` formatting changes without a corresponding update to `capture_findings_output`, tests will fail for the wrong reason (test-helper drift, not production regression). This is a pre-existing pattern (V1.43 P2), slightly worsened by this diff adding more duplicated lines.

**Suggested fix**: A future refactor (not P0-blocking) could extract the formatting into a pure `fn format_findings_summary(result: &FindingsResult, work_id: &str) -> String` that both `print_findings_summary` (wrapping with `println!`) and `capture_findings_output` (returning the string) call. This eliminates the duplication and makes tests resilient to formatting changes.

#### S-3: JSON-path stale fetch uses default 30s timeout while findings fetch uses 5s

**Triggering condition**: In `handle_status` JSON branch, two daemon calls have different timeout profiles:

- Findings fetch (line 380): `fetch_open_findings` creates a `DaemonClient` with `FINDINGS_FETCH_TIMEOUT` (5s) — fast-fail to avoid blocking the status hot path.
- Stale fetch (line 385–388): uses the default `client` directly (30s `DEFAULT_REQUEST_TIMEOUT`).

This mirrors the existing human-path pattern (line 398 also uses `client` directly for stale), so it is not a regression. However, the JSON path now makes **two** sequential network calls with asymmetric timeouts: worst-case ~35s (5s findings + 30s stale) if both endpoints are slow.

**Impact**: Low — the stale endpoint returns a small object (count + threshold), so 30s is unlikely to be hit in practice. The asymmetry is a minor inconsistency in hot-path protection philosophy.

**Suggested fix**: Consider routing the stale fetch through a similarly shortened timeout for consistency. Not P0-blocking — the behavior is correct and matches the human-path precedent.

## Source Trace

- **Finding ID: W-1**
  - Source Type: manual-reasoning + spec-code cross-check
  - Source Reference: `.mstar/knowledge/specs/novel-author-experience.md` lines 146 vs 151–155; `crates/nexus42/src/commands/creator/works/mod.rs` lines 1141–1146, 380–383; test `enrich_novel_unavailable_findings_omits_key` line 1678–1686
  - Confidence: High

- **Finding ID: W-2**
  - Source Type: manual-reasoning + cross-crate scope verification
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs` lines 385–388, 1147–1155; `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` lines 349–362 (`read_active_creator_id` → creator-scoped); `.mstar/knowledge/specs/novel-author-experience.md` line 147
  - Confidence: High

- **Finding ID: S-1**
  - Source Type: manual-reasoning (dead code analysis)
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs` lines 1275–1280 (use of `work_id`) vs 1310–1312 (dead `let _ = work_id;`)
  - Confidence: High

- **Finding ID: S-2**
  - Source Type: manual-reasoning (code duplication pattern)
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs` lines 1264–1313 (production) vs 1485–1529 (test helper)
  - Confidence: High

- **Finding ID: S-3**
  - Source Type: manual-reasoning (timeout asymmetry)
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs` lines 380 (`fetch_open_findings` → 5s) vs 385–388 (`client.get` → 30s default); `FINDINGS_FETCH_TIMEOUT` const at line 1075
  - Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The implementation is architecturally sound: `enrich_status_json` is a well-documented pure function with comprehensive unit test coverage (8 tests covering novel-only gate, degradation, stale trigger, and field preservation). The novel-only gate (Grill #6), per-finding remediation (Grill #7), and JSON shape (Grill #8) are all correctly implemented. Clippy is clean and all 40 tests pass.

However, two Warning-level findings concern the **newly-introduced JSON contract clarity** (W-1: "Required: yes" vs omission-on-unreachable contradiction; W-2: creator-global `findings_stale` embedded in work-scoped JSON without scope documentation). Both are spec-amend fixes (no code change required) that should be resolved before P1 builds automation on top of this contract. Per the QC gate rules, 2 unresolved Warnings ⇒ Request Changes.

The three Suggestions (dead code, test-helper duplication, timeout asymmetry) are non-blocking maintainability notes.
