---
report_kind: qc-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-12-v1.43-author-visibility
verdict: Approve
generated_at: 2026-06-12T21:20:00+08:00
---

# Code Review Report — P2 (author-visible UX)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-12T14:42:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-author-visibility
- Review range / Diff basis: merge-base: 04c2490d + tip: 6e6b03bb
- Working branch (verified): feature/v1.43-author-visibility
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p2
- Files reviewed: 3
- Commit range: 04c2490d..6e6b03bb
- Tools run: git diff, git log, git status, git rev-parse, read (source + daemon handler + spec + quickstart + plan), cargo +nightly fmt --all --check, cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings, rg for TODO/unsafe/paths/secrets, manual API contract trace against daemon findings handler.

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- **W-01 Silent error swallowing in fetch_open_findings produces misleading "findings: none open"** → `fetch_open_findings` does `client.get(...).await.ok().and_then(|v| v.as_array()...).unwrap_or_default()`. On any failure (network hiccup on the findings subcall, auth edge, handler error, or temporary DB issue for findings table — even after the primary `/works/{id}` succeeded), `print_findings_summary` emits "findings: none open". User cannot distinguish "truly zero open findings" from "findings unavailable". The chapter table (from local DB via the main work fetch) still renders, so partial success is possible. Per cli-spec §7.1 (cited in code comments) and the R#1 observation in the implementing report: errors on secondary status enrichment should be observable/actionable rather than silently defaulting to a semantically different user message. The 96h stale banner uses similar best-effort but only adds output on >0; here the default is the "good" message. **Fix suggestion**: on `.get` Err, either skip the findings block entirely, or print a distinct line e.g. "findings: unavailable (daemon error)" while still showing the chapter table. This is Warning (not Critical) because the primary status path fails hard on daemon unreachable, and findings is explicitly best-effort per comments.
  - Source: crates/nexus42/src/commands/creator/works/mod.rs:762 (fetch_open_findings), 332 and 347 and 400 (call sites), 868 (print_findings_summary default path).

- **W-02 User-supplied data (finding title, routing_hint, work_id) interpolated into terminal output without sanitization** → `print_findings_summary` and the review hint line do direct `format!` / `println!` of `title` (from create-finding user input, stored in DB, returned by daemon), `severity` (string), `routing_hint`, and `work_id` (from active-work resolution or user arg). Example:
  ```rust
  let display_title = truncate_with_ellipsis(title, 48);
  println!("  #{} [{sev}] \"{display_title}\" {hint}", i + 1);
  println!("  Address findings or run: nexus42 creator run stage advance {work_id} --stage review");
  ```
  `truncate_with_ellipsis` is a simple byte slice + "…"; no ANSI stripping, no control-char filtering. Finding titles are author-supplied (via POST /findings or from-review hook). A malicious or corrupted title containing `\x1b[31m`, `\r`, newlines, or other terminal controls could corrupt the status display, inject fake lines, or (in extreme terminal emulators) trigger other side effects. `work_id` is more trusted (minted + existence-checked), but still flows from daemon response. This is a display-layer hygiene / correctness risk for untrusted user data in CLI output. Not Critical (no RCE, not a web context, IDs are constrained), but should be fixed before broader author visibility surfaces. **Fix suggestion**: add a lightweight `sanitize_for_terminal` (strip ASCII control chars except \n/\t if wanted, or at minimum strip ESC/CSI) before any user field goes into `println!` for this path; or use a safe formatting helper. Consider the same for other status strings that embed DB content.
  - Source: crates/nexus42/src/commands/creator/works/mod.rs:893 (title), 899 (work_id), 868-901 (print_findings_summary), 956 (truncate), daemon side: findings.rs:48 (title from user request), 835 (FindingApiDto).

### 🟢 Suggestion
- **S-01 Severity handling is graceful but not future-proof for unknown values** → `from_findings_json` does `unwrap_or("info")` for ranking and treats only the four known strings as orderable. Unknown severity strings (e.g. future "critical", or a typo "majr") are counted, displayed literally in the top-findings lines, but never become `highest_severity` and sort to the end of `severity_counts`. No panic/crash. The daemon side stores severity as free string (no enum in the handler DTO or visible creation validation in the reviewed slice). This is acceptable for P2 (current findings are created with known values from review presets / manual), but if the set expands the CLI will silently mis-rank. **Recommendation**: either (a) centralize the severity vocabulary in a shared constant/enum used by both daemon creation and CLI parser, or (b) document the closed set in novel-writing/quality-loop.md §2 and add a test that unknown severities are at least preserved in display. Low priority for this plan.
  - Source: crates/nexus42/src/commands/creator/works/mod.rs:798 (order), 806 (unwrap_or), 812-817 (rank logic), 839 (display), daemon findings.rs:36 (String), 70 (CreateFindingRequest).

