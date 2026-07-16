---
iteration_id: V1.119
start_date: 2026-07-15
status: completed
end_date: 2026-07-16
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.119
plans:
  - 2026-07-15-v1.119-setup-continue-unblock
  - 2026-07-15-v1.119-setup-workspace-profile-path
  - 2026-07-15-v1.119-setup-agent-picker-catalog
---

# V1.119 Delivery Compass — Setup unblock + polish

> **Phase 1:** product-manager §5.1 · architect §5.2 · writing-specialist §5.3 — all done (2026-07-15).
> **PM lock (§5.4):** `status: locked`. Prepare gates pass on all three plans (specify / clarify / plan). Spec freeze locked.

### PM lock notes (§5.4)

1. **P0 Must** — Continue unblock is the dogfood gate; happy path must advance without Reset.
2. **P1 after P0** — same Workspace step; `blocked_by` P0 documented.
3. **P2** — AgentPicker independent; Settings shares picker; may parallel after P0.
4. **`wire_contracts_changed: false`** — all three plans (architect AD-*).
5. **Residuals for implement:** P0 locale keys `setup.continueError.helper.*`; optional P2 native descriptions.

## Product story

After V1.118, authors dogfooding Setup hit a hard stop on the Workspace step: **继续** fails, the UI stays put, and **重置** appears. Further product feedback cannot continue until first-launch Setup works again.

Alongside that blocker, intake locked three Setup polish themes: honest AgentPicker catalog, Profile default name `default`, path sync with name unless folder picked, and focus-ring layout hygiene.

The coherent bet:

> **First-run Setup must complete on the happy path; then Agent and Workspace fields behave as authors expect.**

| Who | Pain today | What they get when V1.119 is Done |
| --- | --- | --- |
| **Authors (Workspace Continue)** | Continue fails; Reset appears; blocked | Happy path `default` / `.../nexus/default` advances to Done |
| **Authors (Profile + path)** | Name ≠ path segment; focus overlaps label | Default name `default`; path tracks name until folder picked; focus does not cover next label |
| **Authors (Agent step)** | Placeholder icons; Install on installed cards; ACP dupes; thin default list | ACP icons for native; calm installed cards; curated 12-agent grid + More without `*-acp` |

### Grill-me decisions (locked)

1. **Intake stop:** Feedback ended when Continue fully blocked dogfood (F7).
2. **Direction:** V1.119 = Setup unblock + polish only (not Creation depth / residual slate).
3. **P0 F7:** Fix Continue root cause; inline errors; Reset only for migration/DB-class failures.
4. **P1 F4–F6:** Default display name `default`; focus overlap fix; path last-segment sync gated by `workspacePicked`.
5. **P2 F1–F3:** ACP icons for native Claude/Codex; hard-hide `claude-acp`/`codex-acp`; installed omit Install/Docs; curated order + installed-first sort; Settings shares AgentPicker.
6. **Branch policy:** `main` → `iteration/v1.119` → `main`.

## Scope slices (non-overlapping)

| Slice | Plan | Surface boundary | Ships alone? |
| --- | --- | --- | --- |
| **P0** | setup-continue-unblock | Workspace Continue / bootstrap / error UX | Yes — unblocks dogfood |
| **P1** | setup-workspace-profile-path | Workspace step name + path + focus | Prefer after P0 (same step) |
| **P2** | setup-agent-picker-catalog | Agent step + Settings AgentPicker | Yes — independent of P0/P1 runtime |

**Overlap guard:** P0 must not redesign AgentPicker. P1 must not change bootstrap recovery semantics beyond F7 UX gates. P2 must not change Workspace path/name logic.

## Plans

| plan_id | Name | Status | Tier | Notes |
|---------|------|--------|------|-------|
| 2026-07-15-v1.119-setup-continue-unblock | Setup Continue unblock | Done | Must / P0 | F7 - QC Approve w/ residuals, QA Pass |
| 2026-07-15-v1.119-setup-workspace-profile-path | Workspace profile + path | Done | Must / P1 | F4 F5 F6 - QC Approve w/ residuals, QA Pass |
| 2026-07-15-v1.119-setup-agent-picker-catalog | AgentPicker catalog polish | Done | Must / P2 | F1 F2 F3 - QC Approve w/ residuals, QA Pass |

