---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-26-v1.67-frontend-scope-gaps"
verdict: "Request Changes"
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