- **S-02 Minor test duplication of formatting logic** → `capture_findings_output` manually re-implements the summary line + top findings + hint formatting that lives in `print_findings_summary`. This is because `println!` is hard to capture in unit tests without global writer redirection. The tests themselves are high quality (hermetic, real JSON payloads via `finding_json`, assert on counts/ordering/highest/truncation/display strings, 10 tests covering empty/single/mixed/top-cap/completed paths). The duplication is a maintainability tax if the output format changes. Acceptable for V1.43 P2; consider extracting a pure `format_findings_summary_lines(...) -> Vec<String>` in a follow-up so the display fn and tests share the exact logic.
  - Source: crates/nexus42/src/commands/creator/works/mod.rs:1032 (capture helper), 1072-1122 (the 10 tests).

## Source Trace
- Finding ID: W-01
- Source Type: api-contract-check + manual-reasoning + code-read (error path)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:762 (fetch + .ok().unwrap_or_default), 868 (print default), daemon findings.rs:180 (list handler that would be called)
- Confidence: High

- Finding ID: W-02
- Source Type: git-diff + manual-reasoning (user data provenance)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:835 (title extraction), 893 and 899 (interpolation sites), 956 (truncate), daemon findings.rs:48 and 168 (title from CreateFindingRequest)
- Confidence: High

- Finding ID: S-01
- Source Type: git-diff + api-contract-check
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:798-817 (severity_order + rank), daemon findings.rs:36 (severity: String, no enum)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

## API contract audit (qc-specialist-2 focus)
- New call: `GET /v1/local/works/{work_id}/findings?status=open&limit=50`
- Daemon handler: `list_findings_handler` (findings.rs:180) exists, accepts `ListFindingsQuery { status: Option<String>, ... }`, builds `FindingListFilters { status, ... }`, calls `findings::list_findings`, maps to `Vec<FindingApiDto>`.
- Response shape: `FindingApiDto { finding_id, work_id, chapter, severity: String, status, title: String, ..., routing_hint: Option<String>, ... }` — CLI reads exactly `severity`, `title`, `routing_hint` (plus implicit status filter). Extra fields ignored. Shape matches; no drift.
- Query param: `status=open` is the documented filter (not `severity`). Limit honored.
- Wire contract note: this is a local daemon HTTP API (not the public ACP/schemas/ contracts). No schema/ codegen mismatch introduced.
- Best-effort call site does not change the primary work fetch contract.

## Silent error swallow audit (R#1)
- Confirmed: `fetch_open_findings` silently returns `[]` on any `client.get` failure.
- Output: "findings: none open" (identical to the zero-findings happy path).
- Context: called only after primary `/v1/local/works/{id}` succeeded (so daemon was reachable for the main object), but the secondary findings list can still fail independently.
- The chapter table (local DB via the work object) will still print.
- Per assignment and cli-spec §7.1: this is the exact misleading case flagged. Not Critical because overall status command fails hard on primary daemon unreachability, but it is a correctness/UX observability gap for the new feature. See W-01.

## User-data injection audit (display layer)
- Finding titles and routing hints originate from user input (create finding or from-review verdict) → stored in local DB → returned verbatim by daemon → interpolated into `println!`.
- No sanitization between DB round-trip and terminal emission.
- Risk: terminal control sequences / ANSI escapes / newlines in titles (possible if author pastes rich text or attacker compromises the local creator account).
- `work_id` in the action hint is more constrained but still not escaped.
- Terminal (not HTML/JS) lowers the blast radius, but display corruption and user confusion are real. See W-02.
- Pre-existing status output already prints other DB strings (title, chapter titles, etc.); this change adds more user-controlled fields to the same surface.

## Severity enum handling
- CLI: string-based, `unwrap_or("info")` for ranking, closed list of 4 for `highest_severity` computation and sort. Unknown strings are preserved for display in top findings and counted under their own key (sort to end).
- No crash on unknown severity.
- Daemon: `severity: String` in DTO and request; creation accepts whatever the caller (review preset or manual) sends. No server-side enum in the reviewed handler.
- Result: graceful degradation today; future severity additions will require CLI code change to be ranked correctly. See S-01. Acceptable for P2.

## System invariants audit (5 checked)
1. "Do not sync full manuscript text by default" — new code only fetches findings metadata (id, title, severity, hint, status). No chapter body / manuscript content. Invariant holds.
2. "Wire contracts must match schemas" — findings are internal local daemon API (not exposed via the public ACP/schemas/ JSON Schema contracts that drive codegen). CLI uses dynamic `serde_json::Value` for the best-effort enrichment (consistent with other status best-effort paths like stale banner). No generated type drift.
3. "World history is immutable" — not touched; findings are mutable per-work quality notes.
4. "Daemon runtime is client-only, not an ACP Agent/Server" — the new call is just another local HTTP client call from the CLI (same pattern as all prior status/pool calls). No change to daemon role.
5. Single truth source for DTOs — CLI does not hand-write a parallel Finding struct; it consumes the JSON shape produced by the daemon's `FindingApiDto` (which itself comes from the local DB `Finding` row). Good.

