---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-26-v1.67-frontend-scope-gaps"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture & maintainability (focus per role parameters)
- Report Timestamp: 2026-06-27

## Scope
- plan_id: 2026-06-26-v1.67-frontend-scope-gaps
- Review range / Diff basis: P1 commits `e74321db`+`963fa1ed`+`aeaaf91a` (G1 work_profile selector + G2 TS-client 21→24 + tests) merged at HEAD; diff basis vs the pre-P1 base `26e477ee`. Equivalent `git log 26e477ee..HEAD -- apps/web`.
- Working branch (verified): iteration/v1.67
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6 source files (apps/web/src/lib/nexus/{types.ts,browser-client.ts,tauri-client.ts,browser-client.test.ts}, apps/web/src/pages/dialogs/{create-work-dialog.tsx,create-work-dialog.test.tsx}) + apps/web/AGENTS.md housekeeping
- Commit range: e74321db, 963fa1ed, aeaaf91a (P1 changes only; merged via fedf82e4)
- Tools run: git rev-parse/branch/log/diff/show, read, grep, glob, bash (pnpm contracts build, pnpm web typecheck, pnpm web test, bash tooling/check-wire-drift.sh)

## Findings

### 🔴 Critical
- **C1 — Wire-value mismatch: UI sends `game-bible` (hyphen) but the backend only recognizes `game_bible` (underscore).** The new `WORK_PROFILES` array in `apps/web/src/pages/dialogs/create-work-dialog.tsx:10-15` defines the four options as `novel`, `essay`, `game-bible`, `script`. The first three option values happen to match the backend's canonical identifiers (single words, no separator); `game-bible` does **not**. The backend enforces wire identity at the Rust layer via `is_game_bible_profile` (and the three sibling helpers) in `crates/nexus-local-db/src/works.rs:28-61`:
  ```rust
  pub fn is_game_bible_profile(profile: Option<&str>) -> bool {
      profile == Some("game_bible")
  }
  ```
  Every gate site uses the same underscore form: `crates/nexus-orchestration/src/preset_gates.rs:445` (`work_profile: Some("novel".to_string())`); `crates/nexus-local-db/src/work_chapters.rs:3146,3175,3207,3245,3272` (`UPDATE works SET work_profile = 'game_bible' …`); `crates/nexus-orchestration/src/capability/builtins/game_bible_scaffold.rs:375` (`UPDATE works SET work_profile = 'game_bible'`); `crates/nexus-daemon-runtime/src/api/handlers/works.rs:623` (`record.work_profile.as_deref() == Some("game_bible")`). The daemon handler `create_work` stores `work_profile: req.work_profile` verbatim (`works.rs:387`); the wire schema declares `work_profile: { "type": "string" }` (no enum) at `schemas/local-api/works/create-work-request.schema.json:19`, so the daemon accepts the hyphenated form silently and stores it in `works.work_profile`. **Effect**: any user who selects "Game Bible" from the new dropdown will produce a Work that:
  - is stored with `work_profile = 'game-bible'` (hyphen),
  - fails `is_game_bible_profile` (returns `false`),
  - fails `is_novel_profile` / `is_essay_profile` / `is_script_profile` (all return `false`),
  - is therefore treated as a legacy Work (`work_profile IS NULL` semantics, per `work_chapters.rs:796-797`), bypassing every game-bible-specific code path (chapter reconciliation block at `work_chapters.rs:800-810`, game-bible DTO branch at `works.rs:792`, capability-gate remediation at `preset_gates.rs:355`).
  `check-wire-drift.sh` passes (Rust struct ↔ schema are aligned), but **no tool catches the UI ↔ daemon-runtime identifier drift** — this is exactly the kind of bug the wire-drift check is silent about. The 3 other options coincidentally match because they are single words. **Fix**: change the option value to `'game_bible'` (underscore) at `create-work-dialog.tsx:13`. Update the existing test assertion (`create-work-dialog.test.tsx:78`, which currently asserts `work_profile: 'essay'` for the essay path) and add a new test asserting `work_profile: 'game_bible'` for the game-bible path. Optionally add a single integration-style assertion in the daemon test surface (or a comment in the UI mapping table) to keep the four option values in lockstep with the canonical helpers in `crates/nexus-local-db/src/works.rs`. **Severity = critical (JSON `severity: "critical"`)**: blocks the primary value-add of G1 (the entire reason this residual exists) for one of the four profiles; merge-blocking per `mstar-review-qc` §"门禁规则".

