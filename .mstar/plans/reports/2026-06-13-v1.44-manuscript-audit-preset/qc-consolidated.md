---
report_kind: qc-consolidated
plan_id: "2026-06-13-v1.44-manuscript-audit-preset"
verdict: "Request Changes"
generated_at: "2026-06-13"
reviewer_shas:
  qc-specialist: "16e8e703"
  qc-specialist-2: "a0858b17"
  qc-specialist-3: "3f963183"
review_range: "068135ed..9d471bdc"
---

# QC Consolidated Report — V1.44 P0 (`novel-manuscript-audit` preset + CLI entry)

## Verdict: **Request Changes**

All three reviewers raised **Request Changes**. **One Critical finding** (qc-specialist-3 F-001) blocks merge; **8 Warning** findings across reviewers require targeted re-review after fix wave.

## Reviewer verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
| --- | --- | --- | --- | --- | --- |
| qc-specialist | Architecture / maintainability | Request Changes | 0 | 3 | 4 |
| qc-specialist-2 | Security / correctness | Request Changes | 0 | 2 | 3 |
| qc-specialist-3 | Performance / reliability | Request Changes | 1 | 6 | 3 |
| **Total** | | **Request Changes** | **1** | **11** | **10** |

## Critical findings (merge-blocking)

### F-QC3-001: Extract mode unreachable in preset state machine
- **Reviewer**: qc-specialist-3
- **Scope**: `crates/nexus-orchestration/embedded-presets/novel-manuscript-audit/preset.yaml`
- **Issue**: `load_chapter.next: review_report` is hardcoded; `extract_sync` state exists but never entered; `--mode extract` runs the review path instead of `kb.extract_work`.
- **Impact**: DF-69 extract mode feature is completely non-functional; spec §3.2 contract broken.
- **Fix options**: (1) split into `novel-manuscript-audit-review` + `novel-manuscript-audit-extract` and have CLI dispatch; (2) add GoNogo conditional on `load_chapter` reading `preset.input.mode`; (3) make `load_chapter` use `exit_when: rule` and branch to `extract_sync` when `mode == "extract"`.
- **Severity mapping**: Report Critical → JSON `critical`.

## Warning findings (consolidated; severity per mapping table)

| Finding | Reviewer | Title | Severity (JSON) | Source |
| --- | --- | --- | --- | --- |
| F-QC1-W1 | qc-specialist | Preset YAML hardcoded `load_chapter.next`; dual-path reachability implicit | `medium` | qc1.md §Warning W1 |
| F-QC1-W2 | qc-specialist | `resolve_audit_body_path` accepts `volume` parameter but ignores it (`_volume`) | `medium` | qc1.md §Warning W2 |
| F-QC1-W3 | qc-specialist | Plan §6 verification references nonexistent `audit_chapter_cli` test file | `low` | qc1.md §Warning W3 |
| F-QC2-W1 | qc-specialist-2 | 422 world_required_for_extract enforced only in CLI; daemon schedule endpoint has no re-validation | `high` | qc2.md F-QC2-001 |
| F-QC2-W2 | qc-specialist-2 | `resolve_audit_body_path` copies raw `body_path` verbatim; no normalization / `..` rejection | `high` | qc2.md F-QC2-002 |
| F-QC3-W1 | qc-specialist-3 | CLI returns immediately for an operation advertised as on-demand (async schedule vs sync claim) | `medium` | qc3.md F-002 |
| F-QC3-W2 | qc-specialist-3 | `resolve_audit_body_path` ignores `volume` parameter (duplicate of F-QC1-W2) | `medium` | qc3.md F-003 |
| F-QC3-W3 | qc-specialist-3 | Missing `body_path` causes strict template-render failure | `medium` | qc3.md F-004 |
| F-QC3-W4 | qc-specialist-3 | 422 error returned as unstructured `Other(format!(...))` | `low` | qc3.md F-005 |
| F-QC3-W5 | qc-specialist-3 | Missing end-to-end CLI integration test (CLI → daemon schedule → preset execution) | `medium` | qc3.md F-006 |
| F-QC3-W6 | qc-specialist-3 | Runtime-lock invariant not visibly enforced in audit schedule path | `medium` | qc3.md F-007 |

## Residual findings (open / after fix wave)

| ID | Title | Severity | Decision | Owner | Target | Tracking |
| --- | --- | --- | --- | --- | --- | --- |
| R-V144P0-001 | DF-69 extract mode unreachable (preset state machine hardcodes next=review_report) | `critical` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-001 |
| R-V144P0-002 | 422 world_required_for_extract only enforced in CLI not daemon | `high` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc2.md F-QC2-001 |
| R-V144P0-003 | `resolve_audit_body_path` accepts but ignores `volume` parameter | `medium` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc1.md W2 / qc3.md F-003 |
| R-V144P0-004 | `resolve_audit_body_path` copies raw `body_path` verbatim; no normalization | `high` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc2.md F-QC2-002 |
| R-V144P0-005 | CLI returns async schedule; review/extract advertised as on-demand | `medium` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-002 |
| R-V144P0-006 | Plan §6 verification references nonexistent `audit_chapter_cli` test file | `low` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc1.md W3 |
| R-V144P0-007 | Missing `body_path` → strict template render failure | `medium` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-004 |
| R-V144P0-008 | 422 error unstructured; should be typed | `low` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-005 |
| R-V144P0-009 | Missing end-to-end CLI integration test | `medium` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-006 |
| R-V144P0-010 | Runtime-lock invariant not visibly enforced | `medium` | fix (this wave) | @fullstack-dev | V1.44 P0 ship | qc3.md F-007 |
| R-V144P0-S01..S10 | Suggestions (style, refactor, dedup) | `nit` / `low` | defer (post-V1.44 / P-last) | @fullstack-dev | V1.44 P-last | qc1/qc2/qc3 §Suggestion |

## Decision

- **Fix wave required**: dispatch `@fullstack-dev` (P0 owner) with explicit R-V144P0-001..010 fix list.
- After fix wave: targeted re-review by qc-specialist + qc-specialist-2 + qc-specialist-3 (all 3 raised blocking findings).
- Per harness gate rule: any unresolved Critical blocks Approve. **F-QC3-001 must close before re-review**.
- Re-review scope: same `068135ed..9d471bdc` range + post-fix commits up to next integration HEAD.

## Source trace

- qc1.md commit: `16e8e703 qc(v1.44 P0): qc-specialist architecture review`
- qc2.md commit: `a0858b17 qc(v1.44 P0): qc-specialist-2 security review`
- qc3.md commit: `3f963183 qc(v1.44 P0): qc-specialist-3 performance review`
- Plan: `.mstar/plans/2026-06-13-v1.44-manuscript-audit-preset.md`
- Compass: `.mstar/iterations/v1.44-novel-quality-and-serial-hardening-delivery-compass-v1.md`