## Test quality audit (10 tests)
- Location: `crates/nexus42/src/commands/creator/works/mod.rs` lines 982-1123 (inside `#[cfg(test)] mod tests`).
- All 10 are **hermetic unit tests** on the new pure logic (`FindingsSummary::from_findings_json` + `truncate_with_ellipsis` + formatting reconstruction).
- Payload construction: `finding_json(...)` builds real `serde_json::Value` objects matching the daemon array shape (not pre-parsed structs). Good.
- Coverage: empty, single, mixed severities (ordering + highest), top-5 cap, display reconstruction for zero / with findings / completed-work path, truncation edges.
- No shared state, no DB, no daemon client, no `handler_state()`. (handler_state is the integration pattern for DB-backed handler tests elsewhere; not applicable here.)
- Not snapshot-only: explicit asserts on counts, vec contents, strings, ordering.
- One minor maintainability note (duplicated formatting logic in the capture helper) — see S-02.
- Overall: real, valuable, correctly scoped unit tests for the new display code. No red flags.

## Risks / open questions for PM
- The two Warnings are the only blockers. Both are "best-effort hygiene" issues rather than "feature is broken at runtime."
- If the implementing team prefers to treat "findings: none open on transient subcall error" as acceptable (because the primary work object succeeded and findings is explicitly documented best-effort), the W-01 severity could be downgraded to Suggestion after discussion — but the current implementing report and assignment both flag it, so treat as actionable.
- Terminal sanitization (W-02) is a broader surface (other status fields already emit DB titles); fixing only the new findings lines is a start but the class may deserve a small shared helper in a later hygiene plan.
- No other security surface (no new auth, no path handling, no secrets, no SQL in the changed CLI code).

## Self-attestation
Report committed; report frontmatter complete; no invented findings; verdict rationale documented. All alignment fields (cwd/branch/range/plan_id) were re-verified at start of session and match Assignment exactly. No subagent delegation occurred. Review executed entirely in the assigned worktree on the specified diff basis.

## Revalidation (post-fix wave, fix commit 0d6b072f)

**Re-review mode**: Targeted — qc-specialist-2 only (raised 2 blocking Warnings in initial wave)
**Fix range reviewed**: 6e6b03bb..0d6b072f
**Files in fix wave**: crates/nexus42/src/commands/creator/works/mod.rs (+224/-35)

### Previously raised blocking findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc2-W-01 | Silent API error → "none open" misleading | PASS | `FindingsResult` enum (Fetched / Unavailable) at line 770; `fetch_open_findings` now returns `FindingsResult`; `print_findings_summary` matches on `Unavailable` → prints "findings: unavailable (daemon error)" (distinct from "none open"); new test `display_unavailable_findings` (line 1294) asserts the unavailable message contains "unavailable" and does NOT contain "none open". |
| qc2-W-02 | User-data terminal sanitization missing | PASS | `sanitize_for_terminal` helper at line 1021 strips ANSI CSI (`\x1B\[...`) and ASCII control chars 0x00-0x1F (except \n/\t) + 0x7F (DEL); preserves \n, \t, Unicode, printable; applied to title, routing_hint (top findings), and work_id (action hint) — 3 call sites in `print_findings_summary` (lines 941-942, 948) plus mirrored in `capture_findings_output` (lines 1152-1153, 1160); 6 new tests: `sanitize_for_terminal_strips_escape_codes`, `sanitize_for_terminal_preserves_unicode`, `sanitize_for_terminal_strips_control_chars`, `sanitize_for_terminal_strips_del`, `sanitize_for_terminal_preserves_newline_and_tab`, `sanitize_for_terminal_strips_clear_screen`. |

### Static checks (re-run on full P2 feature scope 04c2490d..0d6b072f)
- `cargo +nightly fmt --all --check`: PASS (no output = clean)
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`: PASS (finished dev profile, no warnings emitted)
- Test counts: nexus42 635 passed (0 failed), daemon-runtime 186 passed (0 failed), orchestration 559 passed (0 failed, 1 ignored) — 0 failed across scope

### Updated verdict
**Verdict**: Approve
**Rationale**: Both blocking Warnings (qc2-W-01, qc2-W-02) are resolved with distinct error state, sanitization helper + call sites, and new covering tests. Static checks and scoped lib tests are clean. No unresolved Critical or Warning from this re-review. Per gate rule: Critical=0 and Warning=0 (unresolved from this re-review) → Approve.