### 🟡 Warning
- **W1 — Default `'novel'` is a wire-value behavior change vs V1.66 (was `None` / unset).** The V1.66 UI did not send `work_profile` at all (the field existed on the wire schema since at least V1.65, `a51222ec`, but the dialog omitted it), so the daemon stored `work_profile = NULL`. The new dialog sends `work_profile: 'novel'` by default. The plan claims "An untouched form yields the same outcome as V1.66 (a novel-profile Work) — no behavior change" — this is mostly true in practice (`is_novel_profile` is true either way, gates pass), but `work_chapters.rs:796-797` explicitly states "Legacy Works (work_profile IS NULL) are treated as novel for backwards compatibility — they were created before the profile system existed", which signals the system **does** distinguish NULL from the explicit `'novel'`. Today's evidence is that the two states are functionally equivalent (any gate that cares about "is this a novel work" returns true either way; any gate that cares about "is this a legacy work" returns false either way), so practical impact is low — but a future code path that branches on `work_profile IS NULL` (e.g. one-time legacy migration, telemetry, "needs-onboarding" hints) will silently misclassify V1.67-created Works. **Fix options** (any one is acceptable): (a) leave the default at `'novel'` and add a code comment in `create-work-dialog.tsx` noting the V1.66 → V1.67 NULL → explicit shift and the rationale; (b) drop `work_profile` from the submit body when the value is the default (so the daemon keeps the old `None` shape) — but this fights the plan's intent to make the value visible; (c) do nothing and rely on a follow-up plan to make the legacy-NULL branch fully equivalent. **Severity = medium (JSON `severity: "medium"`)**: not merge-blocking but worth PM decision before merge. Suggested residual ID: `R-V167P1-G1-DEFAULT-NULL-DRIFT`.

### 🟢 Suggestion
- **S1 — Selector's `useState<string>` widens literal types and disables a compile-time guard against future value drift.** `create-work-dialog.tsx:41` declares `useState<string>` instead of `useState<typeof WORK_PROFILES[number]['value']>`. This is the same root cause that allowed C1 to slip past the type system: a typo in `WORK_PROFILES[3].value` (e.g. `'scipt'`) would still compile, and the test (which uses `user.selectOptions(..., 'essay')`) would not catch the regression because `essay` is unchanged. Narrowing the state type to a union of the four literal values (`useState<(typeof WORK_PROFILES)[number]['value']>`) turns the `WORK_PROFILES` constant into a single source of truth and makes the future bug structurally impossible. **Severity = nit (JSON `severity: "nit"`)**.
- **S2 — `WORK_PROFILES` is colocated with the dialog; if a second surface needs the same canonical list (e.g. an Edit-Work dialog in V1.68), it will fork.** Move the array to `apps/web/src/lib/work-profiles.ts` (or extend `apps/web/src/lib/nexus/adapters.ts`) with a `WORK_PROFILES_BY_ID: Record<WorkProfileId, { label: string }>` and a `WORK_PROFILE_IDS = ['novel', 'essay', 'game_bible', 'script'] as const` source of truth. This also gives a single place to keep the wire identifier in lockstep with the Rust helpers. **Severity = low (JSON `severity: "low"`)**: clean-up, not behavior.
- **S3 — `AGENTS.md` "Contracts status" row text could be more precise.** The V1.66 row text was: "Preset get/update/delete (no routes/contracts) — Presets page offers list/scaffold/validate/reload only". The V1.67 update is correct (`"Resolved (V1.67 G2) — getPreset/updatePreset/deletePreset promoted onto NexusClient (21 → 24); daemon routes + contracts already shipped. A form-based management UI is deferred to the V1.68 canvas."`) but the "Resolved" marker is only at row level; consider moving both "Resolved" rows to a separate "Closed Gaps" subsection below the active "Remaining gaps" table to keep the active list scannable. Pure docs/nit. **Severity = nit (JSON `severity: "nit"`)**.

