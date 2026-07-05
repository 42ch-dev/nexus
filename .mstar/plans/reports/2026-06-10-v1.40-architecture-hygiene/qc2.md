---
report_kind: qc
plan_id: 2026-06-10-v1.40-architecture-hygiene
reviewer: qc-specialist-2
reviewer_index: 2
focus: security and correctness risk
verdict: Approve
generated_at: 2026-06-10T14:22:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk
- Report Timestamp: 2026-06-10T14:22:00Z

## Scope
- plan_id: 2026-06-10-v1.40-architecture-hygiene
- Review range / Diff basis: iteration/v1.40..feature/v1.40-architecture-hygiene (equivalently commits 3c90c18f..dc7f81e7)
- Working branch (verified): feature/v1.40-architecture-hygiene
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: 3c90c18f..dc7f81e7 (4 commits: T1 add module + relocate, T2 refactor reader, T3/T4 remove old dir + grep hygiene, T5 doc path corrections)
- Tools run: git rev-parse --show-toplevel, git branch --show-current, git diff --name-only, git diff --stat, git show (content + shasum), git log, read (embedded_rules.rs, stage_gates.rs, lib.rs, AGENTS.md, plan, world-kb doc), grep (for read_rules_layers / stale paths), cargo test -p nexus-orchestration -- read_rules_layers, manual SHA256 + byte-size verification for byte-identity

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- (none)

### 🟢 Suggestion
- The `embedded_rules.rs` module docstring and the `read_rules_layers()` doc comment now correctly document the new `embedded-rules/` location and the `include_str!` contract. Consider adding a brief compile-time-visible comment or `const_assert!`-style note (if the project adopts one) that "Layer 1 is a compile-time constant with no runtime FS dependency" so future maintainers cannot accidentally re-introduce a runtime read for the shared craft rules. This is documentation hardening only; current implementation already satisfies the security invariant. -> optional follow-up in a later hygiene pass or when user-override (Layer 0) is wired.

## Source Trace
- Finding ID: (N/A — clean review)
- Source Type: manual-reasoning + git-diff + static-analysis + test-execution
- Source Reference: `git diff iteration/v1.40..feature/v1.40-architecture-hygiene`, SHA256 identity check on writing-craft.md, source read of `embedded_rules.rs:21` (`include_str!("../embedded-rules/writing-craft.md")`), `stage_gates.rs:191` (consumer), `read_rules_layers` tests (hermetic), grep for `embedded-presets/rules` (only historical docs remain)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

**Rationale (security + correctness)**:
- `writing-craft.md` content move is byte-identical (SHA256 `6a9e6b196b5e951a06c3187e7abb1ba1a13cbd38485d9c62537695eae59431cd` on both sides; 1578 bytes). The `git diff` shows a "new file" only because the old path delete and new path add were in separate commits within the range; content was not altered.
- `include_str!("../embedded-rules/writing-craft.md")` is a compile-time constant resolved by rustc relative to `embedded_rules.rs`. It is deterministic, cannot be influenced by runtime cwd, env vars, or untrusted input, and introduces zero path-traversal surface.
- `read_rules_layers()` public signature is unchanged. Layer 1 sourcing was moved from the old (incorrect) location to the dedicated `embedded_rules` module; Layer 2 (per-work `novel-rules.md`) construction and behavior are untouched. All existing callers (primarily `build_preset_input` for novel-writing preset input) and the 4 hermetic unit tests continue to observe identical observable behavior.
- No new runtime filesystem reads, no TOCTOU, no logging of embedded file paths that could leak layout, no new env-var or cwd dependencies for the shared rules layer.
- Stale string references to the old interim path (`embedded-presets/rules/writing-craft.md`) exist only in historical artifacts (archived plan JSON, the plan document describing the problem being fixed, and the DF-65 tracker note which was *corrected* by T5 to record the migration accurately). No code under `crates/` references the old path.
- DF-65 tracker update (T5) preserves historical context: it now explicitly states "V1.39 shipped Layer 1 at `embedded-presets/rules/...` (interim). V1.40 P0.5 migrated ... per spec §5.5.4" while also correcting the cross-reference to `world-kb-runtime-architecture.md`. Audit trail is improved, not lost.
- The change is a pure relocation + internal reader update with no behavioral regression for downstream preset consumers or the rules content injected into LLM prompts.

All items on the security + correctness checklist pass with no residual risk.