### Plan dependencies (implement order)

| Plan | Depends on | Rationale |
| --- | --- | --- |
| P0 Continue unblock | — | Dogfood gate |
| P1 Workspace profile + path | P0 | Same step; avoid conflicting Continue UX while P0 lands |
| P2 AgentPicker catalog | — | May prepare in parallel after P0 unblocks; no runtime dep on P1 |

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Product specify + clarify (§5.1) | 2026-07-15 | done |
| Architect plan lock (§5.2) | 2026-07-15 | done |
| Writing Review & Edit (§5.3) | 2026-07-15 | done |
| Spec freeze (iteration package) | 2026-07-15 | locked (§5.4 PM) |
| Dev complete | TBD | pending |
| QC complete | TBD | pending |
| Iteration close | TBD | pending |

## Acceptance Criteria (iteration-level)

| ID | Criterion | Verification |
| --- | --- | --- |
| IC-1 | Desktop Setup Workspace Continue on default name/path advances to Done without Reset | Happy-path Continue → Done step |
| IC-2 | Continue failures show inline alert; Reset only for migration/DB recovery | Soft failure → inline error, no Reset; migration failure → Reset OK; display-name failure → inline, no Reset, no advance |
| IC-3 | Profile name defaults to `default` | Open Workspace step; field value is `default` |
| IC-4 | Profile Input focus does not cover Workspace folder label | Focus name field; label fully visible |
| IC-5 | Unpicked folder: path last segment tracks Profile name slug; after pick: name changes do not alter path | Type name without Browse; Browse then type |
| IC-6 | Claude/Codex native show ACP icons; `*-acp` absent from grids | Visual + catalog ids |
| IC-7 | Installed cards omit Install/Docs; uninstalled retain them | Installed vs not-installed cards |
| IC-8 | Default grid order/groups match F3; More excludes ACP wrappers | Installed-first → curated uninstalled → More |

## Non-Goals

- Creation Memories / World authoring / WorkDetail re-home (V1.118 next)
- Medium residual slate (TOFU, i18n gaps, CodexNative HostManager)
- Platform / remote auth
- DF-70 execution-mode matrix / DF-71 menu-bar daemon
- Reintroducing ACP wrappers as selectable agents
- Wire-contract churn unless architect proves P0 needs additive fields

## Roadmap Position

