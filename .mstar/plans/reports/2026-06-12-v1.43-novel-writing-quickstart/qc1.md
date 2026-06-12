---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-12-v1.43-novel-writing-quickstart
verdict: Request Changes
generated_at: 2026-06-12T10:35:22+08:00
---

# Code Review Report — P0 (BL-10 novel-writing quickstart)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-12T10:35:22+08:00

## Scope
- plan_id: 2026-06-12-v1.43-novel-writing-quickstart
- Review range / Diff basis: merge-base: ae7c9415 + tip: 23dac267
- Working branch (verified): feature/v1.43-novel-writing-quickstart
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p0
- Files reviewed: 2 (docs/ARCHITECTURE.md, docs/novel-writing-quickstart.md)
- Commit range: ae7c9415..23dac267
- Tools run: git rev-parse, git branch, git log, git status, git diff, rg (links, emojis, cross-refs), test -f (link resolvability), spec cross-reference (cli-spec.md, cli-command-ia.md)

## Findings
### 🔴 Critical
(None)

### 🟡 Warning
- **W-001**: `creator works pool` (line 220) is a bare subcommand with no sub-action. The CLI spec (cli-spec.md §6.2H line 452) defines `nexus42 creator works pool list` as the list subcommand — bare `pool` is not a valid command. Copy-paste will fail or show help text. → Fix: change to `nexus42 creator works pool list`.
- **W-002**: Spec overlay divergence — `novel-author-experience.md` §2 row for Part I §4 says `creator run status`, but the doc correctly uses `creator works status` (per V1.41 migration, cli-spec.md line 370). The doc is correct; the spec overlay is stale. → Fix: amend spec overlay at P-last hygiene (per writing-specialist Completion Report §7). No doc change needed; this is a spec-maintenance residual.

### 🟢 Suggestion
- **S-001**: ACP prerequisite (line 10) states "ACP setup is outside this quickstart's scope" but provides no pointer to where ACP setup documentation lives. A brief "See [ACP setup guide](...)" or equivalent would reduce user friction at the only blocking prerequisite.
- **S-002**: Further Reading table (lines 268–276) links to `.mstar/knowledge/specs/` paths using `../` relative links. While these resolve correctly from `docs/`, they expose harness-internal paths to end users. If specs are reorganized, these links break silently. Consider whether end-user docs should link to harness paths or to a stable user-facing reference page.
- **S-003**: The doc uses `creator run continue --note` for inspiration injection (lines 126, 254). The CLI spec also defines a dedicated inspiration surface (`creator works pool inspiration add --title`, cli-spec.md line 455). The doc doesn't mention this dedicated path. Not a bug — `--note` is valid — but the dedicated surface could be noted for completeness at P2.

## Source Trace
- Finding ID: W-001
- Source Type: spec-cross-reference
- Source Reference: docs/novel-writing-quickstart.md:220 vs .mstar/knowledge/specs/cli-spec.md:452
- Confidence: High

- Finding ID: W-002
- Source Type: spec-cross-reference
- Source Reference: .mstar/knowledge/specs/novel-author-experience.md §2 row 4 vs docs/novel-writing-quickstart.md §4
- Confidence: High

- Finding ID: S-001
- Source Type: doc-rule
- Source Reference: docs/novel-writing-quickstart.md:10
- Confidence: Medium

- Finding ID: S-002
- Source Type: manual-reasoning
- Source Reference: docs/novel-writing-quickstart.md:268–276
- Confidence: Medium

- Finding ID: S-003
- Source Type: spec-cross-reference
- Source Reference: docs/novel-writing-quickstart.md:126,254 vs cli-spec.md:455
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

## Spec §2 Alignment Audit

| Spec §2 Row | Doc Heading | Match |
|---|---|---|
| Part I §1 — Prerequisites & bootstrap | `### §1 Prerequisites & Bootstrap` (L20) | ✅ PASS |
| Part I §2 — World + novel-project-init | `### §2 World & Project Init` (L43) | ✅ PASS |
| Part I §3 — First chapter: outline → draft → finalize | `### §3 First Chapter` (L79) | ✅ PASS |
| Part I §4 — Serial: auto-chain, status, chapter N | `### §4 Serial Writing with Auto-Chain` (L108) | ⚠️ PASS (spec says `creator run status`; doc correctly uses `creator works status` — see W-002) |
| Part I §5 — Quality loop: findings, review, 96h banner | `### §5 Quality Loop` (L143) | ✅ PASS |
| Part I §6 — Completion: when writing stops | `### §6 Completion` (L170) | ✅ PASS |
| Part II A — Multi-work desk | `### A) Multi-Work Desk` (L208) | ✅ PASS |
| Part II B — Multi-volume | `### B) Multi-Volume` (L225) | ✅ PASS |
| Part II C — Inspiration pool | `### C) Inspiration Pool` (L249) | ✅ PASS |

**Result**: 8/9 exact match, 1/9 with spec-overlay divergence (W-002 — doc is correct, spec overlay needs P-last amendment).

## Architecture Coherence Assessment

### Cross-link (docs/ARCHITECTURE.md)
- Placed in monorepo-layout table, "End-user docs" row (line 17) — least intrusive location ✅
- Link target `novel-writing-quickstart.md` resolves from `docs/` ✅
- No duplicate IA entries ✅

### Document structure
- Heading hierarchy consistent: `#` title → `##` Prerequisites / Part I / Part II / Further Reading → `###` sections ✅
- Part I labeled "Ongoing Serial", Part II labeled "Optional / Advanced" — clear dependency chain ✅
- Section ordering follows spec §2 table exactly ✅
- 19 commands verified against cli-spec.md / cli-command-ia.md — all exist except W-001 (bare `creator works pool`) ✅

### P1/P2/P-last follow-up setup
- §3 (First Chapter) references `creator works status` which P2 will enhance with findings visibility ✅
- §5 (Quality Loop) documents the 96h banner and review pass — sets up P2 visibility work ✅
- §6 (Completion) documents completion-lock — sets up P-last hygiene ✅
- Section anchors use `### §N` format — P1 CLI copy can cite e.g. "See docs/novel-writing-quickstart.md §3" ✅

### Language & style consistency
- Code blocks use ````bash` and ````text` — consistent with CONTRIBUTING.md, CODEGEN.md ✅
- No emojis (verified via Unicode range check) ✅
- Pre-release note (line 12) consistent with root AGENTS.md versioning policy ✅
- No invented commands — all verified against CLI specs ✅

### Out-of-scope callouts
- ACP: "ACP setup is outside this quickstart's scope" (line 10) — called out but no pointer (see S-001) ⚠️
- Platform account, cloud sync, harness knowledge: explicitly excluded in opening paragraph (line 3) ✅
- No new runtime features implied in Part II ✅

### Further Reading table
- Links to stable normative sources (ARCHITECTURE.md, cli-spec.md, cli-command-ia.md, creator-centric-entry-model.md, novel-workflow-profile.md, CONTRIBUTING.md) ✅
- All 6 links resolve from `docs/` ✅
- Uses `../` relative paths to harness specs — see S-002 ⚠️

## Static Checks

| Check | Result |
|-------|--------|
| Markdown links well-formed | PASS — 6 links, all resolve |
| Cross-link in ARCHITECTURE.md | PASS — line 17, `novel-writing-quickstart.md` |
| No emojis | PASS — Unicode range check clean |
| All Further Reading targets exist | PASS — 6/6 files present |
| No `creator run status` (legacy) in doc | PASS — correctly uses `creator works status` |
