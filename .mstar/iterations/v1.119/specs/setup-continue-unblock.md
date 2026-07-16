# P0 Spec — Setup Continue unblock

**Status:** Draft (Phase 1 — product §5.1, architect §5.2, writing §5.3 locked)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Promote target:** fold into [desktop-shell.md](../../../specs/desktop-shell.md) / [web-ui.md](../../../specs/web-ui.md) wizard sections at iteration P5

## Problem statement

On Desktop Setup Workspace step, clicking **Continue** does not advance. A **Reset** button appears instead. Authors are blocked from finishing Setup and from further dogfood feedback.

**Observed failure modes (product):**

| Mode | Symptom | Root cause class |
| --- | --- | --- |
| A | Continue disabled, no error | Empty `profileDisplayName` (P1 fixes default `default`; P0 must not regress once name is set) |
| B | Continue clicked, step stuck, Reset visible | `ensureSetupBootstrap()` or post-bootstrap persist throws; **Reset currently renders for all `bootstrapError` values** |
| C | Continue clicked, nothing visible | Error surfaced toast-only; inline region missing or easy to miss |

Code path: [`setup-step-workspace.tsx`](../../../../apps/web/src/pages/setup-step-workspace.tsx) sets `bootstrapError` when `ensureSetupBootstrap()` throws or when post-bootstrap `updateCreator(display_name)` fails; Reset renders when `bootstrapError && desktop` (over-broad today).

## Target users

| Persona | Scenario |
| --- | --- |
| New / returning author | Workspace Continue on happy path `default` + `~/Documents/nexus/default` (or desktop-resolved equivalent) |
| Author with transient network/daemon issue | Soft failure — inline message, retry Continue, **no Reset** |
| Author recovering from bad local DB | Migration/schema mismatch — inline message + Reset allowed |

## User stories

1. **As a new author**, when I tap Continue with the default Profile name and workspace path, Setup advances to Done without seeing Reset.
2. **As an author**, when Continue fails for a recoverable reason, I see the error **inline above the CTA row** and can tap Continue again without destructive recovery.
3. **As an author** with a corrupted local database, I see a clear inline error **and** Reset when the failure is migration/DB-class only.

## Product rules (normative)

1. Happy path Continue **must** reach Done without showing Reset.
2. Failures **must** show **inline** error text in the Workspace step body (above the CTA row), with `role="alert"`. Toast may duplicate the message but **must not** be the only signal (AC-P0-4).
3. Reset is allowed **only** when the failure is classified **migration/DB-class** (see classifier below). All other failures: inline error, **no Reset**.
4. **Display-name persist failure** (`updateCreator` after successful bootstrap): **soft failure** — inline error, **no Reset**, **do not advance**; author retries Continue or edits the name. Bootstrap state is already valid.
5. **Workspace path persist failure** (before bootstrap): inline error, **no Reset**, do not advance; author fixes path or retries.
6. Continue **must remain enabled** after a soft failure (unless loading) so retry is one tap.

## Error classifier (normative — see Architecture contract)

Implementation locked in § Architecture contract. Product invariant: **Reset is never shown for display-name or workspace-path soft failures**.

## Inline error copy (normative intent)

### Alert structure

| Region | Source | Notes |
| --- | --- | --- |
| **Body** | Daemon/error `message` when present; else phase fallback key (below) | Primary signal; `role="alert"` |
| **Helper** | Class-selected i18n key (below) | Sentence case; appears below body, above CTA row |
| **Reset CTA** | `action.resetLocalDatabase` | Short label **Reset** / **重置** — only when `showReset` |

### i18n keys (`setup` namespace)

Implementers add locale strings at T3; keys are normative now.

