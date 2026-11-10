# Service-only shell promotion and home-control admission

## Problem

A fresh Nexus home opens the service in a **service-only, uninitialized** state: `workspace_initialized` is false and full-domain operations (`json_call`, host/provider work) are denied until a core is published. The browser first-run flow, however, needs the **home control plane** — list/create creators, list/select workspaces — *before* that publication, because onboarding establishes exactly the state that publication requires. Denying the whole control plane strands first-run onboarding (observed as the browser LAN onboarding gap, I-000060).

## Pattern

1. **Admit home control only.** In `service_only_uninitialized`, route creator/workspace NAPI calls through `CoreHomeService::open(user_home)` using the `user_home` recorded in `EnvState` at open time. Keep `deny_service_only` on the full-domain entry points (`json_call`, `host_call`, provider lanes).
2. **Promote on first selection, not on boot.** `POST /v1/daemon/workspace/active` runs select + promote under a per-service promise queue (WeakMap) so concurrent requests cannot double-open the core. The promotion reuses the exact startup publication sequence (`publish_full_core`), with `OpenMode::Promotion`; on failure it releases only candidate owners and keeps the shell `Open` + service-only so a retry re-runs promotion instead of hitting an idempotent no-op.
3. **Publish readiness last.** Clear the service-only marker only after core/host/provider slots all replace successfully and just before `publish_open`; set `workspace_initialized` only after promotion + execution owner succeed. A failed settlement must never leave `runtime/status` advertising `db up` while domain routes still deny.
4. **Materialize on select.** The first selection of a bootstrap-precreated `default` workspace registers its metadata (`meta.json` + `<creative_root>/.nexus42/workspace.json`) so the workspace is discoverable and its `creative_root` is real; the derivation rule (legacy `workspace_path_by_creator` → `workspace_path` → `~/Nexus`) lives on the server because the browser cannot resolve host paths.

## Failure modes to test

- Concurrent selects → exactly one promotion, no leaked owner, both requests succeed.
- Settlement failure → promotion fails, marker stays, retry re-runs (not a no-op `Ok`).
- Before promotion: domain routes still 409; after: same-session 200.
- Fresh-home creator creation writes the global identity row so `display_name` never degrades to the creator id.

## Why not the alternatives

- **Client-side or bootstrap marker**: reintroduces per-browser state and contradicts the derived gate (see the access spec D21).
- **Restart after selection**: breaks the single-session first-run promise the iteration exists to deliver.
- **Serving the create route instead**: creation has no core implementation yet (I-000063); selecting the pre-created default unblocks first run without inventing one.

## Files

- `crates/nexus-core-node/src/{env_state,lifecycle,actors}.rs` — home admission, `promote_core`, `publish_full_core`.
- `crates/nexus-core/src/{creators,home}.rs` — fresh-home identity, workspace materialization.
- `apps/nexus-service/src/workspaces.ts` — route family, selection queue, promotion ordering.
- `packages/nexus-native/src/{index,loader}.ts` — binding exposure.
