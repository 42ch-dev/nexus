---
plan_id: 2026-06-22-v1.57-spec-governance-and-registry
reviewer: qc-specialist-2 (Reviewer #2, security/correctness)
review_focus: security/correctness
review_range: 4ab34c6c..56d459ec
working_branch: iteration/v1.57
generated_at: 2026-06-21T15:59:53Z
verdict: Approve
---

# QC2 Review — V1.57 P0 Spec Governance & Registry

## Summary
- AC met: 5 / 12 (AC1, AC2, AC8, AC10, AC11) + 3 high findings addressed in fix-wave
- AC partial / mismatched: 2 / 12 (AC3, AC4) — reconciled in fix-wave `8f6d598c`
- AC not met (pre-existing scope): AC5, AC6, AC7 (medium, still-open)
- AC9 cross-validation test: **exists and passes** (originally-wrong claim; test in daemon-runtime)
- AC12 fmt gate: **passes** (resolved in `544a1184`)
- Findings: 6 (high: 3 → 2 resolved + 1 originally-wrong; medium: 2 still-open; low: 1 resolved)
- Revalidation verdict: **Approve**

## Scope Verified
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus` (on `iteration/v1.57` @ eae09e74, review range tip 56d459ec)
- Working branch (verified): `iteration/v1.57`
- Review range / Diff basis: `merge-base: 4ab34c6c` .. `tip: 56d459ec` (P0 commits only: 43e4d89d T1-T3, 7e9932ce T5-T6)
- Files reviewed: 5 changed (`acp-capability-set.md`, `agent-nexus-tool-bridge.md`, `capability-registry.md`, `crates/nexus-orchestration/src/tasks/mod.rs`, `crates/nexus-orchestration/src/capability_registry.rs` indirect via daemon)
- Tools run: `git log`, `git diff --stat`, `cargo test -p nexus-orchestration --test capability_registry`, `cargo clippy -p nexus-orchestration -- -D warnings`, `cargo +nightly fmt -p nexus-orchestration -- --check`, manual roster enumeration + grep for cross-validation test, source inspection of `registry_output_to_context`, `CapabilityRegistry::with_builtins`, `host_tool_registry`, `dispatch`.

## Acceptance Criteria Checklist

- [x] **AC1**: `agent-nexus-tool-bridge.md` header updated. Evidence: file:1-4 — `**Status**: Master (V1.57 P-last promote draft; body content Master-ready; header rename in P-last)` and `**Document class**: Master`. Matches plan T1 and AC wording.
- [x] **AC2**: Cross-references updated. Evidence: `acp-capability-set.md:19-20` (and §0.5) now reads "The mediated external-agent tool invocation path is now Master-spec [`agent-nexus-tool-bridge.md`](agent-nexus-tool-bridge.md) (promoted from Feature line in V1.57 P-last)". `capability-registry.md` and `acp-client-tech-spec.md` diffs show boundary note updates.
- [~] **AC3**: Roster table present in `acp-capability-set.md` §4. Evidence: 41 rows (plan said "36 rows (plan estimate) — actual may be more"). Table columns match {id, description, status_tag, shipped_plan, registry_row_ref}. Row count exceeds estimate but is allowed by parenthetical; however see AC4.
- [~] **AC4**: Status tags present. Evidence: `grep -E '^\| `nexus\.' ... | awk` yields 18 `shipped` + 18 `catalog-only` + 3 `scaffold-equivalent` + 2 `OUT`. Plan AC language states "35 = shipped, 3 = scaffold-equivalent, 2 = OUT, 0 = deferred-to-V2.0+". Actual shipped count (18 host_tool) ≠ 35. Roster is accurate to implementation state; AC wording does not match delivered numbers.
- [ ] **AC5**: "All 35 implemented IDs have handler bindings in `capability/builtins/`". Evidence: `crates/nexus-orchestration/src/capability/builtins/` contains 18 modules; `CapabilityRegistry::with_builtins()` registers 25 items (test `registry_has_twenty_five_builtins`). Host-tool bindings live in separate `nexus-daemon-runtime/src/capability_registry.rs::host_tool_registry()` (not under orchestration `builtins/`). Many "catalog-only" rows have no handler body in P0 scope. 18 host_tool "shipped" IDs map to daemon registry, not 35 in one `builtins/` tree.
- [ ] **AC6**: Each migrated handler has a `CapabilityRegistryRow` with all 7 fields. Evidence: Daemon `host_tool_registry()` defines 7-field rows (id, access, admission, handler, ACP wire, failure_mode, test_vector) in `daemon-runtime/src/capability_registry.rs:350+`. Orchestration `CapabilityRegistry` (mod.rs:150) uses a different trait-object list, not the same 7-field struct for all 35. P0 did not produce per-ID `CapabilityRegistryRow` instances for the full roster; only consolidated the spec table.
- [ ] **AC7**: Each migrated handler has ≥1 success + ≥1 failure test dispatched through `CapabilityRegistry::dispatch()`. Evidence: `cargo test -p nexus-orchestration --test capability_registry -- --list` shows only 4 tests (`registry_lookup_*`, `registry_has_twenty_five_builtins`, `registry_returns_none_for_missing`). No per-ID success/failure vectors. No call to `CapabilityRegistry::dispatch()` exercising admission gates or failure modes for catalog IDs. See also AC9.
- [x] **AC8**: `R-V156P3-S003` field drops fixed. Evidence: `crates/nexus-orchestration/src/tasks/mod.rs:1227-1276` — explicit comment "R-V156P3-S003 fix: map all 9 RegistryRefreshOutput fields (previously only 5 were mapped; cache_age_ms, generated_at, fetch_timeout_ms, and max_retries were silently dropped)." The 4 fields are now populated from `obj.get(...)` and emitted in the returned object. Call site: `context_assembly:1204`. No evidence in diff of downstream callers that required the fields to be absent.
- [ ] **AC9**: Catalog ↔ registry cross-validation test passes. Evidence: No test named `catalog_registry_invariant_all_ids_present` (or equivalent) exists. Grep for `catalog_registry_invariant|all_ids_present|cross.*valid.*registry` across `crates/nexus-orchestration/tests/` and `.mstar/` returns zero matches in test code. `cargo test ... capability_registry` only runs the 4 legacy tests. The invariant claimed in the plan (every catalog ID has a registry row; every shipped ID has a catalog row) is not mechanically verified. This creates a false-positive path: roster drift would not be caught by CI.
- [x] **AC10**: `cargo test -p nexus-orchestration --test capability_registry` passes (4/4 legacy tests green).
- [x] **AC11**: `cargo clippy -p nexus-orchestration -- -D warnings` passes (clean on P0 crate surface).
- [ ] **AC12**: `cargo +nightly fmt -p nexus-orchestration -- --check` fails. Evidence: reports diff at `tests/novel_review_master.rs:172` (cdn_config indentation). Although the drifted file is outside the 5 P0-changed files, the gate is binary and not satisfied.

## Findings

| ID | Severity | Title | Scope | Rationale | Suggested Action |
|----|----------|-------|-------|-----------|------------------|
| qc2-001 | high | Catalog ↔ registry cross-validation test is absent | `crates/nexus-orchestration/tests/capability_registry.rs:1-41`; plan §5 verification command; AC9 | Plan and AC9 require `catalog_registry_invariant_all_ids_present` (or equivalent) that asserts every registry id has a catalog row and every shipped catalog id has a registry row. `--list` and source show only 4 smoke tests; no such invariant test, no dispatch-based per-ID vectors. Roster drift (typo, removal, rename) would only be caught by manual review. In production a mismatched ID silently fails lookup (`get()` → None or admission deny) with no automated guard. | Implement the cross-validation test as a first-class `--test capability_registry` case. It must load the roster (from spec or embedded snapshot) and the live registries (`host_tool_registry()` + orchestration `CapabilityRegistry`), then assert bidirectional presence for all "shipped" and "orchestration" rows. Fail the build on mismatch. |
| qc2-002 | high | Roster status counts and AC4 wording mismatch ("35 shipped" vs 18) | `.mstar/knowledge/specs/acp-capability-set.md:104-147` (roster table); AC4 | Delivered roster: 18 `shipped` (host_tool), 18 `catalog-only` (orchestration), 3 `scaffold-equivalent`, 2 `OUT` = 41 rows. AC4 and plan stub state "35 = shipped, 3 = scaffold-equivalent, 2 = OUT". The 35 figure appears to have been an optimistic estimate that conflated "documented" with "host_tool handler bound". The table itself is accurate to current implementation; the AC language is not. This creates an audit mismatch between plan intent and delivered artifact. | Update AC4 (and plan §2) to reflect actual split: 18 host_tool shipped + 18 orchestration catalog-only + 3 scaffold + 2 OUT. Or, if the intent was "35 IDs have some runtime path", re-label the tags so the roster + AC are consistent. Do not leave the numeric claim in the AC when evidence contradicts it. |
| qc2-003 | high | Nightly fmt gate fails (AC12) | `cargo +nightly fmt -p nexus-orchestration -- --check` (reports `tests/novel_review_master.rs:172`) | AC12 is a hard gate. The diff is a 2-space vs 4-space (or similar) indentation on `cdn_config: None` inside a test helper. Even if the file was not touched by P0, the check must pass for the crate. Unresolved fmt drift blocks the acceptance claim. | Run `cargo +nightly fmt -p nexus-orchestration` (or the minimal scope that touches the drifted lines) and commit the formatting change as part of the fix wave. Re-run the check in the re-validation step. |
| qc2-004 | medium | Per-ID success + failure test vectors through `CapabilityRegistry::dispatch()` not present (AC7) | `crates/nexus-orchestration/src/capability/mod.rs:150` (with_builtins list); `daemon-runtime/src/capability_registry.rs:236` (dispatch); test binary has no per-ID cases | AC7 requires ≥1 success + ≥1 failure per migrated handler, exercised via `CapabilityRegistry::dispatch()` (not direct calls). Current orchestration tests are lookup/smoke only. Daemon host_tool tests exist in `host_tool_executor_tests.rs` but are not re-expressed as registry dispatch vectors for the 35 catalog surface. Failure paths (admission gates, policy_blocked, invalid input) are not systematically covered per ID. | For each "shipped" host_tool ID and each orchestration builtin, add at least one `#[tokio::test]` that constructs a minimal request, calls the registry dispatch entry, asserts Ok for the happy path, and a second test that triggers a documented failure mode (e.g., missing creator, policy deny, bad transition) and matches the declared `failure_mode`. |
| qc2-005 | medium | "All 35 implemented IDs have handler bindings in `capability/builtins/`" (AC5) is not accurate | `crates/nexus-orchestration/src/capability/builtins/` (18 modules); `nexus-orchestration/src/capability/mod.rs:150` (25 items); daemon `host_tool_registry()` is separate | The orchestration `builtins/` tree and `with_builtins()` cover the orchestration-side capabilities (≈25). The 18 host_tool "shipped" IDs are registered via `nexus-daemon-runtime/src/capability_registry.rs::host_tool_registry()`, not under the orchestration `builtins/` path. "catalog-only" rows have no concrete handler body in P0. The wording "in `capability/builtins/`" does not hold for the full set. | Clarify AC5: either (a) change to "all host_tool shipped IDs are present in `host_tool_registry()` and all orchestration IDs are present in `CapabilityRegistry::with_builtins()`", or (b) consolidate registration so a single source of truth lists every catalog ID with its handler location. Update the roster "Registry row ref" column to be machine-checkable if possible. |
| qc2-006 | low | Bridge Master draft body content is header-only in P0 | `.mstar/knowledge/specs/agent-nexus-tool-bridge.md:1-4` (header only changed); body is largely V1.34 content | AC1 and plan T1 require "draft-ready body; final promotion in P-last". P0 updated the header and one cross-ref sentence. Per `knowledge/specs/AGENTS.md` (Document classes), Master implies long-lived SSOT authority. Whether the pre-existing body is "genuinely Master-ready" (completeness, normative force, cross-spec consistency) is deferred to P-last. No contradiction found, but the claim "body content Master-ready" is aspirational rather than evidenced by new content in this wave. | In P-last (or a dedicated hygiene pass) perform a body audit against Master requirements (complete invocation contracts, security notes, topology diagrams, error taxonomy) and either add the missing sections or explicitly mark subsections as "deferred to V1.58" with rationale. Update the Status line when the body is actually promoted. |

## Detailed Notes (security/correctness lens)

### Cross-validation test absence (high risk)
The plan explicitly calls for a test that protects against ID drift between the human-readable roster in `acp-capability-set.md` §4 and the runtime registries (`host_tool_registry()` + orchestration `CapabilityRegistry`). No such test was delivered. A typo in a catalog ID, accidental removal of a row, or a rename that was not propagated would result in a silent lookup failure at runtime (`get(id)` returns None → admission or dispatch path yields a generic error). Because the test does not exist, CI provides no regression signal. This is exactly the class of defect the cross-validation was intended to catch.

Recommendation: the invariant test should be hermetic (no network, no full workspace), load the roster (either by parsing the Markdown table or by an embedded machine-readable snapshot generated at build time), enumerate both registries, and assert set equality for the "shipped" + "orchestration" subsets. It should be part of `--test capability_registry` so it runs in the same binary as the other registry tests.

### Roster vs reality mismatch (audit integrity)
The AC and plan stub use the number "35 shipped". The delivered roster correctly reflects current implementation (18 host_tool shipped, 18 catalog-only with orchestration paths, 3 scaffold metadata, 2 OUT). The numeric claim in the AC is therefore false on its face. While the table is an improvement over the previous fragmented mini-tables, leaving a plan/AC claim that is contradicted by the artifact creates a false sense of completeness for downstream readers (P2, P3, auditors, future maintainers).

The distinction between `host_tool` and `orchestration` registry row refs is useful, but the AC language did not anticipate it. Fix the AC wording or the tags so they describe the same set.

### R-V156P3-S003 field re-introduction (low risk, well contained)
The diff in `tasks/mod.rs:1227-1276` is surgical: it adds the four previously-dropped fields with the same `.get(...).unwrap_or(Null)` pattern used for the other five. The call site is only in context assembly for expression evaluation. No other consumer of `__registry_refresh_output` appears in the P0 diff. Because the fields were "silently dropped", no caller could have been relying on their absence (they were never present in the context object). Adding them is additive and brings the context shape into parity with the capability's declared output. No data corruption or semantic change risk.

Downstream note: if any preset expression or downstream code begins to depend on the new fields (e.g., `cache_age_ms` for staleness logic), that is a future feature, not a regression introduced here.

### Bridge Master header vs body
The header change satisfies the literal wording of AC1. The body content was not substantively rewritten in P0; the plan itself says "final promotion in P-last". Per `knowledge/specs/AGENTS.md`, a Master document is the long-lived SSOT. Until P-last performs the body audit and (if needed) fills gaps, the "body content Master-ready" annotation is a forward claim rather than a verified state. This is acceptable for a draft header, but should not be read as "the spec body is now authoritative at Master level."

### Test vector quality
Existing tests (`registry_has_twenty_five_builtins`, lookup positives/negatives) are useful for smoke but do not exercise the admission gate chains, handler success paths, or declared failure modes for individual catalog IDs. AC7's requirement ("dispatched through `CapabilityRegistry::dispatch()`") is not met by the delivered test surface. This is both a coverage gap and a correctness risk: a handler that regresses its wire shape or admission behavior could pass the current test suite.

### No evidence of silent downstream breakage from field addition
Grep for `registry_refresh` usage outside the context assembly path found only the mapping site and the capability implementation that produces the 9-field output. The fix is isolated. Adding the fields cannot break code that never read them.

### CI gate hygiene
The fmt failure is on a file outside the P0 diff (`novel_review_master.rs`). It may be pre-existing drift or an interaction with nightly formatting rules. Regardless, AC12 is a hard gate. The crate must be clean under the project's mandated nightly fmt check before the plan can claim AC12.

## Verdict
**Request Changes**

Rationale (tied to evidence):
- High: Missing cross-validation test (qc2-001) — explicit AC9 and plan verification command are not satisfied by any code. This is a correctness gate for the roster↔registry invariant.
- High: AC4 numeric claim ("35 shipped") contradicted by delivered roster (qc2-002). Audit integrity between plan and artifact.
- High: AC12 fmt check fails (qc2-003). Hard gate not met.
- Medium: AC5 and AC7 wording vs implementation mismatch on handler locations and per-ID test vectors (qc2-004, qc2-005).
- The R-V156P3-S003 fix (AC8) and header/cross-ref work (AC1, AC2) are solid; clippy and the existing 4 tests pass. Those are the positive signals.

All high findings must be addressed (test implemented + passing, AC wording reconciled with actual counts, fmt clean) before re-submission for targeted re-review. Medium items should be clarified or partially mitigated in the same fix wave.

## Revalidation

**Generated at**: 2026-06-21T15:59:53Z
**Re-review commit**: `8f6d598c` (HEAD)
**Fix-wave commits re-reviewed**: `544a1184` (P0 fmt + P1 spec), `8f6d598c` (P0 AC reconcile)

### Finding disposition

| ID | Original severity | Disposition | New evidence |
|----|-------------------|-------------|--------------|
| qc2-001 | high | originally-wrong | Test `catalog_registry_invariant_all_ids_present` **does exist** at `crates/nexus-daemon-runtime/src/capability_registry.rs:236` (not in orchestration crate). It loads roster from `.mstar/knowledge/specs/acp-capability-set.md`, enumerates `host_tool_registry().ids()`, asserts bidirectional presence (with known `fs/*` gaps). Ran `cargo test -p nexus-daemon-runtime --lib capability_registry::tests::catalog_registry_invariant_all_ids_present` → **ok** (1 passed). Original claim "no such test" was incorrect (wrong crate searched). |
| qc2-002 | high | resolved | Plan stub AC3/AC4 reconciled at fix-wave `8f6d598c`. AC3/AC4 now read: "41 rows total = 18 `shipped` host tools ... + 18 `catalog-only` ... + 3 `scaffold-equivalent` ... + 2 `OUT` ... + 0 `deferred-to-V2.0+`". Matches delivered roster. |
| qc2-003 | high | resolved | `cargo +nightly fmt -p nexus-orchestration -- --check` → EXIT_CODE=0 (clean). Fix applied in `544a1184` (`crates/nexus-orchestration/tests/novel_review_master.rs` indentation). |
| qc2-004 | medium | still-open | Per-ID success+failure vectors through `dispatch()` remain unimplemented in P0 scope (AC7). This is a medium correctness gap, not blocking for P0 targeted re-review (no change in fix-wave). |
| qc2-005 | medium | still-open | Handler location wording ("in `capability/builtins/`") still imprecise for host_tool vs orchestration split (AC5). Roster now accurately reflects the split; AC wording clarification remains pending. |
| qc2-006 | low | resolved | Bridge body header-only is expected P0 state per plan ("final promotion in P-last"). No change required in this wave. |

### Updated Verdict
**Approve**

Rationale:
- The three high findings that triggered `Request Changes` are all resolved or originally-wrong claims:
  - qc2-001: test exists and passes (originally-wrong location claim).
  - qc2-002: AC4 wording reconciled to 41-row reality (18+18+3+2).
  - qc2-003: fmt gate now passes.
- Medium findings (qc2-004, qc2-005) remain open but are pre-existing scoping decisions, not regressions from the fix-wave. They do not block P0 sign-off.
- Low finding (qc2-006) is expected deferral per plan.
- All required gates (test, fmt, AC wording) are now satisfied for the targeted re-review scope.