| Key | EN copy | zh-CN copy (intent) | When |
| --- | --- | --- | --- |
| `continueError.helper.soft` | Fix the issue and tap Continue again. | 请修正问题后再次点击「继续」。 | `soft_workspace_path`, `soft_bootstrap`, `soft_display_name` |
| `continueError.helper.migrationDb` | If this persists, use Reset below to clear local database state. Your workspace files are not deleted. | 如果问题仍然存在，请使用下方的「重置」清除本地数据库状态。你的工作区文件不会被删除。 | `migration_db` only |
| `error.workspacePathFailed` | Could not save the workspace path. | (existing) | Body fallback — path phase |
| `error.workspaceBootstrapFailed` | Could not prepare your local workspace. | (existing) | Body fallback — bootstrap phase |
| `error.profileDisplayNameFailed` | Could not save the Profile name. | (existing) | Body fallback — display-name phase |
| `action.resetLocalDatabase` | Reset | (existing) | Reset button label |

**Component binding:** `helperKey = continueError.class === 'migration_db' ? 'continueError.helper.migrationDb' : 'continueError.helper.soft'`.

**Toast:** May duplicate body + helper; must not be the only signal (AC-P0-4). Do not use the legacy conflated `toast.workspaceBootstrapFailedDescription` for inline-primary UX — when toast is shown, mirror the same class-selected helper.

Reset button label stays **Reset** (`setup.action.resetLocalDatabase`); migration recovery detail lives in the helper line only.

## Scope boundary

| In scope | Out of scope |
| --- | --- |
| Root-cause fix for Continue failure on happy path | AgentPicker catalog redesign (P2) |
| Inline error region + Reset gating | Profile name default / path sync (P1) — except not regressing Continue when name is `default` |
| Tests aligned to classifier | Platform auth |
| Reproduce evidence for root cause (Task T1) | Changing bootstrap idempotency semantics |

## Acceptance criteria

| ID | Criterion | Verification |
| --- | --- | --- |
| AC-P0-1 | Happy path Continue advances to Done | Profile `default`, path `~/Documents/nexus/default` (or resolved desktop root ending in `/default`), no Reset |
| AC-P0-2 | Soft failures show inline error without Reset | Simulate display-name failure; simulate non-migration bootstrap error (`config write failed`) |
| AC-P0-3 | Migration-class failure shows inline error + Reset | Known migration-mismatch path or classified stub |
| AC-P0-4 | Toast is secondary only | Inline alert present whenever `bootstrapError` set |
| AC-P0-5 | After soft failure, Continue retry succeeds without reload | Click Continue twice after mocked transient failure |

## Architecture contract (normative — architect locked)

### Error classifier (implementation)

Module: `apps/web/src/lib/setup/continue-error.ts`

| Class | Phase | Reset? | Advance? | Detection (first match wins) |
| --- | --- | --- | --- | --- |
| `soft_display_name` | `display_name` | No | No | Always — `updateCreator` failure after bootstrap OK |
| `soft_workspace_path` | `workspace_path` | No | No | Always — `setWorkspacePath` failure |
| `migration_db` | `bootstrap` | **Yes** | No | Structured code ∈ `{ migration_failed, database_migration_failed, database_error }`; **or** message `/migration/i`; **or** sidecar `detail` contains `Daemon output:` + `migration` |
| `soft_bootstrap` | `bootstrap` | No | No | Default when not migration-class |

HTTP failures: read `error.code` from daemon `ErrorResponse` when thrown by client adapter. Tauri failures: read `DesktopCapabilityError.code` when present.

### UI state

| Field | Rule |
| --- | --- |
| `continueError` | `{ message: string, class: SetupContinueErrorClass } \| null` — set on any Continue-path failure |
| Inline alert | Render when `continueError !== null`; `role="alert"`; **above CTA row** in step body; helper copy from i18n |
| `showReset` | **Derived:** `continueError?.class === 'migration_db'` — never bind Reset to message presence alone |
| Continue enabled | Remains enabled after soft classes unless loading (AC-P0-5) |

Workspace path failures **must** populate `continueError` (not toast-only).

### Wire contracts

**`wire_contracts_changed: false`** — no new Tauri return types required; optional additive Rust `{ code, message }` for bootstrap only if T1 repro demands it.

## Open questions (architect)

~~All resolved in § Architecture contract.~~

1. ~~IPC error-code mapping~~ → web classifier + existing HTTP/Tauri codes.
2. ~~Structured IPC vs string~~ → web classifier default; Rust enum deferred to T1 if needed.
3. Root-cause repro — **Task T1** (implementer evidence).
