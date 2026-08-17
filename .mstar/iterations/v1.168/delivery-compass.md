---
iteration_id: v1.168
start_date: 2026-08-17
status: locked
iteration_base_branch: main
target_branch: main
plans:
  - 2026-08-17-v1.168-p1-native-claude-codex-replace
  - 2026-08-17-v1.168-p2-dsh-native-provider
---

# v1.168 Delivery Compass

## Scope

Locks: [specs/v1.168-native-host-locks.md](specs/v1.168-native-host-locks.md) PD-1..PD-7 (grill-me 2026-08-17) + AR-1..AR-7 (architect pass 2026-08-17). This file is the iteration product SSOT for Scope / AC / Non-Goals / Roadmap; grill-me *decisions* and AR *resolutions* stay in the locks file.

### Problem

Authors harness local agents on the **native** Harness rail (`claude-native`, `codex-native`). Those adapters today own vendor JSON parsers and process protocols. CLI releases drift weekly; Nexus pays the parser tax; a decode miss can look like success or a hung turn. Authors who already run DeepSeek Harness locally have no native provider at all.

This is a **native-host series** iteration (not DF-81/82). It does not flip the frozen ACP-first rule: native stays supplementary, not a second control plane.

### Author value

| Author | After V1.168 |
|--------|----------------|
| Already uses Claude Code CLI or Codex CLI via native providers | Same provider ids on scan/Setup. Turns still stream, cancel, and shut down. Session continuity stays a native capability claim. Vendor wire formats are no longer Nexus-owned. A turn that cannot be decoded **fails once**, it does not succeed empty or hang. |
| Has a local DeepSeek Harness runtime | Third native provider `dsh-native` appears when the runtime is discoverable. Bring-your-own — Nexus does not install, bundle, or npx-launch it. |
| Has not installed a given CLI | That native id is **absent** (skip), not a broken catalog row. |

**Persistent empty-line Claude mode** is not an author-facing Setup option (default execute is already per-invocation). Deleting it does not change the default author path. Multi-turn continuity remains `native_cli_limited.session_restore` (crate-owned resume), not a custom always-on stdin protocol.

### Locked work (PD-1..PD-7)

- **Replace Claude/Codex native internals (PD-1, PD-2)** — complete replace, not a second stack. Keep `ProviderAdapter`, `HostEvent` normalization, PATH-scan **mechanism**, permission policy, `HostManager` registration, and ids `claude-native` / `codex-native`. Internals become `claude-codes` / `codex-codes` `async-client` (types included). Codex uses **app-server JSON-RPC**, not `codex exec --json`. Claude uses **stream-json**, not `--print`. Delete Claude persistent mode. P2 **adds** `dsh-native` to the same scan/boot path; it does not invent a second discovery system.
- **Decode contract (PD-3)** — unknown variant/method: per-item skip + debug. typed-decode / stream abort (`Error::Deserialization` and kin): **whole turn `OpFailed`**, never fake success. T1 of P1 verifies crate behavior before wiring. Keep `native_cli_limited()` claims **as-is** (do not raise the false fields; do not silently drop `streaming` / `cancellation` / `session_restore`).
- **Add dsh-native (PD-4, PD-5)** — new id `dsh-native` via crates.io `deepseek-harness-sdk` exact pin. Discover via PATH `dsh-jsonrpc-agent` **or** `DSH_RUNTIME_BIN`. No bundled runtime, no default npx. No `NATIVE_PREFERRED_FAMILIES` entry (no ACP twin → do not suppress any ACP row). Same decode contract. Surfacing = existing native catalog/scan (no new Setup chrome).
- **ACP rail unchanged (PD-6)** — `nexus-acp-host` / `claude-acp` / `codex-acp` out of scope. `docs/` must not pin third-party versions.

Grill-me locks (2026-08-17, user): direction = native host series (not DF-81/82); decode = stream-level OpFailed; dsh dep = crates.io exact pin; two plans; branch = `main → iteration/v1.168 → main`; dsh discovery = PATH + env.

### Architecture notes (AR-1..AR-7, architect pass 2026-08-17)

Resolved in [specs/v1.168-native-host-locks.md](specs/v1.168-native-host-locks.md) § AR. Author-visible consequences:

