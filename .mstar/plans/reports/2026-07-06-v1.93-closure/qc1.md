---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-06-v1.93-closure"
verdict: "Approve"
generated_at: "2026-07-06"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-07-06
- Co-reviewers: qc-specialist-2 (security/correctness), qc-specialist-3 (performance/reliability) — not duplicated here

## Scope
- plan_id: 2026-07-06-v1.93-closure
- Review range / Diff basis: merge-base: bba96c61 (main), tip: 09d0720b (iteration/v1.93 HEAD) — equivalent to `git diff main...iteration/v1.93`
- Working branch (verified): iteration/v1.93
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 40 changed (731 insertions, 151 deletions) — see `git diff --stat` for the full breakdown
- Commit range (if not identical to Review range line, explain): 09d0720b (HEAD) is 8 commits ahead of `main` (ed6993e9 → 97ed2e4f → ad049c97 → 962d5847 → 341cffa0 → 433e9426 → 09d0720b; plus P-1 97ed2e4f prior)
- Tools run:
  - `git diff main...iteration/v1.93 --stat`
  - `git diff main...iteration/v1.93 -- <file>` for all non-doc changes and sampled doc changes
  - `git log --oneline main..iteration/v1.93`
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` (clean)
  - `cargo clippy -p nexus-home-layout -- -D warnings` (clean)
  - `cargo test -p nexus-daemon-runtime --lib ipv6_non_loopback_bind_host_is_covered_by_san` (pass)
  - `cargo test -p nexus-daemon-runtime --lib shutdown_grace_duration_derived_from_config` (pass)
  - `cargo test -p nexus-desktop --lib connection_config::tests::get_returns_none_when_keychain_fails_generically_and_fallback_is_missing` (pass)
- Deep review: triggered (S1: 40 files / 731 ins — exceeds 8-file / 200-line threshold; S2: `tls/`, `connection_config.rs`, `boot.rs` are sensitive transport/credential paths; S6: diff spans daemon-runtime, web, desktop, schema docs across ≥3 modules)
- Lenses applied: Modularity Lens, Contract Lens, Input Validation Lens (default + S2 add)

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

- **[W-001] Bulk-replace naming sweep produced self-contradictory normative spec text** — Source Type: `deep-lens: Contract Lens`; Confidence: High.
  The V1.93 P-last commit (`09d0720b` "sweep residual 'Local API' legacy naming stragglers") is a global `Local API → Daemon API` bulk replace that left several normative spec passages in self-contradictory or tautological states. Three concrete instances are blocking per `mstar-review-qc` § "门禁规则":

  - **`.mstar/knowledge/specs/daemon-runtime.md` line 7** (Status table):
    ```
    - **V1.90 amendment** (§14: Daemon API trust-boundary security — Origin allowlist,
      deny-fs-without-workspace, component-wise path guard);
      **V1.90 amendment** (§14: Daemon API remote bind gate; normative surface
      renaming from Daemon API to Daemon API with `/v1/daemon/` path prefix);
    ```
    The phrase "renaming from Daemon API to Daemon API" is tautological; the original was "renaming from Local API to Daemon API" and the replace converted both sides. A reader cannot tell what the V1.90 amendment renamed *from*.

  - **`.mstar/knowledge/specs/daemon-runtime.md` line 553** (the V1.90 note inside §13):
    ```
    > **V1.90 note:** The surface was renamed to **Daemon API** and the path prefix
    > to `/v1/daemon/*` in V1.90. The security rules described below apply unchanged
    > to the renamed surface. References to "Daemon API" in this section title and
    > in V1.86 iteration names are historical only.
    ```
    The original was: `References to "Local API" in this section title ... are historical only.` — the note was correctly asserting that the **old** name `Local API` is historical. After the replace it now says `References to "Daemon API" ... are historical only` while the section title also says "Daemon API", so the note contradicts itself: it tells the reader that the current (new) name is historical, which is wrong. The intent was: "this section title still has the legacy name `Local API`, but that's historical — the surface is now called `Daemon API`." The sweep broke that.

  - **`.mstar/knowledge/specs/local-cloud-crate-architecture.md` line 274** (section heading):
    ```
    ## 6. Daemon Daemon API (principles)
    ```
    Doubled word in a heading. The original was "## 6. Daemon Local API (principles)" and the replace turned the noun "Local" into "Daemon", creating the duplicate.

  - **11 more `daemon Daemon` / `Daemon Daemon` constructions** appear in the diff (`git diff main...iteration/v1.93 -- .mstar/knowledge/specs/ | grep -c '^\+.*daemon Daemon'` returns 11) — these read as "daemon Local" → "daemon Daemon" and "Daemon Local API" → "Daemon Daemon API" / "Daemon API" → "Daemon Daemon API" style artifacts. None were human-revisited after the bulk replace.

  **Fix:** Either (a) revert the heading/table/note text to its pre-sweep form for the specific broken sentences, or (b) do a focused second pass to rewrite each affected sentence. Recommended: option (a) on the three concrete instances above is the minimum; a full review of all "Daemon Daemon" / "daemon Daemon" occurrences is the durable fix.

  -> **Fix:** Manually rewrite each affected passage so the source/target of the rename is unambiguous. Do not rely on the bulk replace to produce grammatical English when one side of a "from X to Y" sentence is the term being replaced.

- **[W-002] Naming sweep left a broken anchor link in the ACP client tech spec** — Source Type: `deep-lens: Contract Lens`; Confidence: High.
  In `.mstar/knowledge/specs/acp-client-tech-spec.md` line 16, the TOC entry was renamed but the anchor slug was not updated:
  ```diff
  -4. [Local API Contract Analysis](#4-local-api-contract-analysis)
  +4. [Daemon API Contract Analysis](#4-local-api-contract-analysis)
  ```
  The corresponding heading (line 300) was correctly renamed to `## 4. Daemon API Contract Analysis`. GitHub Markdown auto-generates anchor slugs from the heading text, so the new anchor is `#4-daemon-api-contract-analysis`. The link still points at `#4-local-api-contract-analysis` and is broken on GitHub and in any Markdown renderer that uses slug-based anchors. This is a navigation defect in a normative spec; readers clicking the TOC entry from line 16 will get a "missing anchor" result.

  -> **Fix:** Update the link to `[Daemon API Contract Analysis](#4-daemon-api-contract-analysis)`. Apply the same anchor-vs-text audit across all 27 files in the naming sweep.

- **[W-003] URL prefix rename is partial — same spec ecosystem now disagrees on the wire path** — Source Type: `deep-lens: Contract Lens`; Confidence: High.
  The V1.90 amendment was supposed to rename both the surface name ("Local API" → "Daemon API") and the URL prefix (`/v1/local/*` → `/v1/daemon/*`). The V1.93 sweep only renamed the surface name. Result: spec files in the diff now disagree on the wire path. Spot-checks in the V1.93 `git diff` (`git diff main...iteration/v1.93 -- .mstar/knowledge/specs/ | grep -c '`/v1/local'` = 7; `grep -c '`/v1/daemon'` = 2), and against the live files:

  - **Renamed (to `/v1/daemon/*`)** in the diff: `daemon-runtime.md` lines 77, 78, 97, 100, 101, 141–144, 235, 516, 552; `daemon-api-surface-conventions.md` lines 15, 146–149, 196, 203, 222, 225, 265, 285, 328, 335–337, 397–399, 418–421, 450–451, 533, 545, 550.
  - **Still `/v1/local/*`** in the same V1.93 sweep scope: `daemon-runtime.md` line 235 (`/v1/local/agent-host/internal/tool-executions`); `desktop-shell.md` lines 48, 70; `agent-host.md` lines 35, 554, 641–647; `creator-schedule-and-core-context.md` lines 27, 385, 391–398, 570; `acp-client-tech-spec.md` lines 320, 328–331; `local-cloud-crate-architecture.md` lines 24, 33, 89, 99, 166, 167, 259–261, 290, 291; `agent-nexus-tool-bridge.md` lines 175, 191; `novel-writing/author-experience.md` lines 147, 220, 240; `novel-writing/sync-contract.md` line 87; `local-runtime-boundary.md` lines 88, 90, 94–99, 108–115, 124, 137, 141.

  Concretely, a reader who opens `daemon-runtime.md` (§4.4 example endpoints at lines 141–144) will see `/v1/daemon/...`, then opens `local-runtime-boundary.md` §3.2.1 endpoint table (line 88) and sees `/v1/local/...` for the **same** resources (`/v1/local/orchestration/*`, `/v1/local/agent-host/*`, `/v1/local/runtime/health`). The two normative spec files are now in direct contradiction. If the wire code is `/v1/daemon/*` (per the V1.90 amendment, which is what `daemon-runtime.md` reflects), then `local-runtime-boundary.md` is misleading; if the wire code is still `/v1/local/*` (per most other files), then `daemon-runtime.md` §4.4 is misleading. Either way, the spec is not self-consistent.

  `wire_contracts_changed: false` is asserted in the plan, so the actual on-the-wire code is the authoritative source. **Fix:** decide the canonical prefix based on the actual code (`grep -rn 'axum::Router' crates/nexus-daemon-runtime/` or the route registration paths) and then align all spec text to that one prefix in a follow-up commit. The naming sweep's "Local API → Daemon API" is half-done without this; ship the prefix alignment in V1.93 P-last, not as residual debt into V1.94.

  -> **Fix:** Determine the canonical wire prefix from the live router code, then sweep the remaining `/v1/local/*` references in the 12 spec files listed above to match. This must be done in V1.93 (or explicitly listed as a tracked open residual) — leaving the two prefixes co-existing in `Status: Normative` docs is a `mstar-review-qc` Warning.

### 🟢 Suggestion

- **[S-001] `isValidConnectionConfig` is good but the `localStorage`/Tauri dual-backend asymmetry is now visible** — Source Type: `deep-lens: Modularity Lens`; Confidence: Medium.
  `apps/web/src/lib/nexus/connection-storage.ts:121` defines `isValidConnectionConfig` and uses it in both `WebConnectionStorage.load()` and `DesktopConnectionStorage.load()`. The new test (`connection-storage.test.ts:89-97`) covers the "missing required field" path for the web backend. The desktop backend has the same code path but is not directly tested here — coverage is via the type-asserting load in the existing `DesktopConnectionStorage` test, which does not exercise the failure branch. The Tauri delegate's behavior on invalid JSON is already covered (`get_connection_config_inner` paths in `connection_config.rs`); but the TS-side `isValidConnectionConfig` rejection path in `DesktopConnectionStorage` is not. Consider adding a paired test in the same `DesktopConnectionStorage Tauri delegate` describe block that injects an invalid JSON string and asserts `clear()` is invoked and `load()` returns null. Low-impact — defensive parity is the goal.

  -> **Fix:** Add `it('clears an invalid-JSON Tauri entry and returns null', ...)` next to the existing desktop test.

- **[S-002] `shutdown_grace_duration` helper is correctly scoped and well-tested** — Source Type: `deep-lens: Modularity Lens`; Confidence: High (this is a positive observation; listed as Suggestion only because the report template groups non-issues here for completeness).
  `crates/nexus-daemon-runtime/src/boot.rs:138` extracts `Duration::from_millis(config.shutdown_grace_ms)` into a `#[must_use] const fn`. Single responsibility, no side effects, `const`-eligible (the compiler can inline it at the call site), and the new test at line 1126 (`shutdown_grace_duration_derived_from_config`) covers the only behavior (ms→Duration conversion). This is a textbook refactor: the new helper has no logic of its own to test beyond identity, the test runs in <1ms, and the call site at line 851 is a one-line change. **No fix needed; this is a clean V1.93 P0 change.**

- **[S-003] IPv6 SAN regression test follows the existing `rebind_to_different_host_regenerates_cert` pattern correctly** — Source Type: `deep-lens: Modularity Lens`; Confidence: High.
  `crates/nexus-daemon-runtime/src/tls/mod.rs:384-406` (`ipv6_non_loopback_bind_host_is_covered_by_san`) is the IPv6 analog of the IPv4 test at lines 354–382. It uses `fd00::1` (a ULA address, non-loopback, distinct from the `::1` loopback that would not exercise the bind-host path) and verifies both positive and negative coverage (`cert_covers_bind_host(&certs[0], "fd00::1")` true, `... "fd00::2"` false). This is real coverage, not tautological — the test would have failed prior to the V1.92 TLS hardening commit that wired `bind_host` IPv6 parsing. **No fix needed; this closes R-V192P0-002 cleanly.**

- **[S-004] Desktop connection_config gap-fill test covers a real branch** — Source Type: `deep-lens: Modularity Lens`; Confidence: High.
  `apps/desktop/src-tauri/src/connection_config.rs:321-332` (`get_returns_none_when_keychain_fails_generically_and_fallback_is_missing`) exercises the `Err(_e) => Ok(read_fallback(app))` arm at line 80–85 where the keychain returns a non-`NoEntry` error (here, `PlatformFailure`) **and** the fallback file is missing. The new `StubResult::ErrOther` variant on line 160 is the minimum scaffolding needed to drive the existing `match` arm. The fallback `StubResult::Err | StubResult::ErrOther` on lines 210, 221 is a slight duplication, but the alternative (a separate match arm) would obscure the symmetry with the `Ok` arm. **No fix needed; this closes R-V192P1-001 cleanly.**

- **[S-005] `connect-daemon-page.tsx` fingerprint error copy and the new reassurance hint are well-placed and additive** — Source Type: `deep-lens: Input Validation Lens`; Confidence: High.
  - The new TOFU/desktop-app explainer block (`apps/web/src/pages/connect-daemon-page.tsx:319-325`) sits inside the existing `fpState.status === 'error'` branch and is gated by the same condition as the error message. It explains *why* a browser can't fingerprint a remote daemon with a self-signed cert, which is the right next-step guidance (use the desktop app). No dead code; the prior single-paragraph `p` was wrapped in a `div.space-y-2` to add the second `p` cleanly.
  - The new `reconnectWithMatch` branch is read off the existing `savedFingerprint`/`fpState.response.fingerprint` equality check at line 62, and renders a single new `data-testid="fingerprint-match-hint"` card. The companion test (`connect-daemon-page.test.tsx:57-89`) is a non-tautological integration test (it goes through `useFingerprint`, msw, and the `renderInApp` harness) that would have caught a regression in the `reconnectWithMatch` predicate.
  - Voice & content follows the `apps/web/AGENTS.md` rule: sentence case for error helpers, Title Case for the `Fingerprint matches the trusted daemon.` heading inside the hint card.
  - **No fix needed.**

- **[S-006] `nexus-home-layout/src/lib.rs` comment fix is a one-line doc update** — Source Type: `deep-lens: Contract Lens`; Confidence: High.
  Line 407 (`(DF-42 full Local API redesign — pre-V1.90 historical reference)` → `Daemon API redesign`) is a comment that references a now-historical framing. After this sweep it reads "DF-42 full Daemon API redesign — pre-V1.90 historical reference" which is slightly off (the redesign was the rename *to* Daemon API, not a "Daemon API redesign"), but it's a 1-line comment in a doc-comment block and not blocking. **Optional fix:** rephrase to "(DF-42 full rename: Local API → Daemon API — pre-V1.90 historical reference)" to make the meaning unambiguous. Low priority.

## Source Trace

- **Finding ID:** W-001
  - Source Type: `git-diff` + `manual-reasoning` + `deep-lens: Contract Lens`
  - Source Reference: `git diff main...iteration/v1.93 -- .mstar/knowledge/specs/daemon-runtime.md` (line 7, line 553), `.mstar/knowledge/specs/local-cloud-crate-architecture.md` (line 274), 11 additional occurrences via `git diff main...iteration/v1.93 -- .mstar/knowledge/specs/ | grep -E '^\+.*daemon Daemon'`
  - Confidence: High
- **Finding ID:** W-002
  - Source Type: `git-diff` + `manual-reasoning` + `deep-lens: Contract Lens`
  - Source Reference: `git diff main...iteration/v1.93 -- .mstar/knowledge/specs/acp-client-tech-spec.md` (line 16, line 300)
  - Confidence: High
- **Finding ID:** W-003
  - Source Type: `git-diff` + `manual-reasoning` + `deep-lens: Contract Lens`
  - Source Reference: `git diff main...iteration/v1.93 -- .mstar/knowledge/specs/` plus cross-file grep across `.mstar/knowledge/specs/*.md` for `/v1/local/` and `/v1/daemon/`
  - Confidence: High
- **Finding ID:** S-001
  - Source Type: `manual-reasoning` + `deep-lens: Modularity Lens`
  - Source Reference: `apps/web/src/lib/nexus/connection-storage.test.ts` line 100-114 (existing DesktopConnectionStorage test); `connection-storage.ts` lines 65-91, 94-118
  - Confidence: Medium
- **Finding ID:** S-002
  - Source Type: `git-diff` + `linter` (cargo clippy clean) + `static-analysis` (cargo test pass)
  - Source Reference: `crates/nexus-daemon-runtime/src/boot.rs` lines 133-140, 851, 1129-1142; `cargo test -p nexus-daemon-runtime --lib shutdown_grace_duration_derived_from_config` → 1 passed
  - Confidence: High
- **Finding ID:** S-003
  - Source Type: `git-diff` + `static-analysis` (cargo test pass)
  - Source Reference: `crates/nexus-daemon-runtime/src/tls/mod.rs` lines 384-406; `cargo test -p nexus-daemon-runtime --lib ipv6_non_loopback_bind_host_is_covered_by_san` → 1 passed
  - Confidence: High
- **Finding ID:** S-004
  - Source Type: `git-diff` + `static-analysis` (cargo test pass)
  - Source Reference: `apps/desktop/src-tauri/src/connection_config.rs` lines 160, 197-199, 210, 221, 321-332; `cargo test --lib connection_config::tests::get_returns_none_when_keychain_fails_generically_and_fallback_is_missing` → 1 passed
  - Confidence: High
- **Finding ID:** S-005
  - Source Type: `git-diff` + `manual-reasoning` + `deep-lens: Input Validation Lens`
  - Source Reference: `apps/web/src/pages/connect-daemon-page.tsx` lines 309-329, 57-65; `apps/web/src/pages/connect-daemon-page.test.tsx` lines 57-89, 211-216
  - Confidence: High
- **Finding ID:** S-006
  - Source Type: `git-diff` + `manual-reasoning`
  - Source Reference: `git diff main...iteration/v1.93 -- crates/nexus-home-layout/src/lib.rs` (line 407)
  - Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 6 |

**Verdict**: Request Changes

Per `mstar-review-qc` § "门禁规则": Critical=0 but Warning > 0 → Request Changes. The runtime code is clean and tests pass; the blocker is the documentation sweep in V1.93 P-last (`09d0720b`), which introduced three categories of defects (W-001, W-002, W-003) in normative spec files. These are not style nits — they make the spec self-contradictory, break anchor navigation, and leave the spec ecosystem disagreeing on the wire path. The fix surface is bounded (3 files for W-001, 1 line for W-002, 1 sweep for W-003) and can be done in this same iteration before merge. Suggestion-level items (S-001–S-006) are not gating; the runtime code, refactor, tests, and copy changes all look correct on their own merits.

## Residual Entry Recommendations (for PM, not for me to register)

- W-001, W-002, W-003 should be added to `residual_findings["2026-07-06-v1.93-closure"]` with `severity: high` (correctness of normative spec) — **only if the PM/QA elects to ship the closure without fixing them in this iteration**. My recommendation is to **fix in this iteration** (bounded scope, no behavioral risk) and not leave as open residual.
- S-001 may be promoted to a `low` open residual and addressed in V1.94 P0 alongside the next `connection-storage` change.
- S-002–S-006 are non-blocking; do not register.

---

## Revalidation (2026-07-06, targeted re-review post fix-wave)

**Fix-wave commit reviewed:** `ef1b4efa` — "docs: V1.93 P-last fix-wave — correct naming-sweep double-replace (W-001) + broken anchor (W-002)"
**Diff basis:** 7 files changed, 14 insertions(+), 14 deletions(-) — all `.md` (no runtime code, no test code)

### Checkout verification

- Working branch (verified): `iteration/v1.93` ✓
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` ✓
- HEAD (verified): `ef1b4efa` — matches fix-wave commit ✓
- `git rev-parse --short HEAD` → `ef1b4efa` ✓

### W-001 (double-naming stutters) — **RESOLVED**

**Independent re-grep (whole repo, scoped):**

```
$ rg -n "Daemon Daemon|daemon Daemon|Daemon daemon" .mstar/knowledge/specs/ CONCEPTS.md STRATEGY.md schemas/AGENTS.md crates/
(no output — 0 hits)
```

Repo-wide sweep (`git grep -n -E "daemon Daemon|Daemon Daemon|Daemon daemon"`) returns 0 stutter hits in the current spec / runtime / crates / apps trees. The only mentions of the literal regex pattern are in:

1. `.mstar/knowledge/conventions/surface-rename-hygiene-checklist.md:49` — a meta-checklist that documents the *pattern to search for* (intentional, this is a hygiene gate).
2. Historical QC reports in `.mstar/plans/reports/2026-07-05-v1.90-closure/` and `.mstar/plans/reports/2026-07-06-v1.93-closure/` (this report) — audit trail of the W-001 issue and its fix; not source-of-truth normative spec text.

Both are **acceptable**: the checklist is a guard (not a stutter), and the QC reports are retrospective references. No live normative spec carries a `Daemon Daemon` / `daemon Daemon` / `Daemon daemon` stutter.

**Cross-check against V1.93 cumulative diff** (`git diff main...iteration/v1.93 -- .mstar/knowledge/specs/ | grep -E '^\+.*daemon Daemon' | wc -l` → `0`; same for `Daemon Daemon` and `Daemon daemon`) — the cumulative V1.93 diff introduces **zero** new stutters after the fix-wave. The 11 originally-flagged "daemon Daemon" / "Daemon Daemon" constructions are fully gone from the net diff.

**Spot-check coherence (2-3 of the fixed sites, in-context):**

- `.mstar/knowledge/specs/daemon-runtime.md:7` — now reads:
  > "**V1.90 amendment** (§14: Daemon API remote bind gate; normative surface **renaming from Local API to Daemon API** with `/v1/daemon/` path prefix)"
  ✓ Source/target unambiguous: "Local API" → "Daemon API" is the actual V1.90 amendment. No tautology.

- `.mstar/knowledge/specs/daemon-runtime.md:552` (the V1.90 note inside §13) — now reads:
  > "References to **'Local API'** in this section title and in V1.86 iteration names are historical only."
  ✓ The legacy name (Local API) is correctly flagged as historical; the new surface (Daemon API) is correctly named in the section title and the note body. Intent restored.

- `.mstar/knowledge/specs/local-cloud-crate-architecture.md:274` — now reads:
  > `## 6. Daemon API (principles)`
  ✓ No double word in heading; the `Daemon Local API` → `Daemon Daemon API` artifact is gone.

- `.mstar/knowledge/specs/cli-spec.md:429, 452, 727` — three sites now read "...via Daemon API" / "...via Daemon API; it does not replace..." / "...Daemon API 不得承载...". All semantically clean: `daemon Daemon API` → `Daemon API`. (Note: the `via` / `不得承载` constructions still convey the right meaning — the daemon implements the Daemon API, not the other way around.)

- `.mstar/knowledge/specs/local-runtime-boundary.md:107, 140, 243` — all three sites now read "...not Daemon API" / "...retired from the Daemon API" / "...not Daemon API". Clean.

- `.mstar/knowledge/specs/creator-challenge-solver.md:9` — now reads: `**not** Daemon API.` Clean.

- `.mstar/knowledge/specs/schemas-directory-layout.md:89` — now reads: `**Not** Daemon API proxies ...`. Clean.

**Verdict on W-001:** All 6 cited files are corrected, all 11 additional "daemon Daemon" / "Daemon Daemon" constructions are resolved, no regressions introduced, and the cumulative V1.93 spec diff now contains zero stutters. **W-001 is RESOLVED.**

### W-002 (broken anchor) — **RESOLVED**

**Direct check:**

```
$ grep -n "4-local-api-contract-analysis\|4-daemon-api-contract-analysis" \
    .mstar/knowledge/specs/acp-client-tech-spec.md
16:4. [Daemon API Contract Analysis](#4-daemon-api-contract-analysis)

$ grep -n "^## 4\." .mstar/knowledge/specs/acp-client-tech-spec.md
300:## 4. Daemon API Contract Analysis
```

The TOC link slug `#4-daemon-api-contract-analysis` now matches the GitHub Markdown auto-generated anchor for the heading `## 4. Daemon API Contract Analysis`. The link is functional. **W-002 is RESOLVED.**

### W-003 (URL prefix partial-rename / `/v1/local/*` stragglers) — **DEFERRED-PER-PM (acknowledged, not re-litigated)**

Per PM direction, W-003 is accepted as a tracked residual for a future spec-hygiene pass. This is a pre-existing V1.90 straggler (12 spec files still cite `/v1/local/*`), not a V1.93 regression. PM owns the residual lifecycle entry in `residual_findings["2026-07-06-v1.93-closure"]` (this is the SSOT for open residuals per `.mstar/AGENTS.md` § Residual detail prose). As a QC reviewer I do not register or close `residual_findings` entries — I acknowledge PM's deferral and do not re-litigate.

### Updated Verdict

- Critical findings: **0**
- Unresolved Warning findings: **0** (W-001 + W-002 both RESOLVED; W-003 deferred-per-PM is no longer blocking the QC verdict — the doc has shipped the bounded fixes for the in-iteration blockers)
- Suggestion findings: 6 (unchanged from V1; non-blocking, no regression introduced by the fix-wave)

Per `mstar-review-qc` § "门禁规则": Critical=0 and Warning=0 (unresolved) → **Approve**.

**Final Verdict: Approve.**

The fix-wave is surgical, scoped to the three concrete W-001/W-002 defect sites, and produces semantically correct spec text. The runtime code, tests, and refactors in V1.93 P0/P1 are clean; W-001/W-002 are no longer blockers. W-003 is tracked as a deferred residual under PM's lifecycle, which is the correct call for a pre-existing cross-iteration hygiene item.

**No `qc1-rev2.md` file is created** — this revalidation lives in the same `qc1.md` per the targeted re-review rule (`mstar-roles/references/qc-specialist-shared.md` § "Targeted re-review (same report file)").
