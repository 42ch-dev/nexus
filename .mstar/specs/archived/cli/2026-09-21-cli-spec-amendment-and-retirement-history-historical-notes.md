# Nexus CLI Spec — Amendment Ledger and Retired Surfaces Historical Notes

**Provenance**: frozen from [`specs/active/cli/cli-spec.md`](../../active/cli/cli-spec.md) on 2026-10-13 during the **v1.216** de-historicization pass.
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 completed the obsolete-host retirement on that date (the removed amendment log's own "V1.193 delivered (P2 complete 2026-09-21)" line); the removed blocks are the file's self-declared amendment ledger and retired-surface records. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Header amendment ledger (removed)

**Status (removed):** Normative — V1.51 Shipped (T-A P0/P1/P2 KB CLI amendments folded into Master)

**Amendment log — current surfaces** (shipped amendments whose command surfaces exist today; individual lines carry their own v1.193 retarget notes):
**V1.40 Shipped amendments:** §6.2G `nexus42 creator world create --title`/`list`/`show` (mandatory world binding; `--name` is alias); §6.x `nexus42 creator kb queue-extract --chapter N` sugar for novel profile (N ≥ 1).
**V1.41 Shipped amendments:** §6.2H `creator works` (list/status/use/pool); completion-lock + runtime lock (DF-60/61). Lineage via `--from-work` migrated to `creator works use` + `creator run <preset>` in V1.45 (`creator run` was removed in v1.193 P2 — §6.2D is historical).
**V1.51 Shipped amendments:** §6.2K `creator world kb adopt` LLM metadata surfaces; `creator kb rescan --work <work_ref>` cross-chapter reconciliation; `creator world kb pending --missing-only` (T-A P0/P1/P2).
**V1.52 Shipped amendments:** §6.2G.1 `creator world kb adopt --auto` + §6.2G.2 Legacy `creator kb --scope world` alias + deprecation for World KB CLI surface consolidation (closes R-V150KBED-01); both sections promoted to Normative (V1.158). **(The `--scope world` alias itself was deleted in v1.193 P2 — §6.2G.2 is now a historical record.)**
**V1.54 P0 Draft overlay:** §6.2M ACP host write-tool CLI mappings — 6 new mutation-capable `nexus.*` host tools map to `creator world kb edit/adopt`, `creator world configure`, `creator works cron set`, `creator findings resolve`, and `creator pool` entry management (DF-46). (Naming note: findings resolution lives under `creator works findings`, pool entry management under `creator works pool`; no `creator world configure` leaf is shipped.)
**V1.65 Prepare amendment:** outline and chapter-structure editing becomes UI-first through the bundled Web UI chapter-content Daemon API. CLI parity for existing creator/run/chapter workflows is retained; no shipped CLI command is removed or renamed by this UI-first slice. (The `creator run` side of that parity was later retired in v1.193 P2.)
**V1.175 P1 amendment:** §6.2G.3-6 — reading / fork / inspector leaves (groups 3, 5, 6), strategy patch leaves (group 1), outline/timeline/chapter leaves (group 2), KB entity patch + memory closure + findings triage (groups 4, 7, 8). Thin daemon-HTTP leaves over existing routes (AR-83); no daemon route changes. **Retargeted in v1.193 P2:** the leaves survive as direct-core calls — see the delivered notes in §6.2G.3–G.6.
**V1.182 P1 amendment:** §6.3B — hidden `nexus42 ops inspect [SESSION_ID] [--json]` operator group (BL-04): daemon-free read-only checkpoint projection with shared `resume_rules`; never triggers resume.
**V1.185 P0 amendment:** §6.2I — `nexus42 creator character edit|archive|restore` identity lifecycle (explicit revision CAS; thin daemon-HTTP leaves).
**V1.185 P1 amendment:** §6.2I — `nexus42 creator character binding show|edit` WorldSheet maintenance (binding revision CAS; thin daemon-HTTP leaves).
**V1.185 P2 amendment:** §6.2I.2 — `nexus42 creator character knowledge show|edit|remove` authored-content maintenance (knowledge revision CAS; thin daemon-HTTP leaves; `--summary`/`--summary-file` on add/edit).
**Retargeted in v1.193 P2 (V1.185 amendments above):** the Character identity/binding/knowledge leaves survive as direct-core calls (§6.2I–§6.2I.2); `creator character run` and `creator character soul reflect` lost their CLI entries (§6.2I.3).

**Amendment log — historical surfaces** (retired in v1.193 P2 — records only, not current setup steps):
**V1.35 shipped supplements:** [cli-command-ia.md](../cli/2026-06-07-cli-command-ia.md) (§6 IA rationale), [creator-centric-entry-model.md](../creator/2026-06-07-creator-centric-entry-model.md) (§7 entry paths)
**V1.44 Shipped amendments:** §6.2D `creator run audit-chapter` (DF-69): dual-mode review/extract, embedded `novel-manuscript-audit` preset, `--mode`/`--chapter`/`--volume`/`--json` flags; does NOT enter FL-E auto-chain driver (**the runner itself was removed in v1.193 P2**).
**V1.45 Shipped amendments:** §6.2D generic `creator run <preset_id>` — see [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (**Shipped Master, retained as version history**); legacy subcommand enum removed from clap surface (**the runner lost its CLI entry in v1.193 P2; the Master is the version history of that pre-retirement surface**).
**V1.46 Shipped amendment:** §6.2E FL-E stage subcommand block deleted (superseded by V1.45 generic preset runner — see changelog). Historical IA record: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (pre-v1.193 P2 surface).
**V1.64 P3 Draft overlay (retired in v1.193 P2):** §6.3 daemon Web UI serving — `daemon start` logged the Web UI URL; the `daemon ui`/`daemon web` convenience command and the §7.1 first-run step are deleted. See also [web-ui.md](../../active/surfaces/web-ui.md) §11 and [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) §4.4 (historical host spec — its crate was deleted in v1.193 P2).
**V1.185 P3 amendment (entry retired in v1.193 P2):** §6.2I.3 — `nexus42 creator character run --remember` + owner outcome observation (thin daemon-HTTP leaves).
**V1.189 P1 amendment (both cohorts retired in v1.193 P2):** §6.2G.7 — `basic-cli` / `legacy-cli` feature cohorts on the same `nexus42` binary/parser; `creator world kb graph` + `entity patch` called `nexus-core` directly (no daemon HTTP) in both cohorts. That direct path is now the ordinary `cli` cohort; the cohort selectors themselves are deleted.
**V1.193 delivered (P2 complete 2026-09-21):** the daemon group, `web-embed`, and the thin DaemonClient leaves are gone. Every `daemon` descendant (including schedule and ui/web), hidden `daemon-run`, Model A `mcp serve`, raw `host-call`, and the compatibility aliases were deleted with no replacement service launcher. Complete direct Rust-library operations, legitimate cloud transport and Connect are retained; the app cohorts are `cli` (default) and independent `connect-host`. **Every daemon/runner instruction below is a historical record of the old shipped system, not a current setup step.**

**Removed v1.193 cutover plan narration (reworded present-tense in the live header):** the original "The target removes `creator run`, …", "Retain real local …", and "Ordinary CLI uses the shipped `cli` default cohort …" paragraphs, plus the §0/§0.1/§0.2/§1/§4 "**Delivered (v1.193 P2):**"/"**Retired (v1.193 P2):**"/"**Historical:**" annotations (exact original wording preserved in git history at the freeze commit).

## §6.3 retired `nexus42 daemon` command group (removed body)

> **Historical record — retired in v1.193 P2.** This entire group is **deleted**: `nexus42 daemon …` and hidden `daemon-run` are unknown commands, and nothing below is a current setup step. Web UI is Electron-hosted, not `daemon ui`/`web`. Schedule control is not a CLI surface; the only retained scheduling surface is `creator works cron` declaration editing — the `creator run` runner was removed with the other incomplete execution entrances.

Retired command set (historical):

- `nexus42 daemon start|stop|restart|status|logs|doctor|ui|web` (**retired in v1.193 P2**)
- `nexus42 daemon schedule add|edit|remove|list|inspect|context|context-history|start|pause|resume|cancel|advance|timeline` (**retired in v1.193 P2**)

说明（历史 — 该组已删除）：

- 已删除的 daemon runtime 曾是本地 supervisor，不是 ACP Agent/Server。
- `daemon` 曾负责运行态控制，不承载 ACP 协议协商职责。
- **Was shipped:** `daemon schedule ...` was wired to the daemon orchestration schedules Daemon API (`/v1/daemon/orchestration/schedules/*`) via the deleted `commands/daemon/schedule.rs`.
- **Session control ownership (historical):** `daemon schedule ...` was the primary orchestration CLI surface. It exercised the full sessions control plane through schedule operations: `current_session_id` pointed at the active orchestration session, and schedule signals cascaded through the supervisor to the active session as described in [`creator-schedule-and-core-context.md`](../../active/orchestration/creator-schedule-and-core-context.md) §3.3.
- **Still removed:** `daemon orchestrate ...` was never a shipped compatibility surface. Do not document `daemon orchestrate run` in new plans or runbooks; the whole group is deleted, so no daemon orchestration CLI control surface exists.

**V1.56 P1 amendment (retired with the group in v1.193 P2):** `daemon start` and `daemon restart` gained an optional `--cdn-url <url>` flag:

| Flag | Purpose |
| --- | --- |
| `nexus42 daemon start --cdn-url <url>` | When set, `nexus.registry.refresh` fetches the ACP registry from the given CDN URL (configurable 10s timeout, 3 retries with exponential backoff). When absent (default), returns synthetic output from an embedded snapshot — zero network calls, sandbox / air-gap compatible. |
| `nexus42 daemon restart --cdn-url <url>` | Passes the CDN URL through daemon restart to the new daemon process. |

The flag was passed to the hidden internal `__internal daemon-run` command (same flag name; that command is deleted). Timeout and retry counts were not individually configurable via CLI flags (post-V1.56 concern).

**V1.56 P1 fix-wave amendment — `--cdn-url` security contract:**

| Invariant | Enforcement | Rejection class |
| --- | --- | --- |
| HTTPS-only scheme | CLI parse + `validate_cdn_url_static` at daemon boot | `CdnError::InsecureScheme`; exit code `E_CDN_URL_INVALID` |
| No open redirects | `reqwest::redirect::Policy::limited(0)` | `CdnError::TooManyRedirects` |
| Private-IP / loopback / link-local / metadata block | CLI parse (DNS resolution) + runtime `is_blocked_ip` | `CdnError::BlockedHost` |
| Body size cap (8 MiB) | Streaming read with byte counter | `CdnError::BodyTooLarge` |
| Empty / whitespace URL | CLI parse + `validate_cdn_url_static` | `CdnError::EmptyUrl` / `CdnError::UrlParse` |

Acceptable examples: `https://registry.cdn.example.com/v1/registry.json`.
Rejected examples: `http://...` (insecure scheme); `https://localhost:8443/...` (loopback); `https://10.0.0.5/...` (private IP); `https://169.254.169.254/...` (cloud metadata); `https://...` with N>0 redirects; empty / whitespace.

These rejections happened **at daemon start**, not per-invocation — once the daemon booted with a `--cdn-url`, that URL was locked. Reconfiguration required daemon restart. This ensured sandbox/air-gap environments were not silently compromised by an attacker modifying a flag at runtime.

**V1.64 P3 amendment (retired in v1.193 P2) — Web UI serving and CLI entry:**

`nexus42 daemon start` served the bundled local Web UI SPA at the server root (`http://localhost:<port>/`) from embedded assets (`rust-embed`). On startup the daemon logged both the Daemon API base URL and the Web UI URL (historical example; the command is deleted):

```
$ nexus42 daemon start
✓ Daemon started successfully on port 8420
  PID: 12345
  Daemon API: http://127.0.0.1:8420
  Web UI:    http://127.0.0.1:8420/
```

A convenience subcommand `nexus42 daemon ui` (alias `nexus42 daemon web`) started the daemon in background mode if it was not already running, then opened the Web UI in the OS default browser (`open`/`xdg-open`/`start`). Both entries are deleted.

| Retired command | Purpose it had |
| --- | --- |
| `nexus42 daemon ui` | Start daemon (if needed) + open browser to `http://127.0.0.1:<port>/` |
| `nexus42 daemon ui --port <N>` | Use a specific port (default: 8420) |
| `nexus42 daemon web` | Alias for `nexus42 daemon ui` |

The static SPA shell (HTML/JS/CSS) was unauthenticated — it carried no data. All data flowed through the loopback Daemon API (`/v1/daemon/*`), which remained keyless on `localhost` per the V1.20 model; the Electron/TS host serves the SPA today. See [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) §4.4 (historical host spec) and [web-ui.md](../../active/surfaces/web-ui.md) §4 for the historical serving model.

**V1.65 authoring note:** chapter outline and structure editing is exposed first through the daemon-served Web UI (`/v1/daemon/works/{work_id}/chapters/*`). This does not remove CLI parity for the then-existing `creator run`, Work status, reconcile, or chapter-oriented orchestration flows. The CLI remains the power-user and automation surface; the Web UI becomes the primary author-facing surface for outline/structure planning. Body full-text editing and native `Open with` actions are deferred to the V1.66 native desktop shell/body-editor design.

The `daemon` command group **used to include** (all deleted in v1.193 P2):

- `nexus42 daemon start|stop|restart|status|logs|doctor|ui|web`
- `nexus42 daemon schedule add|edit|remove|list|inspect|context|context-history|start|pause|resume|cancel|advance|timeline`

## §6.2D `creator run` retired runner body (removed)

> **Historical record — removed in v1.193 P2.** The CLI no longer exposes any `creator run` entry (the `Run` leaf is gone from the `creator` clap group); the underlying preset/execution libraries remain. Nothing below is a current setup step.
>
> **Authoritative surface (historical)**: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (Shipped Master, V1.45). The detail below is kept for cli-spec continuity; on any divergence the Master wins.

**V1.45 rewrite:** The bespoke subcommand dispatch (`start`, `continue`, `stage`, `resume`, `reconcile-chapters`, `audit-chapter`, `review-master`) is replaced by a single generic entry point:

```
nexus42 creator run <PRESET_ID> [<WORK_ID>] [global flags] [preset args...]
```

| Command | Purpose |
| --- | --- |
| `nexus42 creator run <preset_id> [<work_id>]` (retired in v1.193 P2) | Generic preset dispatch. FL-E stage-advance presets (`research`, `novel-writing`, `novel-chapter-review`, `kb-extract`) were routed to `stage_advance`; all other presets were scheduled directly via the deleted Daemon API. `<work_id>` was optional — it defaulted to pool `active` Work. |

**Global flags:** `--json`; `--force-gates --reason "<text>"` (bypass preset admission gates, audited; `--reason` required); trailing args (`--chapter N`, `--volume N`, etc.) parsed against `preset.cli_args` declarations (V1.45 §3.3), global flags first.

**Preset-specific CLI args (declared in `preset.yaml`):** `novel-manuscript-audit-review` / `novel-manuscript-audit-extract` — `--chapter` (integer, required), `--volume` (integer, optional, default 1); `novel-review-master` — `--finding-id` (string, optional), `--auto-schedule` (boolean, optional, default false).

Rules: only presets declaring `run_intents` including `work_init` may be used as the **first** run on a new Work; `creator run` created/updated schedules via the Daemon API and did **not** replace `daemon schedule` for power users; omitted `<work_id>` resolved the work-pool `active` row else failed with remediation to `creator works use`; FL-E presets were identified via `stage_for_preset()` reverse mapping and the runner called `stage_advance` with `force: false`.

**V1.45 shipped:** Generic `RunCommand` struct replaces enum; `creator/mod.rs` uses `#[command(flatten)]` instead of `#[command(subcommand)]`. Legacy handler code preserved as `#[allow(dead_code)]` for P1/P2 migration. Old `start`/`continue`/`stage`/`resume`/`audit-chapter`/`review-master` subcommands are no longer exposed.

## §6.2E `creator run stage` supersession note (removed)

> **Removed in V1.45; the V1.45 replacement was removed in v1.193 P2.** The FL-E `creator run stage list` / `stage advance` subcommands were deleted from the clap surface and replaced by the generic **`creator run <preset_id>`** runner, which itself lost its CLI entry in v1.193 P2. Stage-gate validation and Work stage PATCH then happened inside the preset runner before enqueue. Authoritative IA (historical): [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) §4 (Execution flow). See changelog: V1.45 compass migration appendix.

## §6.2G.2 legacy `creator kb --scope world` deprecation-window record (removed body)

> **Retired.** The `creator kb --scope world` compatibility spelling is **gone**: `creator kb` serves the work-scope file index only, and World-scoped narrative KB lives on the canonical `creator world kb ...` surface (`apps/nexus42/src/commands/creator/kb.rs` module docs, §6.2F). Nothing below is a current instruction — it records the V1.52 deprecation window.

Historical behavior: `creator kb --scope world <subcmd>` was a **deprecated alias** for `creator world kb <subcmd>`.

| Legacy command (retired) | Canonical replacement |
| --- | --- |
| `nexus42 creator kb list --scope world --world-id <id>` | `nexus42 creator world kb list <id>` |
| `nexus42 creator kb show <entry_id> --scope world --world-id <id>` | `nexus42 creator world kb show <id> <entry_id>` |
| `nexus42 creator kb remove <entry_id> --scope world --world-id <id>` | `nexus42 creator world kb delete <id> <entry_id> --yes` |

Historical rules (all retired with the alias): each legacy invocation emitted a deprecation warning on stderr and via `tracing::warn!` ("planned removal V1.53"); `list`/`show`/`remove` (World scope) transparently forwarded to the canonical `world::kb` hermetic functions; `search` and `add` (World scope) had no canonical equivalents and operated inline; `remove` with World scope gated on world ownership (`WORLD_KB_FORBIDDEN` per entity-scope-model §5.5); the `--scope world` flag was preserved for backward compatibility during the V1.52 window and deleted in v1.193 P2 with the other compatibility spellings; work-scope operations were unaffected and remain the only scope.

## §6.2M ACP host write-tool CLI mappings record (removed section)

**Historical (V1.54; the daemon-level `CapabilityRegistry` and its `HostToolExecutor` were deleted in v1.193 P2):** V1.54 added 6 mutation-capable `nexus.*` host tools to the daemon-level `CapabilityRegistry`. They were ACP-facing write tools dispatched through the unified `HostToolExecutor::registry_dispatch()` path, not standalone CLI subcommands. The table records each host tool and the CLI surface it mapped to:

| Host tool (ACP) | CLI surface | Notes |
|---|---|---|
| `nexus.kb_snapshot.write` | `creator world kb edit` / `creator world kb adopt` | Write/upsert KnowledgeEntries into a World KB; admission gate chain: Allowlist → ActiveCreator → RequireWorldOwnership → PermissionPolicy → AuditLog |
| `nexus.world.configure` | `creator world configure` (future) | Update world metadata (title, visibility, time_policy) |
| `nexus.manuscript.chapter.update` | (ACP-only; no standalone CLI yet) | Write chapter body content + metadata for a Work; work-scoped admission |
| `nexus.work.schedule.set` | `creator works cron set` | Link/unlink schedule ids to a Work row |
| `nexus.finding.resolve` | `creator findings resolve` | Resolve/close a finding entry via findings DAO |
| `nexus.pool.entry.manage` | `creator works pool` | Add/remove/promote entries in the selection pool |

All write tools route through the same admission pipeline (`Allowlist → ActiveCreator → WorkspaceBounds → PermissionPolicy → AuditLog`) and use `&'static [AdmissionGate]` slices from the `LazyLock<CapabilityRegistry>` singleton.

## §6.3A retired preset spellings (removed body)

**Historical (removed in v1.193 P2): `nexus42 system preset`.** The `system preset` alias was removed with the compatibility spellings. The retired spellings were:

| Retired command | Purpose it had |
| --- | --- |
| `nexus42 system preset list` | Listed embedded + user + system presets with `run_intents` (V1.33 expanded beyond `_system.*` only). |
| `nexus42 system preset validate <path> [--offline]` | Validated a preset bundle through the shared orchestration facade (V1.33). `--offline` (V1.153 P3) ran the same checks in-process via the validator core; with `--offline` absent it was daemon-backed (`POST /v1/daemon/presets:validate`) — `nexus-runtime` did not serve the daemon router. Invalid presets exited non-zero. |

**Power-user orchestration (retired in v1.193 P2):** `nexus42 daemon schedule add --preset <id> --creator <id> [--seed "..."]` — started preset-driven workflows through schedules; the whole `daemon` group is deleted.

**Daemon API (retired in v1.193 P2, historical):** the daemon group and its `/v1/daemon/*` routes were deleted with the `nexus-daemon-runtime` crate: `GET /v1/daemon/presets`, `POST /v1/daemon/presets`, `POST /v1/daemon/presets:validate`, `POST /v1/daemon/presets/{id}:reload`.

**Historical (V1.33–v1.192; inverted in v1.193 P2):** an earlier revision of this section stated there was **no** top-level `nexus42 preset ...` command group, that the user creative entry was **`creator run`** (V1.33), and that validation/listing was **`system preset`**. Neither `creator run` nor `system preset` survives in v1.193 P2; the top-level `preset` group is the retained canonical surface.

## §6.2G.7 cohort history (removed narration)

`nexus42` gained a **daemon-free basic CLI cohort** built with `cargo build -p nexus42 --bin nexus42 --no-default-features --features basic-cli` (same executable and the same clap declarations — no second binary, no second parser, no alternate flags). **v1.193 P2 removed those selectors:** the direct `nexus-core` path it pioneered is now the ordinary `cli` cohort. Historical feature table:

| Feature | Meaning |
| --- | --- |
| `basic-cli = []` | Daemon-free cohort: World KB `graph` + `entity patch` only. No daemon, no Axum, no orchestration, no WASM host, no libp2p in the dependency tree. **Deleted in v1.193 P2** — the ordinary `cli` cohort covers this slice |
| `legacy-cli` | **Deleted in v1.193 P2** — it was every other command (daemon-mediated or local SQLite); retained leaves moved to the ordinary `cli` cohort and the rest lost their CLI entries |
| `default = ["legacy-cli", "web-embed"]` | The ordinary product until v1.193 P2, which deleted both from the supported default (`default = ["cli"]` today) |
| `web-embed`, `connect-host`, `connect-client`, `embedded-mcp` | Until v1.193 P2 each implied `legacy-cli` (M1). The app `web-embed`, `connect-client` and `embedded-mcp` selectors are deleted; `connect-host` is independent of the ordinary `cli` cohort and keeps its explicit scoped compute/WASM edge; core `connect-client` / `embedded-mcp` library features remain |

**Scope of the (deleted) basic-cli cohort.** Only `creator world kb {graph, entity patch}` was exposed in that cohort. Every other command — including the retained basic reads `creator works list|status|use` — was daemon-mediated and compiled only under `legacy-cli`.

## §6.2I.3 retired observation contract (removed body)

> **Historical record — removed in v1.193 P2.** `creator character run` and `creator character soul reflect` are gone from the clap surface together with the daemon transport they needed; the CLI exposes no run/observation entrance for Character sessions. Core host capture, session and soul-synthesis behavior remain library/core operations. Nothing below is a current setup step. Retained Character surface: `create|list|show|edit|archive|restore`, `binding`, `knowledge`, `memory`, `tom` (see §6.2I–§6.2I.2).

Thin daemon-HTTP leaves over generated Agent Host session/operation DTOs plus the owner-only `CharacterOperationResult` outcome surface. Run output observation used the session SSE stream; capture observation used `GET /v1/daemon/agent-host/operations/:operation_id` polled concurrently (100ms) with a ≤30s grace window after the first terminal observation. The CLI never reran provider execution for capture or outcome recovery.

| Flag / output | Purpose |
| --- | --- |
| `--remember` (default **false**) | Opt into explicit run-to-memory capture after a successful `end_turn`. Maps to `ExecuteOperationRequest.Prompt.remember`. |
| `--json` | Emits `session`, `operation`, `result`, `events`, and when available `outcome` (`CharacterOperationResult`). Adds `output_observation: incomplete` and/or `capture_outcome_observation: unavailable` when grace expires without the peer observation. |
| Human output | Prints `run_status` and `capture_status` / `capture_pending_id` / `capture_code` on separate lines from `result`. |

Rules (removed): run-vs-capture exit codes (opt-out succeeded runs exit 0 when SSE terminal observation complete; `remember` + `captured` exits 0 only when both the matching SSE `end_turn` terminal and the terminal GET outcome are observed; missed SSE terminal with successful GET outcome exits nonzero preserving streamed `result`/`outcome`; terminal `skipped`/`failed` capture or grace-expiry exits nonzero preserving streamed `result`); SSE terminal handling ignores foreign `session_id`/`op_id` pairs; `outcome.capture` from the GET route is authoritative for terminal capture status; the CLI accepts no digests, `source_operation_id`, or capture bodies — daemon drain + storage own capture content.

## §V1.45 supersession section (removed)

**Superseded by**: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (Shipped Master V1.45). The §6.2D/E `creator run` preset-entry table, FL-E stage advance mapping, preset-id examples, and global flags on `creator run` are now part of the canonical Master body.

> **Note on §6.2D/E body**: The §6.2D/E body now defers to the Shipped Master [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (V1.45) for the pre-retirement canonical `creator run` surface — see the authoritative-surface pointer at the top of §6.2D and the supersession note in §6.2E. The V1.33–V1.44 bespoke subcommand table was replaced by the generic dispatch entry in commit `4aa5aa53` (V1.45 P-last); the stale `creator run stage` section was deleted in V1.46 P1. Closes residual `R-V145B3-001`.
>
> **v1.193 P2:** `creator run` and `creator run stage` have no CLI entry — the §6.2D/E bodies are historical records of the removed runner, and the V1.45 promotion above is the version history of that pre-retirement surface.

## §V1.57 P1 overlay: `host-call` (removed section)

**Status**: Historical — the `host-call` CLI entry was removed in v1.193 P2 (was Draft, V1.57 P1).

> **Historical record — removed in v1.193 P2.** `nexus42 host-call` has no CLI entry: the raw host-call leaf was deleted with the daemon group and no replacement service launcher exists. The TS tool-execution route and `execution::peer_tools` remain. Nothing below is a current setup step.

A debug-only, low-level CLI entry point that sent a raw host tool execution request through the daemon's `CapabilityRegistry::dispatch` path. Admission gates (allowlist, active creator, workspace bounds, permissions.toml, audit) applied identically as for HTTP and worker caller paths. Fields: `tool_id` (positional, e.g. `nexus.context.whoami`, `nexus.work.get`), `--args`/`-a` (JSON string, default `{}`). Exit codes: `0` success, `1` admission denied (NOT_SUPPORTED, FORBIDDEN, POLICY_BLOCKED), `2` tool error/internal failure. Debug-only intent: bypassed normal CLI UX layers for ad-hoc developer testing; CLI `--help` documented this. Wiring: `nexus42 host-call` → `DaemonClient::post` → daemon `POST /v1/daemon/agent-host/internal/tool-executions` → `HostToolExecutor::execute()` → `admission_pipeline()` → `CapabilityRegistry::dispatch()` → tool handler. Per Q4 (compass §0), exactly one `host-call` entry existed — not per-tool subcommands; all 20 registered host tools were callable through it.

## §V1.58 P3 overlay: `reference refresh` (removed section)

**Status**: Historical — the `creator reference refresh` CLI entry was removed in v1.193 P2 (was Draft, V1.58 P3): the daemon host-call endpoint it dispatched through is deleted, and the retained `creator reference` surface is `register|list|show` (`apps/nexus42/src/commands/creator/reference.rs`).

`nexus42 creator reference refresh [ref_id|all] [--dry-run]` refreshed one or all non-offline reference source bodies by dispatching `nexus.reference.refresh` through the daemon's host-call endpoint. The CLI resolved the active creator context and opened the workspace state DB; the daemon fetched each source URL, compared the content hash, and updated the DB row (`last_refreshed_at`, `content_hash`, `refresh_status`); with creator context the fetched body was written atomically to on-disk `body.md` via `nexus_home_layout` (V1.58 P3, `<target>.tmp` then rename, matching the V1.55 P3 `ScaffoldTransaction` pattern). `all` listed every non-offline source; `<ref_id>` resolved a single source and errored on `refresh_policy = 'offline'`. `--dry-run` printed `[DRY RUN] Would refresh N reference source(s):` with no daemon call. Exit codes: `0` dispatched; `1` usage/not-found/offline-policy; `1` daemon not reachable. IPC: `DaemonClient` default 30 s timeout; daemon-side 30 s HTTP fetch timeout and 100 MiB body cap. `--policy` was not exposed — the refresh policy was set at registration. Wiring: `DaemonClient::post` → `POST /v1/daemon/agent-host/internal/tool-executions` → `HostToolExecutor::execute()` → `CapabilityRegistry::dispatch("nexus.reference.refresh", ...)` → `ReferenceRefresh::run()`.

## §6.2C retired workspace leaves table (removed)

| Retired command | Purpose it had |
| --- | --- |
| `nexus42 creator workspace clone <world-ref>` | Hidden hard-deprecated World clone — World cloning was platform-only, so the leaf never had a local operation. |
| `nexus42 creator workspace link` / `unlink` | 绑定/解绑本地项目与平台 World — a callable-but-incomplete leaf in the pre-v1.193 overlay that lost its CLI entry in v1.193 P2 ([rust-core-service-boundary.md](../architecture/2026-09-21-rust-core-service-boundary-cutover-record-historical-notes.md) §7.4) |
| `nexus42 creator workspace status` | 当前 workspace 总览 — a callable-but-incomplete leaf in the pre-v1.193 overlay that lost its CLI entry in v1.193 P2 |

## Annotation drops (reworded in place; originals in git history)

- §0.1/§0.2/§1/§4: "**Retired (v1.193 P2):** …已删除", "**Historical:** daemon runtime 曾是…", "**Delivered (v1.193 P2):** …" prefixes reworded present-tense.
- §6 preamble: "Command-surface lock (V1.35, superseded in v1.193 P2)" framing and the "> **Legacy (V1.16–V1.34)**：六组含独立 `sync`" note.
- §6.0B: "**V1.35 SSOT（历史记录）**" pointer; "**v1.193 target:**"; "**Retired (v1.193 P2)**"; "Sync 迁移（V1.35 target）"; "**Delivered (v1.193 P2)：**顶层 alias 已删除…**Historical:** 该 alias 曾转发至 `platform sync`（V1.35 deprecation）".
- §6.2B pair/unpair rows "（v1.212 D6）"; §6.2B.1/.2/.3 heading prefixes "V1.176 P0 T2/T3 amendment —", "V1.176 P0 QC fix-wave amendment —".
- §6.2D knowledge-assets: "**V1.40**:" tag, "**V1.29 additions**", "**V1.40 P2 note (surface updated to the delivered form)**", `--scope work`/`--scope world` provenance clauses ("V1.23 保留，V1.24 KCA-003 C2 强化", "historical V1.27+ shipped", "原 daemon local API 优先路径…已随 daemon 删除"), "**V1.23 最小落地**" and "推荐别名 / 迁移方向…**Delivered:**" labels.
- §6.2F: heading "V1.23"; table parentheticals "(the `--scope` flag was deleted in v1.193 P2)", "(the daemon local API half was deleted in v1.193 P2)", "(the historical `creator kb --scope world` alias was deleted in v1.193 P2 — §6.2G.2)", "(the historical `--name` alias was deleted in v1.193 P2)", "**V1.40 P0**", "**Deferred (V1.26)**", "**Shipped (local, V1.26+)**", "(V1.27+)"; "Implementation task C4 …" sentence.
- §6.2G: "(V1.40 — DF-63 P0)" heading tag; "The historical `--name` alias … was **deleted in v1.193 P2**"; "`--kind` deferred to P1"; "V1.40 Work creation/init"; "enforced by `creator bootstrap` (V1.45; the command was removed…)"; empty "**Target (V1.40 P0/V1.51 T-A P0/P1/P2):** ." placeholders; "V1.51 T-A P0/P1/P2 amendment —" prefixes; "(V1.51; see llm-extract.md)"; "The V1.50 chapter-scoped … (T-B P2) is extended"; "V1.50 chapter-scoped rescan (unchanged)"; "V1.51 work-scoped rescan"; "§6.2G V1.40 rules"; "already enforced by the V1.50 P1 migration"; "and the `creator run` runner removed in v1.193 P2"; "unchanged from V1.50"; "(V1.50 behavior)".
- §6.2G.1: "(V1.52 T-A P0 — Normative)" heading tag; "V1.50/V1.51 behavior"; "V1.52 T-A P0"; "no new aliases or deprecations are introduced in V1.52".
- §6.2G.3–G.6: "V1.175 P1 amendment —" heading prefixes and "These were shipped in V1.175 P1 as thin daemon-HTTP leaves over the then-existing … routes … The daemon group and its `DaemonClient` were deleted in v1.193 P2 and each retained leaf now issues…" preambles; "(V1.89 …)", "(V1.162 P1 T2)", "(V1.162 carrier B)" ×2, "V1.151 inspector packet", "the V1.79 reading surface stays web", "the deleted HTTP transport rendered", "the V1.72 canvas outline+timeline routes", "the V1.65 chapter-**content**", "(V1.165)", "pioneered that direct path in V1.189", "is a NEW verb".
- World structured-rule lifecycle: "(v1.198 normative amendment)" heading tag; "(user decision, 2026-09-27)"; "This amendment specifies".
- §6.2G.7: "V1.189 P1 amendment — basic-cli cohort … (Normative; cohort selectors deleted in v1.193 P2)" heading + "Transport change (supersedes …)" + "the V1.175 statement … no longer holds" + "Scope of the (deleted) basic-cli cohort" narrations (superseded by the compact live section).
- §6.2I/I.1/I.2: "V1.185 P0/P1/P2 amendment —" heading prefixes and "V1.185 … shipped them as thin daemon-HTTP leaves … the daemon group and its `DaemonClient` were deleted …" preambles.
- §6.2H: "(V1.41 Draft — DF-60/61)" heading tag; "it complemented **`creator run`** … (the runner was removed in v1.193 P2 …)"; "(**migrated from** `creator run list|status`)" ×2; "(v1.194 P2-T1 — …, settled)" marker; "(novel + auto-chain — migrated from V1.39 `creator run status`)"; the `daemon` status row "(retired in v1.193 P2) … the field is no longer emitted"; "(V1.39 P1+)"/"(P4)"; "**OUT V1.41:** … (grill-me 2026-06-10)"; "Entry path pointer (no standalone quickstart in V1.41; historical entry-model record)"; empty "**Target (V1.41):** .".
- §6.3B: "(V1.182 P1 — BL-04)" heading tag; "**Shipped:**" label; "the V1.35 visible-group lock in §6.0B forces hiding"; "the V1.180 checkpoint slice"; "the former daemon-boot re-drive is gone with that deleted host … its crate was deleted in v1.193 P2".
- §6.4: "intentionally removed in V1.53 (DF-50 Cancelled …)" note + the "> **V1.53 intentional breaking-change removal** …" blockquote (was removed in V1.53 because … `skills-export-compatibility.md` was retired in V1.53 P-1).
- §6.5/§6.6/§6.7: "**顶层 alias 已在 v1.193 P2 删除**" annotations ×5; "> **Delivered (v1.193 P2):** there is no top-level `sync` group …" reworded; "曾提供的顶层拼写（历史）：`nexus42 sync pull|push|status|retry|resolve`"; "历史拆分记录"; "> **Breaking change (pre-release):** `assemble-local` is **removed** … was superseded by …"; "**Deferred** in V1.26"; "(shipped V1.26, hardened V1.27+)"; "(V1.27+)"; "**Shipped (local)**"; "(**retired in v1.193 P2** — the whole group is deleted)"; "Daemon context-assemble Daemon API is **Retired** (KCA-002 B2)"; "`assemble-local` is **removed** in pre-release".
- §7: "V1.35 将首次使用拆为 … V1.35 历史记录：creator-centric-entry-model.md"; step 6/7 "**Delivered/Removed in (v1.193 P2):**" tags; "**Historical (V1.64; retired in v1.193 P2):** after step 6 the Web UI used to be available …" note; "（v1.193 P2 已删除该命令）"; "（canonical；legacy `nexus42 sync pull` alias 已于 v1.193 P2 删除 — §6.5）"; "> **V1.43 Implemented (P1):** … daemon-not-reachable … `creator run` / `creator works`".
- §8.2: "**Historical (daemon deleted in v1.193 P2):** daemon 使用 token 时以内存持有为主…" bullet.
- §9.2/§10: "**Retired in v1.193 P2:** this mode is not a CLI product surface…" and the §10.1–§10.3 daemon state machine / boot flow / degradation bodies ("`nexus42 daemon status`（已删除）曾展示：…").
- §11.0: "（daemon 已于 v1.193 P2 删除）".
- §12.1: "（V1.36 normative）", tree comments "可选 V1.36"/"V1.36 空模板" ×2, "`work-status.md` 文件在 V1.36 **已移除**", "子树在 V1.36 **已移除**", "**已废弃（pre-1.0 移除）**", "历史兼容叙述", "（V1.52+ essay, V1.54+ game-bible）…曾支持以下值（**v1.193 P2：… 下表为历史 version record**）", the 状态 column ("Shipped V1.36/V1.52/V1.54"), "V1.54 game-bible 布局…`--profile game_bible` 自动选择…V1.54 不自动链式推进".
- §12.1B/§12.2: "（Electron/TS 持有；原 daemon runtime 已在 v1.193 P2 删除）"; "已废弃的扁平".
- §14.3/§15.2: "（顶层 `nexus42 sync` alias 已删除 — §6.5）" ×2; "~~`nexus42 sync retry`~~（随顶层 `sync` 组一起删除…）"; "~~`nexus42 daemon restart`~~ (deleted in v1.193 P2 …)".
- §17: "~~Nexus local API …~~ **Closed (v1.193 P2):**"; "~~workspace 是否支持多 world 共存~~ **Closed（C2）**" strikethrough forms.
- §11.5: "历史布局 **`References/<creator_ref>/`** 仅作为兼容叙述，**不再**由 `init` 默认创建".