## Source Trace
- Finding ID: QC1-C1
- Source Type: cross-crate manual review (UI wire value ↔ Rust identifier)
- Source Reference:
  - UI dropdown definition: `apps/web/src/pages/dialogs/create-work-dialog.tsx:10-15` (`WORK_PROFILES` const)
  - UI submit: `apps/web/src/pages/dialogs/create-work-dialog.tsx:68` (`work_profile: workProfile`)
  - Wire schema (loose, accepts any string): `schemas/local-api/works/create-work-request.schema.json:19` (`"work_profile": { "type": "string" }`)
  - Daemon accept-and-store: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:387` (`work_profile: req.work_profile`)
  - Canonical identifiers (Rust): `crates/nexus-local-db/src/works.rs:28-61` (`is_novel_profile == Some("novel")`, `is_game_bible_profile == Some("game_bible")`, `is_script_profile == Some("script")`, `is_essay_profile == Some("essay")`)
  - Canonical identifier use sites: `crates/nexus-orchestration/src/preset_gates.rs:445`; `crates/nexus-local-db/src/work_chapters.rs:3146,3175,3207,3245,3272,3307,3345,3383,3413,3452,3492`; `crates/nexus-orchestration/src/capability/builtins/{essay_scaffold,game_bible_scaffold,script_scaffold}.rs`; `crates/nexus-daemon-runtime/src/api/handlers/works.rs:576,623,678`
  - Legacy NULL semantics: `crates/nexus-local-db/src/work_chapters.rs:796-797` (comment: "Legacy Works (work_profile IS NULL) are treated as novel for backwards compatibility")
- Confidence: High

- Finding ID: QC1-W1
- Source Type: cross-crate diff + comment archaeology
- Source Reference:
  - UI default: `apps/web/src/pages/dialogs/create-work-dialog.tsx:41,50,68`
  - Plan claim: `.mstar/plans/2026-06-26-v1.67-frontend-scope-gaps.md:50` ("An untouched form yields the same outcome as V1.66 (a novel-profile Work) — no behavior change")
  - Wire schema history: `git log -S "work_profile" -- schemas/local-api/works/create-work-request.schema.json` → `a51222ec feat(local-api): V1.65 chapter content surface + preset full CRUD` (already on the schema in V1.65; the V1.66 dialog simply omitted the field)
  - Legacy-NULL semantics: `crates/nexus-local-db/src/work_chapters.rs:796-797`
- Confidence: Medium (practical impact is low today; future-impact argument is structural)

- Finding ID: QC1-S1
- Source Type: manual code review
- Source Reference: `apps/web/src/pages/dialogs/create-work-dialog.tsx:10-15, 41`
- Confidence: High

- Finding ID: QC1-S2
- Source Type: architectural pattern review
- Source Reference: `apps/web/src/pages/dialogs/create-work-dialog.tsx:10-15` (current colocation); comparison: `apps/web/src/lib/nexus/adapters.ts` (existing adapter SSOT pattern)
- Confidence: Medium

- Finding ID: QC1-S3
- Source Type: docs review
- Source Reference: `apps/web/AGENTS.md:36-42` (Contracts status table)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 (C1: `game-bible` ↔ `game_bible` wire-value mismatch — merge-blocking) |
| 🟡 Warning | 1 (W1: default `'novel'` vs V1.66 `None` NULL drift — PM decision) |
| 🟢 Suggestion | 3 (S1: state type narrowing; S2: extract `WORK_PROFILES` to SSOT; S3: AGENTS.md table layout nit) |

**Verdict**: Request Changes

Rationale: One **Critical** finding (C1) is unresolved. The G1 acceptance criterion AC1 says "Create-Work dialog exposes a `work_profile` selector (novel/essay/game-bible/script) wired to the existing field" — but for the `game-bible` option, the value sent to the daemon does not match what the backend recognizes. The `check-wire-drift.sh` gate is silent on UI ↔ daemon-runtime identifier drift; only manual cross-crate review (or a future codegen-level enum) would catch this. The fix is a one-character change in `create-work-dialog.tsx:13` plus a new test assertion. Per `mstar-review-qc` §"门禁规则" (any unresolved Critical → Request Changes), this review cannot Approve until C1 is fixed and re-verified.

## Revalidation (fix-wave-1)

**Trigger**: PM dispatched this seat only (qc2/qc3 were Approve on the initial wave). Per `qc-consolidated.md` §"Re-review after fix wave": **targeted re-review, qc1 only**, update `qc1.md` `## Revalidation`.
**Review range (re-confirmed)**: P1 fix-wave-1 (`feature/v1.67-p1-fixwave1`) commits `be783538`+`cd67f268` merged at HEAD via `65a72af5` (working branch `iteration/v1.67` @ `c053cdd9`+; `git log ebc7d977..HEAD -- apps/web` lists `be783538` as the sole code fix commit; `cd67f268` is the docs `Completion Report v2` mirror under `.mstar/plans/`, not under `apps/web`). Cwd / branch / `git rev-parse HEAD` all re-verified.