- **Current iteration (V1.119):** `delivered` - Unblock Setup Continue + Workspace/Agent polish from feedback F1–F7
- **Next iteration:** Deeper Memories IA / World authoring UX / WorkDetail + Body discoverability / medium residual paydown — **trigger:** V1.119 PR merged + dogfood unblocked; **owner:** PM
- **North star:** Desktop first-run completes calmly; Setup fields match author mental models

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.119` |
| `target_branch` | `main` |

## Architect decisions (locked — §5.2)

Technical approach locked by `@architect` (2026-07-15). Implementers MUST follow iteration `specs/` + plan Prepare Package (Architecture); fold durable stubs into `.mstar/specs/{desktop-shell,web-ui}.md` at iteration P5.

### AD-P0 — Continue error classifier + inline UX

| Decision | Choice | Rationale |
| --- | --- | --- |
| Structured IPC vs string | **Web-side classifier** (`classifySetupContinueError`) is SSOT for Reset gating in V1.119; **no new Rust bootstrap error enum** unless T1 repro proves ambiguous strings | `ensure_setup_bootstrap` returns plain `String` today; `asDesktopError` already accepts `{ code, message }` when Rust emits it. HTTP `updateCreator` failures expose daemon `error.code`. Avoid desktop-crate churn unless T1 requires one additive serialized code |
| UI state | Store **`continueError: { message, class } \| null`**; derive **`showReset = (class === 'migration_db')`**; bind inline alert to `message` only | Product IC-2: Reset never keyed on “any error” |
| Classes | `happy` \| `soft_workspace_path` \| `soft_bootstrap` \| `soft_display_name` \| `migration_db` | Display-name + workspace-path failures → soft classes, **never** Reset |
| Migration signals | Ordered: (1) structured code ∈ `{ migration_failed, database_migration_failed, database_error }` when present; (2) message `/migration/i`; (3) sidecar `detail` with `Daemon output:` tail containing `migration` | Matches sidecar stderr tail formatting |
| Workspace path failure | Promote from toast-only to **same inline alert region** with `soft_workspace_path` | AC-P0-2 parity with bootstrap/display-name |
| Inline placement | `role="alert"` block **above CTA row** in step body; toast remains secondary duplicate | AC-P0-4 |
| `wire_contracts_changed` | **`false`** | Tauri IPC + web UX only |

### AD-P1 — Profile slug + path reconcile

| Decision | Choice | Rationale |
| --- | --- | --- |
| Slug module | Pure helper `slugProfileSegment(name)` in `apps/web/src/lib/workspace-profile-slug.ts` (unit-tested) | Shared by mount reconcile + `onChange` sync |
| Windows reserved | Exact segment match (case-insensitive) against `CON, PRN, AUX, NUL, COM1–COM9, LPT1–LPT9` → append `-profile`; if still empty/illegal-only → `default` | Cross-platform safe path segments on future Windows desktop |
| Illegal chars | Remove `/ \ : * ? " < > \|` and `\0`; NFKC; whitespace runs → `-`; collapse/trim `-`; empty → `default` | Product slug rule §P1 |
| Mount reconcile | After `getWorkspaceRoot`, when `!workspacePicked`: if `basename(path) !== slug(profileDisplayName)`, **rewrite displayed last segment once** (no `setWorkspacePath` until Continue) | Fixes stale `nexus42` / `local` segments; skip when already aligned |
| Focus/layout | `scroll-margin-top` on Profile Input + existing wizard-stack spacing; verify at **480px** card width | AC-P1-2 |
| `wire_contracts_changed` | **`false`** | Wizard state + display path only |

### AD-P2 — AgentPicker catalog overrides

| Decision | Choice | Rationale |
| --- | --- | --- |
| Icon URLs (pinned) | `claude-native` → `https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg`; `codex-native` → `https://cdn.agentclientprotocol.com/registry/v1/latest/codex-acp.svg` | Verified live CDN 2026-07-15; `/v1/latest/` stable pointer |
| Hard exclude | **`excludeFromPicker: true`** on `claude-acp` / `codex-acp` in overrides; filter in catalog resolver **before** default/More partition | Extensible vs hardcoded TS id list |
| Static descriptions | Optional override `description` on `claude-native` / `codex-native` when scan metadata null — **residual OK** if omitted at ship | No wire enrichment required |
| Sort pipeline | `resolveCatalogItems` → drop `excludeFromPicker` → **default grid**: installed-first (priority asc, name asc), then curated uninstalled (priority 0–11), then other installed; **More**: remainder, installed-first, name asc | IC-6–IC-8 |
| Curated keys | Assign `priority` 0–11 in overrides per spec table; forward-compat name tokens unchanged | Registry id SSOT via `resolveAgentKey` |
| Settings parity | Same `defaultGridEntries` / `moreAgentsEntries` exports — no fork | AC-P2-6 |
| `wire_contracts_changed` | **`false`** | Overrides JSON + TS catalog only |

### Cross-plan merge order

`P0` → **P1** (same Workspace step) ‖ **P2** (independent; may prepare in parallel after P0 unblocks). P1 must not change Reset classifier semantics beyond happy-path defaults.

## Writing decisions (locked — §5.3)

Copy and corpus hygiene locked by `@writing-specialist` (2026-07-15). Locale strings ship with implementers; keys and intent are normative in iteration specs.

### P0 — Continue inline alert i18n

| Key (`setup` namespace) | EN intent | When |
| --- | --- | --- |
| `continueError.helper.soft` | Fix the issue and tap Continue again. | All soft classes (`soft_workspace_path`, `soft_bootstrap`, `soft_display_name`) |
| `continueError.helper.migrationDb` | If this persists, use Reset below to clear local database state. Your workspace files are not deleted. | `migration_db` only |
| `error.workspacePathFailed` | (existing) | Fallback body when path persist fails |
| `error.workspaceBootstrapFailed` | (existing) | Fallback body when bootstrap fails without daemon detail |
| `error.profileDisplayNameFailed` | (existing) | Fallback body when display-name persist fails |
| `action.resetLocalDatabase` | Reset | Reset CTA label — short verb only |

