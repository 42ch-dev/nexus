# P0 Spec — Daemon no-Profile boot

**Status:** Draft (V1.118) — product specify + clarify **done**; architect **plan done** (2026-07-15)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Promote target:** fold into [desktop-shell.md](../../../specs/desktop-shell.md) §13.11 + [daemon-runtime.md](../../../specs/daemon-runtime.md) §17 at iteration P5

## Problem statement

Daemon boot opens creator `state.db` via `resolve_state_db_path`, which requires `active_creator_id` in `~/.nexus42/config.toml`. Empty or wiped home → fatal `No active creator`. V1.105 always-auto-starts the sidecar on desktop launch, so clean-state failure surfaces at the fullscreen daemon gate instead of inside the setup wizard.

Authors expect: **install app → daemon runs → complete setup → create/select Profile → use product**.

## Target users

| Persona | Scenario |
| --- | --- |
| New author | First desktop launch, empty `~/.nexus42` |
| Returning author (reset) | Deleted home dir to recover from bad state |
| Operator | CLI `nexus42 daemon start` on fresh machine |

## Product rule (normative)

**Daemon = system process. Profile = business gate.**

| Layer | Responsibility |
| --- | --- |
| **Daemon process** | Starts on empty home; initializes system dirs; serves health/ready + creators/setup |
| **Profile (creator)** | Required for business data (works, memory, worlds, schedules); selected via setup or footer Profiles |
| **Forbidden sole fix** | Silently auto-creating a "Default" Profile at boot to satisfy `state.db` open |

## Scope boundary

| In scope | Out of scope |
| --- | --- |
| `~/.nexus42/` system dir + config skeleton on boot | Platform / remote auth |
| Lazy/deferred `state.db` open until Profile present | Wire contract changes (unless architect proves additive endpoint) |
| Health/ready + creators CRUD/set-active without prior active creator | Removing wizard `ensureSetupBootstrap` (may remain idempotent on Continue) |
| Explicit uninitialized responses on Profile-scoped API routes | Medium residuals: TOFU (R-V192SEC-001), i18n (R-P1-001), CodexNative (R-V1116P0QA-001) |

## Relationship to V1.100 / V1.105 bootstrap

V1.100 introduced `ensureSetupBootstrap` (writes `active_creator_id` before daemon boot). V1.105 moved bootstrap to Workspace Continue but still assumed daemon needs a creator to boot.

**V1.118 supersedes that assumption:** daemon must boot **without** `active_creator_id`. Bootstrap IPC may still run for wizard convenience but is **not** a daemon prerequisite. See [desktop-shell.md](../../../specs/desktop-shell.md) §13.11 + [daemon-runtime.md](../../../specs/daemon-runtime.md) §17.

## Architecture contract (normative — architect locked)

### Lazy-open `state.db`

| Phase | When | Behavior |
| --- | --- | --- |
| **Boot** | `WorkspaceState::new` on empty/missing `active_creator_id` | Create `~/.nexus42` layout + config skeleton; **do not** call `Schema::init` on creator `state.db` |
| **Attach** | `PUT /v1/daemon/creators/active` succeeds, or Tier-2 handler finds `active_creator_id` in config | `ensure_creator_pool()`: resolve path via ADR-014, `Schema::init`, `DbPool::new`, store on state |
| **Idempotent attach** | Repeated attach / concurrent Tier-2 | Second call no-ops if pool already open for same creator |

Handlers **MUST NOT** call `state.pool()` before attach except Tier-0/Tier-1 code paths audited in P0 T1.

### API route tiers

| Tier | Auth | Active creator | `state.db` | Examples |
| --- | --- | --- | --- | --- |
| **T0** | No | No | No | `GET /v1/daemon/runtime/health`, `status` |
| **T1** | API key (local) | No | No | `GET/POST /v1/daemon/creators`, `PUT …/creators/active` |
| **T2** | API key | **Yes** | **Open** | `GET /v1/daemon/works`, memory, worlds, schedules, orchestration sessions, KB, reading |

Tier-2 without active creator → **`NexusApiError::Uninitialized`** (HTTP **409**, wire `error.code: "uninitialized"`). Do **not** add a new wire error code for P0.

`GET /v1/daemon/creators/active` when unset → HTTP **404**, `error.code: "not_found"` (semantics: no Profile selected yet).

### Wire contracts

**`wire_contracts_changed: false`** — behavior-only; existing `ErrorResponse` envelope sufficient.

## Acceptance criteria

| ID | Criterion | Verification (author / operator) | Priority |
| --- | --- | --- | --- |
| AC-P0-1 | Clean wipe of `~/.nexus42` → daemon/desktop reaches healthy **Running** without fatal `No active creator` | `rm -rf ~/.nexus42`; launch desktop or `nexus42 daemon start --foreground`; health probe / status indicator green | Must |
| AC-P0-2 | `GET /v1/daemon/runtime/health` (or equivalent ready probe) succeeds with **no** `active_creator_id` in config | Inspect config; call health endpoint; 2xx healthy | Must |
| AC-P0-3 | Creators list, create, and set-active (or equivalent setup path) work **before** any Profile is active | API or setup UI: create creator, set active — no prior `active_creator_id` | Must |
| AC-P0-4 | Profile-scoped routes (`/v1/daemon/works/*`, memory, worlds, schedules, etc.) return **HTTP 409** `uninitialized` without active Profile — not process crash | HTTP client without active creator → 409 + `error.code: "uninitialized"` | Must |
| AC-P0-5 | After Profile selected, existing happy-path flows unchanged (create work, outline, memory) | Smoke test with one creator active | Must |
| AC-P0-6 | System dirs under `~/.nexus42/` exist after first boot (config skeleton, expected layout per home-layout crate) | List `~/.nexus42` after boot on empty home | Must |
| AC-P0-7 | Desktop always-start (V1.105): fullscreen gate reaches Ready on empty home — not stuck on bootstrap failure | Clean install desktop E2E or integration test | Must |

## Edge cases

| Case | Expected behavior |
| --- | --- |
| Config exists but `active_creator_id` absent | Daemon healthy; business routes gated |
| Config exists but creator id points to missing workspace | Explicit error on Profile-scoped ops; daemon still healthy |
| Re-run wizard `ensureSetupBootstrap` after P0 | Idempotent; does not break no-Profile boot |
| Browser tab (non-desktop) against empty daemon | Same health/creators behavior; setup flow unchanged |

## Non-goals

- Auto-create Default Profile as the only boot fix
- Changing Tauri sidecar always-start policy (V1.105 D2 stands)
- Consolidating all uninitialized errors to one wire code (optional later)

## Code anchors (implementation hints — not normative)

- `crates/nexus-daemon-runtime/src/config.rs` — `resolve_state_db_path`
- `crates/nexus-daemon-runtime/src/workspace/mod.rs` — workspace open
- `crates/nexus-daemon-runtime/src/api/handlers/creators.rs` — active creator read/write
- `crates/nexus-home-layout/` — system dir init
- `apps/desktop/src-tauri/src/lib.rs` — sidecar lifecycle

## Open questions for architect

~~All resolved in § Architecture contract.~~

1. ~~Lazy-open contract~~ → § Lazy-open `state.db` + route tiers.
2. ~~Uninitialized signal~~ → reuse `NexusApiError::Uninitialized` (409); `GET active` unset → 404.
3. ~~`wire_contracts_changed`~~ → **false**.