- `claude-native` / `codex-native` keep `native_cli_limited()` unchanged — crate-owned resume (`session_restore`), real cancel (`interrupt` / `turn_interrupt`), message-granularity streaming.
- `dsh-native` ships the AR-6 documented narrower descriptor — fields in [AC-V168-4](#acceptance-criteria), rationale in locks § AR-6; per turn exactly one `MessageDelta(final_response)` + one terminal event.
- Codex app-server turns run `AskForApproval::Never` + `SandboxPolicy::ReadOnly` (headless parity with today's `-s read-only`); residual approval requests are auto-answered by the existing native permission classification (`Allow` → accept, `Deny`/`Ask` → deny).

### Intent gate (iteration)

| Gate | Statement |
|------|-----------|
| **True goal** | Authors harness local Claude Code, Codex, and DeepSeek Harness CLIs through the existing native Harness rail without Nexus owning vendor wire parsers. |
| **Success** | AC-V168-1..6. Existing native ids unchanged; dsh-native skip-if-missing; decode honest; ACP unchanged; claude/codex capability claims unchanged; dsh ships the AR-6 documented narrower descriptor. |
| **Non-goals** | See [Non-Goals](#non-goals). |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-08-17-v1.168-p1-native-claude-codex-replace | Replace Claude/Codex native internals | Done | T1 b5efb649 + T2 da1a3863 + T3 2917fc81 + fix wave 79ca73b2 (QC B-1/B-2/B-3); QC tri → Request Changes → targeted revalidation ×3 Approve; QA full Pass with notes; merge 0be48f0c |
| 2026-08-17-v1.168-p2-dsh-native-provider | Add dsh-native provider | InProgress | unblocked (P1 merged); dsh.rs + map_dsh.rs + pin + path_scan/boot |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Phase 1 lock) | 2026-08-17 | pending |
| P1 replace Claude+Codex native | 2026-08-17 | pending |
| P2 dsh-native | 2026-08-17 | pending |
| Iteration close + PR | 2026-08-17 | pending |

## Acceptance Criteria

- **AC-V168-1**: An author with `claude` on PATH still sees provider id `claude-native` on scan/boot. Execute / cancel / shutdown succeed on that id. The adapter no longer uses a Nexus-owned `--print` line parser or persistent empty-line Claude mode. `native_cli_limited` still claims `streaming`, `cancellation`, and `session_restore`. Implementation evidence: execute/cancel/shutdown go through `claude-codes` AsyncClient (stream-json); hand-rolled `--session-id`/`--resume` assembly is gone.
- **AC-V168-2**: An author with `codex` on PATH still sees `codex-native`. Execute / cancel / shutdown succeed on that id. Command approvals stay **auto accept/deny** under the existing native permission policy — no new approval prompt or UI. Sandbox is not more writable than today's native Codex path. Implementation evidence: `codex-codes` AsyncClient (**app-server**); `CodexJsonlEvent`, `exec --json` fallback, and `ChildReaper` are gone.
- **AC-V168-3**: Decode is honest for the author: unknown protocol items are skipped (the turn may still finish). If typed-decode fails or the stream aborts, that turn ends as **exactly one** failed operation (`HostEvent::OpFailed`) — never a fake `OpFinished`, never a hang waiting for a terminal event. Held in tests; T1 observes crate error shape before T2/T3 wire clients.
- **AC-V168-4**: When `dsh-jsonrpc-agent` is on PATH **or** `DSH_RUNTIME_BIN` is set, scan/boot lists `dsh-native` and the adapter implements `ProviderAdapter` with the same decode contract as PD-3 and the AR-6 documented narrower descriptor (`streaming: false`, `cancellation: false`, `session_restore: true`; locks § AR-6). When neither is present, `dsh-native` is absent (same skip-if-missing as claude/codex). Depends on crates.io `deepseek-harness-sdk` exact pin (version chosen at implement; **not** written in `docs/`). No default npx. No `NATIVE_PREFERRED_FAMILIES` row.
- **AC-V168-5**: ACP rail tests still green. `native_cli_limited` still claims no `structured_tool_calls` / `images` / `set_model` / `set_mode`. No `auth` feature from the community crates. No muse / opencode / antigravity crates. No new Harness/Setup/Settings chrome, install CTA, or env-var form.
- **AC-V168-6**: Verification baseline: `SQLX_OFFLINE=true` tests for `nexus-agent-host` + `nexus-daemon-runtime` (native boot tests), fmt + clippy `-D warnings`. Live CLI integration tests stay feature-gated / optional — CI proves mapper + fake-bin / absent-bin; a machine with the real bin is the live path.

## Non-Goals

- DF-81 mental-field authoring UX; DF-82 rules UI forms; DF-83 creator-bootstrap follow-ups. V1.167 already gated those; this iteration does not pick them.
- Raising native capability descriptors this iteration (`structured_tool_calls`, `images`, `set_model`, `set_mode` stay false).
- Silently **dropping** `streaming` / `cancellation` / `session_restore` on `native_cli_limited`. If a crate cannot honor a currently-true field, architect documents a narrower descriptor — do not change the claim quietly.
- Forking/patching `claude-codes` / `codex-codes` for Python-style Unknown-on-decode (evaluate only if T1 proves stream errors are unrecoverable *and* too frequent — Durable Roadmap, not silent P1 scope).
- Bundling a DSH runtime, default `npx` launch, path-depending the local `dsh-rust-sdk` checkout, or adding dsh to `NATIVE_PREFERRED_FAMILIES`.
- New author-facing install, Setup copy, Settings env-var UI, approval UI, or capability-raise UX.
- ACP protocol work, Connect Host, spoke pin changes, `claude-acp` / `codex-acp` / a `dsh-acp` twin.
- Writing dependency versions into `docs/` (project AGENTS.md rule).
- Dual-stack fallback to `--print` / `exec --json` after P1 T2/T3. Gemini or other extra native CLIs.
- Making native the preferred control plane (ACP-first remains).

## Roadmap Position

- **Current iteration (V1.168)**: stabilize the **native** Harness rail by replacing brittle self-written Claude/Codex adapters with maintained protocol clients, and add first-party DeepSeek Harness (`dsh-native`) as a third native provider. Native remains supplementary to ACP.
- **Next iteration**: DF-81 or DF-82 (both dogfood-evidence Y from V1.167); owner: product-manager; trigger: next `/iteration-start` pick. This iteration is a locked detour, not a cancellation of that pick. Optional follow-up: raise `structured_tool_calls` once mappings + permission are proven; optional crate fork if decode hardness remains a product issue.
- **End goal**: authors harness local agents (ACP or native) without Nexus owning vendor wire parsers.

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.168` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Codex app-server ≠ current `exec --json` (protocol swap) | High | High | T1 mapper on captures first; sandbox stays read-only-equivalent; approvals auto via existing native policy |
| typed-decode hard-fails on CLI weekly drift | High | Med | PD-3: stream → OpFailed (honest failure, not fake success); T1 verifies crate error shape |
| `deepseek-harness-sdk` pre-1.0 API churn | Med | Med | exact crates.io pin; isolate behind `DshNativeProvider` |
| Shared files (`boot.rs`, `path_scan.rs`) conflict if P1/P2 parallel | Med | Med | P2 `blocked_by` P1; serial merge |
| Community crate bus-factor (claude/codex-codes) | Med | Med | types+client only; no auth; HostEvent mapping owned here |
| Live CLI absent in CI | High | Low | mapper corpus + fake-bin tests; live tests optional; AC-V168-6 states this honestly |
| Session restore silently lost when hand-rolled `--resume` is deleted | Med | High | PD-3 + AR-5: crate resume mapped for all three providers (builder `.resume` / `thread_resume` / dsh session-id reuse); `session_restore` stays true everywhere — resolved, T1/T2/T3 tests hold it |

## Iteration package

| Path | Purpose |
|------|---------|
| `guides/` | Exploration notes (empty at start) |
| `specs/` | Iteration locks (`v1.168-native-host-locks.md`) |
| `README.md` | Package index |

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-08-17-v1.168-p1-native-claude-codex-replace | | mandatory | | |
| 2026-08-17-v1.168-p2-dsh-native-provider | | mandatory | | |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