Inline alert: **body** = daemon/error `message` or phase fallback key; **helper** = class-selected key above. Toast may duplicate but must not be the only signal (IC-2). Deprecate conflated `toast.workspaceBootstrapFailedDescription` for inline-primary UX — toast should mirror the same class-selected helper when shown.

### P2 — Native agent static descriptions (overrides)

Optional `description` in `agent-catalog-overrides.json` when scan metadata is null:

| Key | Description (EN) |
| --- | --- |
| `claude-native` | Anthropic's agent for local coding with Claude. |
| `codex-native` | OpenAI's agent for local coding with Codex. |

Overrides are EN-only product copy (not locale files). Omit at ship if scan supplies description — residual OK per AD-P2.

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Continue root cause is migration-mismatch only on some machines | Med | High | Reproduce; classify error; gate Reset; fix happy path |
| Native agents lack version/description from scan | Med | Low | Override/fallback copy; do not block P2 on wire change |
| Catalog id mismatch for curated names (Hermes, Grok Build, …) | Med | Med | Map registry ids in architect plan; document gaps |

## Iteration package

| Path | Purpose |
|------|---------|
| `guides/` | Process notes |
| `specs/` | Iteration-scoped specs (P0–P2) |
| `README.md` | Package index |

## Quality Gate Summary

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-15-v1.119-setup-continue-unblock | Approve with residuals | Pass (mandatory) | 10 (low/nit) | QC consolidated: `.mstar/sdd/2026-07-15-v1.119-setup-continue-unblock/review/qc-consolidated.md` |
| 2026-07-15-v1.119-setup-workspace-profile-path | Approve with residuals | Pass (mandatory) | 8 (low/nit) | QC consolidated: `.mstar/sdd/2026-07-15-v1.119-setup-workspace-profile-path/review/qc-consolidated.md` |
| 2026-07-15-v1.119-setup-agent-picker-catalog | Approve with residuals | Pass (mandatory) | 7 (1 low, 6 nit) | QC consolidated: `.mstar/sdd/2026-07-15-v1.119-setup-agent-picker-catalog/review/qc-consolidated.md` |

## Compound Round Summary

**1 knowledge doc created:** `architecture-patterns/daemon-creator-pool-lazy-attach.md` - daemon creator pool lazy-attach pattern (from P0 root-cause investigation: daemon boots without pool on clean first run, `ensureSetupBootstrap` only writes config, Tier-1 handlers must call `ensure_creator_pool()` before pool access; web-only fixes are dead ends).

**Iteration package inventory (3 specs in `v1.119/specs/`):**
- `setup-continue-unblock.md` - Keep snapshot (promote target: `specs/desktop-shell.md` / `specs/web-ui.md` at P5 merge)
- `setup-workspace-profile-path.md` - Keep snapshot (same promote target)
- `setup-agent-picker-catalog.md` - Keep snapshot (same promote target)

Spec promotion deferred to post-merge P5 fold-in per compass promote target.

## Iteration Retrospective (minimal)

**What went well:**
- SDD per-task model worked effectively for P0 (4 tasks + fix wave); micro-batch for P1/P2 (small interconnected tasks) saved dispatch overhead
- T1 investigation task (root-cause repro) provided high-value evidence that shaped T2-T4 implementation
- QC tri-review caught a real Critical issue (QC2-C-001: migration error swallowed) that would have made AC-P0-3 unreachable in production

**What could improve:**
- Task-brief script (`task-brief`) expects `### Task N` headings but plan used `- [ ] T1:` list format; briefs were written manually
- P2 T1-T3 changes broke 33 tests (not 6 as initially expected) because fixture updates cascaded across 6 test files; T4 absorbed more work than anticipated
- T4 reviewer returned empty result (transient issue); PM had to accept based on implementer evidence + subsequent QC review

**Key learning:** The daemon creator pool lazy-attach pattern is now documented as a knowledge doc to prevent future developers from rediscovering the same root cause.