### What was re-checked
1. **C1 — canonical wire value** — read `be783538` diff for `apps/web/src/pages/dialogs/create-work-dialog.tsx` + `…test.tsx`; re-verified the backend canonical set at three sources (DB CHECK, Rust helpers, daemon handler sites); ran `pnpm --filter web test` (118/118 green); ran `pnpm --filter web typecheck` (clean).
2. **W1 — default-NULL preservation** — re-read the `workProfileTouched` flag wiring + the dialog-open reset; ran the same test/typecheck to confirm the new "untouched → omit" assertion passes and the new "explicit novel select → send" assertion also passes.
3. **Regression scan** — `git diff ebc7d977..HEAD --name-only` shows only the two expected files changed (`create-work-dialog.tsx`, `create-work-dialog.test.tsx`); the 6 other files from the P1 wave are untouched. No new drift.

### Per-finding disposition
- **C1 (Critical) — `game-bible` ↔ `game_bible` wire-value mismatch — Resolved.**
  - `apps/web/src/pages/dialogs/create-work-dialog.tsx:23` now declares `{ value: 'game_bible', label: 'Game Bible' }` (underscore). The other three values (`novel`, `essay`, `script`) were already canonical. Verified by direct read at the committed file (lines 20–25).
  - **All four UI values are now members of the backend canonical set.** Backend canonical set re-verified at three independent sources:
    - DB CHECK constraint: `crates/nexus-local-db/migrations/202606230001_work_profile_script.sql:27` — `CHECK (work_profile IS NULL OR work_profile IN ('novel', 'essay', 'game_bible', 'script'))`.
    - Rust helpers: `crates/nexus-local-db/src/works.rs:28-61` — `is_novel_profile == Some("novel")`, `is_game_bible_profile == Some("game_bible")`, `is_script_profile == Some("script")`, `is_essay_profile == Some("essay")`.
    - Daemon gate sites: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:576,623,678,733` — all use the same underscore form. Handler stores verbatim (no normalization) at `works.rs:387` and `works.rs:898`.
  - **New wire-contract membership-guard test exists and passes.** `apps/web/src/pages/dialogs/create-work-dialog.test.tsx:178-207` (second `describe` block `CreateWorkDialog work_profile wire contract (C1)`) hardcodes `BACKEND_ACCEPTED_WORK_PROFILES = new Set(['novel', 'essay', 'game_bible', 'script'])` sourced from the DB CHECK and asserts:
    - `WORK_PROFILES.length === 4` (line 192-193).
    - Every option value is a member of the backend-accepted set (line 194-198).
    - The Game Bible entry uses the underscore canonical form, not the hyphenated drift (line 202-205).
  - **New C1 round-trip test passes.** `create-work-dialog.test.tsx:151-175` exercises `user.selectOptions(..., 'game_bible')` and asserts the POST body contains `work_profile: 'game_bible'` and NOT `work_profile: 'game-bible'` — passes locally.
  - **Effect of the fix on the previously broken code paths**: any user selecting "Game Bible" will now produce a Work with `work_profile = 'game_bible'` (underscore), which `is_game_bible_profile` returns `true` for, routing the Work into the game-bible-specific branches (`work_chapters.rs:800-810`, `works.rs:792`, `preset_gates.rs:355`). The `check-wire-drift.sh` gate is still silent on UI ↔ daemon-runtime identifier drift; the new test closes that gap at the web-package level.
  - Confidence: **High** (3-source backend verification + 3 new passing tests + manual diff inspection).

- **W1 (Warning) — default `'novel'` vs V1.66 NULL drift — Resolved.**
  - `apps/web/src/pages/dialogs/create-work-dialog.tsx:56` introduces `const [workProfileTouched, setWorkProfileTouched] = useState(false)` — flag defaults to `false` (untouched).
  - The Select `onChange` (line 137-140) sets `setWorkProfileTouched(true)` only when the user actually selects an option.
  - The submit body (line 84) now uses `...(workProfileTouched ? { work_profile: workProfile } : {})` — the field is omitted when the form is untouched (daemon stores NULL, V1.66 semantics).
  - The reset-on-open effect (line 60-69) clears `workProfileTouched` back to `false` whenever `open` flips true, preventing state leakage between successive opens.
  - **New W1 positive test passes.** `create-work-dialog.test.tsx:127-149` — calls `selectOptions(..., 'novel')` on the default value (which fires `change` and sets the touched flag), then asserts `postedBody` matches `{ work_profile: 'novel' }`. Passes locally.
  - **The original round-trip test was also updated** (line 32-59) — its `expect(postedBody).toEqual(...)` no longer includes `work_profile` and the new line 57 explicitly asserts `expect(postedBody).not.toHaveProperty('work_profile')`. Passes locally.
  - **Net behavior change vs initial wave**: zero — the initial P1 wave sent `work_profile:'novel'` on every submit (which was the bug); the fix-wave-1 now sends `work_profile:<value>` only when explicitly selected, defaulting to a NULL-equivalent untouched form. This is the V1.66 wire shape preserved.
  - Confidence: **High** (manual diff + 2 passing tests covering both untouched and touched paths).

### Regression check (scoped to fix-wave-1)
- `pnpm --filter web typecheck` → clean (`tsc --noEmit` no output).
- `pnpm --filter web test` → **118 passed / 118** across 15 files. New `CreateWorkDialog test.tsx` is **8 tests** (was 4 in the initial P1 wave): the 4 originals + 2 new round-trip tests (W1 explicit novel + C1 game_bible) + 2 new wire-contract guard tests. All green.
- `git diff ebc7d977..HEAD --name-only` → exactly 2 source files: `create-work-dialog.tsx`, `create-work-dialog.test.tsx`. No collateral edits.
- The 3 Suggestions from the initial review (S1 literal-union narrowing, S2 SSOT extraction, S3 AGENTS.md table layout) remain deferred as residuals `R-V167P1-QC1-S1` / `R-V167P1-QC1-S2` / `R-V167P1-QC1-S3` per `qc-consolidated.md`; these are out of scope for this fix wave and correctly stay open.

### Verdict (post-revalidation)
**Approve** — Both merge-blocking findings (C1 Critical, W1 Warning) are Resolved with evidence (diff + passing tests + 3-source backend canonical-set verification). The 3 Suggestions tracked as residuals are deferred per PM's `qc-consolidated.md` plan and are not merge-blocking. No new regression observed. Per `mstar-review-qc` §"门禁规则" (no unresolved Critical, no unresolved Warning), this review now passes.

## Notes (positive — what this plan got right)
- **G2 transport promotion (21 → 24) is clean and minimal.** Three new methods on `NexusClient` (`getPreset`/`updatePreset`/`deletePreset`), three new imports from `@42ch/nexus-contracts`, one new private `delete<T>()` transport helper that mirrors the existing `get`/`post`/`patch`/`put` shape (uses the same `request<T>()` core that already handles 204 → `undefined` at `browser-client.ts:288-291`). Generated TS types confirmed present at `packages/nexus-contracts/src/generated/local-api/preset-management/{GetPresetResponse,UpdatePresetRequest,UpdatePresetResponse}.ts`. **No form-based preset-management UI is built** — the new methods exist on the client surface but have no caller beyond the test block; UI is correctly deferred to the V1.68 canvas. Confirmed by `git diff 26e477ee..HEAD --name-only -- apps/web/src/` — only the 6 expected files changed.
- **TauriClient thin-augmentation architecture preserved.** `TauriClient extends BrowserClient` (`tauri-client.ts:72`) — no method duplication, all 24 data methods inherited unchanged. Only the constructor and a `port` field are Tauri-specific. The doc-comment count was updated `21 → 24` in both places (file header line 6 + class JSDoc line 67) — consistent. The test file `tauri-client.test.ts` exercises the constructor / port resolution; it does not need to re-test the 3 new methods (they are covered by the BrowserClient path) — this matches the §5 #1 LOCKED architecture.
- **`test-providers.tsx` noopClient is safe** (`as unknown as NexusClient` at line 34). Adding 3 new methods to the interface does not break the unsafe-cast test client — verified by the 114/114 test pass.
- **Accessibility of the new selector is correct.** `Select` is a forwardRef-wrapped native `<select>` element (`apps/web/src/components/ui/select.tsx:16-32`) — keyboard and screen-reader behaviour come for free from the native element. The label is associated via `<Label htmlFor="work-profile">` (line 117) and the control has matching `id="work-profile"` (line 119). DESIGN.md §Component Primitives/Select heights/colors are consumed via Tailwind utilities; no new design tokens invented.
- **Form reset on dialog open is correct.** `useEffect` at `create-work-dialog.tsx:45-53` clears title, long-term goal, initial idea, work profile, and error when `open` flips true — preventing a state leak between successive dialog opens.
- **No schema change, no `@42ch/nexus-contracts` version bump.** `check-wire-drift.sh` is clean (4/4 drift tests pass); only client interface promotion and a UI control; generated TS types and daemon routes pre-existed. Plan's AC4 verified.
- **`pnpm --filter @42ch/nexus-contracts run build`** → clean (82.09 KB `.d.ts`, regenerated).
- **`pnpm --filter web typecheck`** → clean (`tsc --noEmit` no output = no errors).
- **`pnpm --filter web test`** → **114 passed / 114** across 15 files (including the new selector-change test at `create-work-dialog.test.tsx:58-77` and the 3 new preset-CRUD tests at `browser-client.test.ts:233-276`). React Router v7 future-flag warnings appear in stderr across multiple files — pre-existing, not introduced by this plan.
- **`apps/web/AGENTS.md` housekeeping is appropriate.** Both "Contracts status" rows for the closed residuals are updated in lockstep with the code; "Future preset-CRUD plan" → "V1.68 canvas UI" reflects the §0 Q6 deferred-UI decision.
