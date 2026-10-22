# Web Access Model — Shipped

| Attribute | Value |
| --- | --- |
| **Status** | **Shipped** — iteration v1.219-web-rebuild (2026-10-22), plans `v1219-shell-and-access` T1/T4 + `v1219-onboarding` |
| **Scope** | Browser/dev entry: plain HTTP (local + LAN), tokenized startup URL → session cookie, derived first-run gate, 401 recovery, workspace family exposure + in-process promotion |
| **Owners** | `apps/nexus-service` (transport/auth), `crates/nexus-core-node` (admission/promotion), `apps/web` (gate/recovery UI) |
| **Related** | [web-rebuild-ia.md](../../proposed/surfaces/web-rebuild-ia.md) (IA), I-000059/I-000060/I-000061/I-000062 (entry gaps), I-000063 (workspace create deferred) |

## 1. Entry and transport

- **Plain HTTP** is the dev/default transport, local and LAN. TLS remains opt-in (`--tls-cert`/`--tls-key`); non-loopback binds still require the explicit remote flag.
- On startup the service prints `[nexus-service] open: http://<host>:<port>/__bootstrap?token=…` and auto-opens it unless `--no-open` (Electron passes `--no-open`). The `.dev` log is `chmod 700` and the token line is scrubbed after read.
- `GET /__bootstrap?token=…` is handled **before** route matching: validate + consume the token, set `nexus_session` (`HttpOnly; SameSite=Lax; Path=/; Max-Age=28800`; `Secure` only under TLS), `302` to `/`. Replay and expiry return distinct single-instance 401 bodies; the token is never rendered into the SPA.

## 2. Auth precedence

`X-API-Key` (non-empty) wins; otherwise a valid `nexus_session` cookie; keyless loopback unchanged. Origin admission accepts the request's own Host-derived origin in addition to the allowlist (served SPA on a LAN bind), while foreign/mismatched/opaque origins still 403.

## 3. First-run gate (derived, no client marker)

The browser gate reads the unguarded `GET /v1/daemon/runtime/status`: `workspace_initialized === false` ⇒ `/setup`, true ⇒ shell. There is **no per-browser marker**; `setCompleted(true)` triggers a re-probe. Desktop keeps the `setup_completed` IPC branch. A probe failure is not silently read as completed; the degraded surface explains the state and offers a retry. The wizard stays directly reachable for re-run (`设置 → 重新运行设置向导`), which neither clears a marker nor deletes data.

## 4. Workspace family and promotion

- Exposed (frozen contracts, no new schema): `GET /v1/daemon/workspaces`, `GET /v1/daemon/workspace`, `POST /v1/daemon/workspace/active`. `POST /v1/daemon/workspaces` (create) remains unexposed — I-000063.
- A fresh home opens service-only; creator/workspace home control is admitted via `CoreHomeService::open(user_home)`, while full-domain operations stay denied.
- A successful first selection promotes the process in-process (same publication sequence as startup) and only then sets `workspace_initialized`. Selection + promotion are serialized per service so concurrent requests cannot double-open the core.
- The bootstrap-precreated `default` workspace is materialized on first select (`meta.json` + `<creative_root>/.nexus42/workspace.json`, display name `默认工作区`); `creative_root` derives server-side (legacy `workspace_path_by_creator` → `workspace_path` → `~/Nexus`). No user-facing copy claims an absolute path.

## 5. Failure surfaces

- Keyed 401 → one global recovery surface (title `需要访问授权`) with two channels: a terminal instruction to open a fresh tokenized URL, and an inline API-key entry (`输入 API Key…` / `重新检测`). Concurrent 401s collapse to one instance; zero toast storms. A successful cookie-only probe clears a stale stored key so the session wins.
- Degraded service → one instance (wizard full-screen, shell top banner): `Nexus 服务暂不可用：{reason}。请确认本地服务正在运行后重试。` + `重试`; transport failures map to `无法连接本地服务`; stale probe responses are discarded by a monotonic generation guard. A fresh, not-yet-initialized home is **not** reported as degraded.
- Workspace selection failure and gate re-probe failure report separately (`无法选择默认工作区` vs `状态刷新失败`).

## 6. Deferred (tracked)

- Workspace creation (route + core create + re-enable the `＋ 新建工作区` chip) — I-000063.
- Service-owned agent-profile persistence for browser clients (v1.219 uses `nexus-agent-profile:<creator_id>` in localStorage) — I-000064.
- Electron `ensure_setup_bootstrap` IPC has no renderer consumer; retire or document it in the desktop contract.
