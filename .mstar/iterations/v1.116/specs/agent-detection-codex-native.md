# Agent Detection Fix + codex-native Provider (V1.116 P0)

> Iteration-scoped product/tech brief for V1.116 P0. Not a normative
> `{SPECS_DIR}` Master. Architect (seat 2) refines interfaces and protocol
> decisions.

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-13-v1.116-agent-detection-codex-native` |
| **Tier** | Must |
| **Audience** | Authors (first-launch Setup honesty) + maintainers (detection + provider wiring) |
| **primary plan** | `.mstar/plans/2026-07-13-v1.116-agent-detection-codex-native.md` |

## Problem framing

The Setup page is the **first product surface** every new author sees. Today it
**lies about installed agents**: codex, cursor, opencode, and kimi commonly
show as "not installed" even when their CLIs are on PATH. That destroys trust
before the author ever reaches Canvas or Compute.

Root causes (product-relevant, verified 2026-07-13):

1. **ACP registry agents are invisible** — registry binary cmds are relative
   paths (`./kimi`, `./opencode`, `./dist-package/cursor-agent`). Probing those
   strings looks in CWD, not PATH → false "not installed."
2. **codex has no honest product path** — registry `codex-acp` has no binary
   distribution; authors who installed the **codex CLI** never see it. Product
   decision (locked): codex is a **native CLI provider** (like `claude-native`),
   not an ACP-preferred path.
3. **Native CLI discovery is incomplete in the Setup scan** — `KNOWN_COMMANDS`
   only maps `claude` → `claude-native`; even discovered native providers are
   not merged into `/v1/daemon/agent-host/scan`, so the Setup page never lists
   them.
4. **Verify control copy is noisy** — button label should be a single verb:
   "Verify" / "验证".

V1.110 improved PATH enrichment (nvm/volta/etc.). This plan is the **remaining
detection honesty gap**: bare-command extraction + codex-native + scan merge +
copy.

## User value

| Who | Why they care |
| --- | --- |
| **Authors (first launch)** | Installed agents show as installed; Setup is trustworthy; they can select a real provider and continue. |
| **Authors who use codex** | codex is a first-class native provider (detectable, selectable, session can start and accept a prompt) — not a ghost entry or ACP-only dead end. |
| **Maintainers** | One honest scan surface for ACP registry agents + native CLI providers; tests cover the false-negative class that bit Setup. |

## Goals

1. Detect **supported ACP-registry agents** (cursor, kimi, opencode) when their
   bare CLI is on PATH (bare-cmd extraction from registry relative paths).
2. Detect and register **codex** as `codex-native` (native CLI provider).
3. Expose native CLI providers in the Setup scan response (additive merge).
4. Keep detection honest for the supported set: no systematic false negatives
   for agents that are on PATH and in the supported inventory.
5. Shorten Verify button copy to a single verb (en + zh-CN).

## Non-goals

- Native CLI providers for cursor / kimi / opencode (they stay ACP-registry
  agents with existing ACP adapters)
- Full ACP protocol path as the preferred codex experience (codex-native is
  preferred; community `codex-acp` may remain listed but is not the product
  path)
- codex streaming / structured tool calls beyond `native_cli_limited()` parity
  with claude-native
- Expanding PATH enrichment further (homebrew/asdf/mise/nvm already handled
  elsewhere; out of this plan)
- Reworking the entire Setup wizard IA beyond detection honesty + Verify copy

## Supported agent inventory (product)

| Agent (author-facing) | Detection path | Provider path after V1.116 |
| --- | --- | --- |
| Claude | Native CLI (`claude` → `claude-native`) | Existing native; must appear in scan merge |
| Codex | Native CLI (`codex` → `codex-native`) | **New** native adapter |
| Cursor | ACP registry (bare cmd from relative path) | ACP registry agent |
| Kimi | ACP registry (bare cmd from relative path) | ACP registry agent |
| OpenCode | ACP registry (bare cmd from relative path) | ACP registry agent |

**Product DoD** is honesty for this inventory — not "every binary on PATH."

## Target state

- An author with any of the supported CLIs on PATH sees them as **installed**
  on Setup without custom launch hacks.
- codex is selectable as a native provider; a session can start and accept a
  prompt (full provider adapter, locked grill-me decision).
- Scan response is additive: ACP entries unchanged; native CLI entries appended
  (`registry_agent_id: null` / `None` as the product signal for "native").
- Verify is a single-verb control in en and zh-CN.

## Acceptance criteria (author/maintainer-observable)

| ID | Criterion | How to verify (no code read required) |
| --- | --- | --- |
| **AC-P0-1** | cursor, kimi, opencode show **installed** when their CLI is on PATH | Install/have CLI on PATH → open Setup → status is installed (not "not installed") |
| **AC-P0-2** | codex shows **installed**, is **selectable**, and a session can start + accept a prompt via `codex-native` | PATH has `codex` → Setup shows installed → select codex → start session → send a prompt successfully |
| **AC-P0-3** | For the **supported inventory**, no systematic false negatives when the CLI is on PATH; absent CLIs still show not-installed | Toggle PATH presence for each supported agent; UI matches reality |
| **AC-P0-4** | Verify button label is "Verify" (en) / "验证" (zh-CN) | Visual check on Setup in both locales |

## Product decisions (locked)

| Decision | Choice | Rationale |
| --- | --- | --- |
| codex path | Full native CLI provider adapter (not ACP-preferred) | Grill-me #3; matches claude-native pattern; registry binary absent |
| Detection fix | Bare-cmd extraction for ACP registry relative paths | Fixes the lie for cursor/kimi/opencode without inventing native providers |
| Scan surface | Merge native CLI into existing Setup scan (additive) | One Setup story; authors should not need a second discovery path |
| Copy | Single-verb Verify | First-impression polish; low cost |
| False-negative scope | Supported inventory only | Avoid unbounded "every binary on PATH" DoD |

## Architect decisions (seat 2 — resolved)

### AD-1: codex CLI protocol (studied from `codex exec --help` v0.144.1)

codex CLI is **subcommand-based**, not flag-based like claude. The non-interactive
surface is `codex exec`:

| Capability | claude-native (existing) | codex-native (new) |
| --- | --- | --- |
| Non-interactive invocation | `claude --print [PROMPT]` | `codex exec [PROMPT]` |
| Prompt via stdin | yes (write + close) | yes (omit positional, or use `-`) |
| Structured streaming | plain stdout lines | `--json` → JSONL events on stdout |
| Resume | `--resume <uuid>` (host-generated) | `codex exec resume <id> [PROMPT]` (codex-generated ID) |
| Session ID ownership | host generates UUID, passes as `--session-id` | codex generates ID; host captures from JSONL output |
| Sandbox policy | n/a | `-s read-only` (safe MVP default) / `workspace-write` (full agent) |
| Working directory | cwd at spawn | `-C <dir>` / `--cd <dir>` |

**Adapter design (`CodexNativeProvider`):**

- **NOT a copy-paste of `ClaudeCliProvider`.** The arg assembly, event-stream
  parsing, and session-ID lifecycle differ. The adapter shares the
  `ProviderAdapter` trait surface and the `NativeSession` state shape, but
  implements its own `execute()` path.
- **Per-invocation mode only for V1.116.** codex `exec` is designed for
  non-interactive single-shot prompts. Persistent child reuse (claude's
  stdin/stdout delimited mode) is not applicable — codex `exec` exits after
  one prompt. No `new_persistent()` constructor needed this iteration.
- **Session ID lifecycle:** `launch()` registers a `NativeSession` with
  `codex_session_id: None`. On first `execute()`, spawn `codex exec --json -s
  read-only` with the prompt on stdin; parse the JSONL stream; capture the
  session ID from the first event that carries it (store in session state).
  On subsequent `execute()` calls, use `codex exec resume <id> --json`.
- **`session_restore` capability:** claim `true` (codex supports resume via
  `exec resume <id>`). If the first-exec JSONL parsing fails to capture a
  session ID, fall back to per-invocation mode without resume and log a warning.
- **JSONL event parsing:** codex `--json` emits structured events (session
  start, message delta, tool call, finish). The adapter maps these to
  `HostEvent` variants. V1.116 MVP maps: session-start → (internal),
  message-delta → `MessageDelta`, finish → `OpFinished`. Tool-call and
  thought-delta events can be deferred (log-and-skip for MVP).

**Product fallback (Risk Register mitigation):** if codex `--json` output shape
is incompatible or unstable across versions, the adapter falls back to plain
stdout line streaming (like claude-native) without `--json`. This preserves
"session can start and accept a prompt" even without structured events.

### AD-2: Wire representation — no schema field change needed

`AgentScanEntry.registry_agent_id` is already `["string", "null"]` in
`schemas/daemon-api/agent-host/agent-scan-entry.schema.json`. The schema
description already says "Null for custom wizard-supplied entries that have no
registry match." Native CLI entries reuse `registry_agent_id: null`.

**No `schemas/` field change required.** Description update is additive
(non-breaking): mention "native CLI providers (e.g. claude-native, codex-native)"
alongside "custom wizard-supplied entries." `@42ch/nexus-contracts` stays at
current version. `wire_contracts_changed` stays `false` (description-only).

### AD-3: Scan endpoint merge — additive append + family dedup

The `scan()` handler in `agent_host.rs` currently iterates `registry.agents`
only. The merge appends native CLI entries discovered via
`nexus_agent_host::discovery::path_scan::scan_path()` after the ACP entries:

```
agents[] = [ACP registry entries...] ++ [native CLI entries...]
```

**Mapping `ProviderCatalogEntry` → `AgentScanEntry`:**

| AgentScanEntry field | Source |
| --- | --- |
| `name` | `display_name` from catalog entry |
| `registry_agent_id` | `None` (native CLI has no registry match) |
| `launch_command` | `LaunchStrategy::NativeCli.command` |
| `installed` | `health.available` |
| `version` | `None` (native CLI path scan does not probe `--version`; deferred) |
| `description` | `None` |
| `icon_url` | `None` |

**Dedup rule (codex-acp vs codex-native):**

codex-acp has `binary: None` in the registry → it never probes as installed.
codex-native appears when `codex` CLI is on PATH. In practice both can coexist
in the response (codex-acp as "not installed", codex-native as "installed").

Product preference: **codex-native is the honest selectable path.** Suppress
rule: when a native entry for the "codex" family exists AND is installed,
suppress the registry `codex-acp` entry from the response. Implementation:

```rust
const NATIVE_PREFERRED_FAMILIES: &[(&str, &str)] = &[
    // (registry_agent_id to suppress, native provider_id that overrides it)
    ("codex-acp", "codex-native"),
];
```

When `codex-native` is in the native entries and installed, filter out
`registry_agent_id == "codex-acp"` from the ACP entries. When codex-native is
absent or not installed, codex-acp remains (shows "not installed" honestly).

`claude-acp` vs `claude-native`: same rule applies. If claude-native is
installed and claude-acp is in the registry, suppress claude-acp.

### AD-4: Bare-cmd extraction — `file_name()` handles all shapes

```rust
fn bare_command_name(cmd: &str) -> String {
    std::path::Path::new(cmd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cmd.to_string())
}
```

| Input | Output |
| --- | --- |
| `./kimi` | `kimi` |
| `./dist-package/cursor-agent` | `cursor-agent` |
| `kimi` | `kimi` |
| `opencode` | `opencode` |
| `.\dist-package\cursor-agent` (Windows) | `cursor-agent` |

Applied in **two** places to keep keys consistent:
1. `scan_local_installations_impl` — extract bare name before probing
2. `build_scan_entry` / `platform_binary_commands` — extract bare name before
   matching against `by_binary` keys and before setting `launch_command`

`launch_command` should show the bare name (e.g. `kimi`), not the relative
path (`./kimi`) — it is the command a user would type.

### AD-5: Frontend — `registry_agent_id: null` rendering

The Setup list must not filter on `registry_agent_id` being non-null. Native
entries use `null` as the product signal. Any existing filter that assumes
`registry_agent_id` is always present must be updated. The `installed` boolean
is the primary display driver; `registry_agent_id` is metadata.

## Mapping to plan tasks

| AC | Plan tasks |
| --- | --- |
| AC-P0-1 | T1 bare-cmd extraction |
| AC-P0-2 | T2 KNOWN_COMMANDS + T3 CodexNativeProvider |
| AC-P0-3 | T1 + T4 scan merge (+ T2 for codex) |
| AC-P0-4 | T5 Verify copy + native entry rendering |
