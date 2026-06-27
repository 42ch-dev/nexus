---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.69-frontend-residuals"
verdict: "Approve"
generated_at: "2026-06-27"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk — type-safety of `WorkProfile` narrowing + `isWorkProfile` guard (C1), wire-value fidelity for `game_bible` vs schema/DB (C1/C2), preset-method parity guard + test semantics (C3), query-key factory shape and collision risk (C4), absence of `any`/unsafe casts, zero wire/contract drift.
- Report Timestamp: 2026-06-27

## Scope
- plan_id: `2026-06-27-v1.69-frontend-residuals`
- Review range / Diff basis: `iteration/v1.69...feature/v1.69-frontend-residuals` (3 commits: `5eacda0c`, `77f26bb3`, `4b1f6433`; ~5 files, +138/−31)
- Working branch (verified): `feature/v1.69-frontend-residuals`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 5 (`apps/web/src/lib/work-profiles.ts` (new), `apps/web/src/pages/dialogs/create-work-dialog.tsx`, `apps/web/src/pages/dialogs/create-work-dialog.test.tsx`, `apps/web/src/lib/nexus/adapter-contract.test.ts`, `apps/web/src/lib/nexus/query-keys.ts`)
- Commit range: `5eacda0c refactor(web): narrow work_profile to literal union + extract SSOT module`, `77f26bb3 test(web): adapter-contract 21->24 method coverage + preset parity guard`, `4b1f6433 refactor(web): stage preset query-key detail structure for V1.70 canvas`
- Tools run: `git fetch origin`, `git checkout feature/v1.69-frontend-residuals`, `git branch --show-current`, `git rev-parse --show-toplevel`, `git diff iteration/v1.69...HEAD --stat`, `git log --oneline iteration/v1.69...HEAD`, full reads of all 5 changed files + plan, `pnpm --filter @42ch/nexus-contracts run build` (pre-req), `pnpm --filter web run typecheck` (clean), `pnpm --filter web run test -- adapter-contract create-work-dialog --run` (all 16 adapter + 8 dialog tests passed), grep for `work_profile|game_bible|WorkProfile|isWorkProfile|any|as any|as unknown` across scope, schema inspection (`schemas/local-api/works/create-work-request.schema.json` and siblings), crate grep for backend canonical set (`game_bible` underscore in DB CHECK + Rust helpers).

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
None.

## Source Trace

**C1 — `R-V167P1-QC1-S1` (type narrowing + guard correctness)**:
- `create-work-dialog.tsx:39`: `const [workProfile, setWorkProfile] = useState<WorkProfile>(WORK_PROFILES[0].value);`
- `onChange` (lines 123-131): `if (isWorkProfile(e.target.value)) { setWorkProfile(e.target.value); setWorkProfileTouched(true); }` — guard is the only setter path.
- Reset path (line 51) and initial mount both source from `WORK_PROFILES[0].value` (typed literal).
- `work_profile` is sent only when `workProfileTouched` (line 70), preserving V1.66 NULL semantics for untouched forms.
- `isWorkProfile` (work-profiles.ts:54-55): `(WORK_PROFILE_VALUES as readonly string[]).includes(value)` — exact set match, no `any`, no cast of the input value.
- No other call sites in `apps/web/src/` mutate the state directly (grep confirmed only dialog + its test + SSOT).
- Wire schema (`create-work-request.schema.json:19`) is open `string` (intentional — daemon stores verbatim). Backend canonical set (DB CHECK + `nexus-local-db/src/works.rs` + daemon handlers) is exactly `novel | essay | game_bible | script`.
- Test (`create-work-dialog.test.tsx:174-175`): explicitly asserts `game_bible` (underscore) is emitted, not `game-bible`.
- No unguarded string can reach typed state; Select only emits known values; guard is defense-in-depth at the boundary.
- Finding: no code path, no runtime value from backend, no type erosion.

**C2 — `R-V167P1-QC1-S2` (SSOT + wire fidelity)**:
- New `apps/web/src/lib/work-profiles.ts` is the single source (plan AC + grep: only dialog + test + SSOT now reference the four values).
- `WorkProfile` literal union, `WORK_PROFILE_VALUES` (with `satisfies readonly WorkProfile[]`), `WORK_PROFILE_LABELS`, `WORK_PROFILES` derived array — all drift-proof.
- `game_bible` (underscore) matches crates exactly (no hyphenated form in daemon/DB path for the web surface).
- Schema remains open `string` — no contract change. Pure client-side TS refactor.
- Test pins both cardinality (4) and exact wire value.

**C3 — `R-V167P1-QC3-S1` (preset parity + test correctness)**:
- `adapter-contract.test.ts:305-341`: new contract test for the three promoted methods.
  - Asserts exact `GET /v1/local/presets/user%2Ffoo`, `PATCH ...` with body `{ yaml: '...' }`, `DELETE ...`.
  - 204 → `resolves.toBeUndefined()` (void contract).
  - Uses the `fetchImpl` seam owned by this file (correct boundary per comment).
- Transport-parity test now calls the three methods and asserts `seen.size === 24` + spot-checks the three paths.
- Parity guard (lines 391-395): `PRESET_METHODS = ['getPreset','updatePreset','deletePreset'] as const satisfies readonly (keyof NexusClient)[]`
  - Compile-time: future removal/rename becomes type error here.
  - Runtime: two tests (BrowserClient, TauriClient thin-over) assert `typeof client[method] === 'function'`.
- No false-positive risk: tests directly exercise path/verb/body/204 semantics; guard would have caught any drift introduced by the 21→24 promotion.
- No `any` in the new test code.

**C4 — `R-V167P1-QC3-S2` (query-key correctness)**:
- `query-keys.ts:45-46`:
  ```ts
  details: () => [...queryKeys.presets.all, 'detail'] as const,
  detail: (presetId: string) => [...queryKeys.presets.details(), presetId] as const,
  ```
- Shape is identical to `works.details/detail`, `chapters.details/detail` — no namespace collision.
- Comment correctly notes current invalidation still uses `all` (covers list + details); detail key is staged for V1.70 canvas. No `invalidateQueries` call in this delta.
- No other key factories define a colliding `presets.detail` path.

**Cross-cutting**:
- Zero `any`, zero unsafe casts, zero `// @ts-` or error suppression in the five changed files for this delta.
- Zero schema/DTO/contract change (schemas untouched; generated contracts not touched).
- Typecheck: clean.
- Relevant tests: 16 adapter-contract + 8 create-work-dialog tests all pass.
- Lint gates not re-run in full (per scope: this is a pure frontend residual closure wave; prior CI baseline assumed clean).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

All four residuals (C1–C4) are closed with correct type narrowing, wire-value fidelity, test semantics, and query-key shape. No security or correctness risk introduced. No `any`, no wire drift, no test false-positive surface. Per `mstar-review-qc` gate (Critical=0 and Warning=0 ⇒ Approve), this seat returns **Approve**.

## Revalidation
N/A — initial wave for this plan. No prior qc2 report exists for `2026-06-27-v1.69-frontend-residuals`.

## Evidence (verification-before-completion)
- Assignment fields verified on-disk: branch, cwd, exact diff range + three commit hashes reproduced.
- Full file reads + targeted greps for every C1–C4 concern (guard, setter paths, wire values, `satisfies`, query-key shape, `any`).
- Schema + crate cross-checks for `game_bible` canonical form.
- `pnpm --filter web run typecheck` (exit 0) and `pnpm --filter web run test -- adapter-contract create-work-dialog --run` (all 24 relevant tests passed).
- Report committed (only this path) before Completion Report v2.
