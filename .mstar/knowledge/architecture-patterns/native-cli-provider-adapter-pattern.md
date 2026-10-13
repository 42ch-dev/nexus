---
module: nexus-agent-host
date: 2026-07-14
last_updated: 2026-10-12
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
tags:
  - native-cli-provider
  - agent-host
  - acp-registry
  - bare-command
  - protocol-client
  - provider-readiness
  - message-level-streaming
applies_when:
  - Adding or changing a native provider
  - Diagnosing candidate-versus-ready drift
  - Mapping SDK callbacks to Host events
  - Retaining owned cleanup after caller timeout
  - Writing PATH-probe code that must normalize a possibly-relative command
---

# Native CLI Provider Adapter Pattern

## Runtime identity, readiness and retained ownership

The SDK cutover is not sufficient evidence that a provider is ready. Preserve
these boundaries when changing the adapter, boot path or public catalog:

1. **One recipe and one health owner.** Resolve explicit dsh command →
   `DSH_RUNTIME_BIN` → PATH `dsh`; an invalid explicit command must not fall
   back. The SDK 0.2 route is ordinary `--profile sdk`, not a helper alias.
   Config/PATH/registry selection feeds one factory and one manager entry;
   selected-and-enabled means candidate. Retain validated boot config instead
   of parsing a second copy or registering an ambient-only provider.
2. **Probe with admission provenance.** Use the same verified Creator and
   canonical workspace cwd as session admission. Missing context, malformed
   config, unsupported no-model handshake or unconfirmed child close stays
   unavailable. dsh must probe both ordinary and sealed deny-all recipes.
   Only a successful handshake plus confirmed direct-child exit can set ready;
   publishing against an unchanged candidate identity prevents stale revival.
3. **Classify by lifecycle stage.** Later launch/initialize/session failure
   invalidates that entry and therefore both catalog and admission. A real
   prompt-content timeout after initialization does not. Test both transitions,
   not merely `is_err()` or an arbitrary fraction of a probe budget.
4. **Sanitize at the public boundary.** A provider-health message must use a
   static category, not SDK `Display`; truncating it to 512 bytes is not
   sanitization. The Codex regression injects hostile initialize text, proves
   initialize was received, then checks the public message excludes it. Its
   negative control fails when the original interpolation is restored.
5. **Map message units, not guessed transcripts.** Emit each complete root
   assistant message while the SDK run is active. Once any text was emitted,
   ignore final response entirely; only a no-message run uses a nonempty final
   fallback. Partial text followed by malformed/non-success output still fails.
   dsh streaming is message-level; cancellation remains false. Nexus delivery
   caps do not bound the SDK's internal accumulation, and its callback window
   is not a submitted-message causal receipt.
6. **A waiter is not the resource owner.** Keep one cleanup task/session/home
   lease through SDK close even after a caller timeout or dropped consumer.
   Unconfirmed cleanup remains actionable Interrupted. Normal direct-child
   exit proof does not justify a process-tree/restart no-leak claim or a blind
   PID kill. A sealed deny-all profile is model-tool denial, not an OS sandbox.

Durable contracts: [agent-host](../../specs/active/agents/agent-host.md) §§5–8,
[workspace OCC/recovery](../../specs/active/runtime/concurrency.md) §9 and
[run settlement / same-run replay](../../specs/active/orchestration/orchestration-engine.md) §15–§15.1 with
[principal-bound run-event subscription](principal-bound-run-event-subscription.md).

## External protocol clients replace self-written wire parsers

Native providers do **not** own vendor wire formats. The provider is a thin `ProviderAdapter` shell; an external crates.io protocol client owns spawn + protocol:

| Provider | Client crate (pin in workspace `Cargo.toml` only, never in `docs/`) | Wire |
|----------|---------------------------------------------------------------|------|
| `claude-native` | `claude-codes` (`default-features = false`, `async-client`) | Claude Code stream-json |
| `codex-native` | `codex-codes` (`default-features = false`, `async-client`) | `codex app-server` JSON-RPC |
| `dsh-native` | `deepseek-harness-sdk` | DSH runtime stdio JSON-RPC 2.0 |

Hard-won rules from the wire-parser replacement (QC findings B-1..B-3, B-1..B-4):

1. **Never hold the session-registry `RwLock` across crate I/O.** `receive()`/`next_message()`/setup RPCs run behind a **per-session `Mutex`** on the crate client; the map lock is for short metadata lookups only. A global write lock across frame reads serializes every session on the provider and stalls `cancel()`/`shutdown()` for up to a full frame budget.
2. **Do not reuse the prompt-setup timeout as an inter-frame read timeout.** Long-silent turns (tool execution) emit no frames for minutes; a 180 s frame-gap timeout hard-fails them. Streaming reads wait until stream end; keep a separate budget only if a lock explicitly says so.
3. **Long-lived app-server clients must filter turn-scoped notifications by the active turn id** and interrupt+drain on decode-error/timeout/cancel — otherwise a stale `turn/completed` ends the *next* turn empty-success.
4. **Decode-drift contract (fixed):** unknown nested variant / unknown method → per-item skip + debug; typed-decode failure / stream abort → exactly one terminal `OpFailed` for the turn (`decode_error` / `stream_closed` / `provider_error` / `timeout` / `io_error` tokens; `error_message` truncated at 512 bytes — crate `Display` embeds raw wire frames).
5. **No-protocol-RPC runtimes (e.g. dsh, no cancel)**: on execute timeout, rotate the stored session id so a retry starts a fresh session instead of splicing into a zombie turn.
6. **Honest capability descriptors:** declare only what the client surface supports. dsh uses `CapabilityDescriptor::dsh_limited()` with message-level `streaming: true`, `cancellation: false`, `session_restore: true`, `text_prompt: true` — do not borrow `native_cli_limited()` when the surface differs.
7. **Test protocol adapters with spawnable mock protocol stubs** (`mock_claude_cli.py`, `mock_codex_app_server.py`, `mock_dsh_agent.py` under `crates/nexus-agent-host/tests/fixtures/native_protocol/`) — real protocol shapes without the vendor binary. Do not fake protocols with `echo` scripts.
8. **Discovery can have more than one route.** dsh-native resolves an explicit command, `DSH_RUNTIME_BIN`, or PATH `dsh`; catalog and boot must consume the same selected recipe.
9. **Dedup is per `provider_id`.** Do not maintain a separate native-preferred-family table; the catalog dedups by `provider_id` (`crates/nexus-agent-host/src/discovery/catalog.rs`).

## Adding a new native CLI provider

1. **Add to `KNOWN_COMMANDS`** in `path_scan.rs`:

   ```rust
   const KNOWN_COMMANDS: &[(&str, &str)] = &[
       ("claude", "claude-native"),
       ("codex", "codex-native"),
       ("dsh", "dsh-native"),
   ];
   ```

2. **Create the provider adapter** in `providers/native_cli/<name>.rs`: implement `default_config()`, `launch()`, `execute()`, `shutdown()`, `descriptor()`, `probe()`, and use the capability descriptor that matches the client surface (rule 6 above).
3. **Register in `providers/native_cli/mod.rs`.** The surviving `POST /v1/daemon/agent-host/scan` is served by host provider discovery (`crates/nexus-agent-host/src/discovery/path_scan.rs`).

## PATH probing and command normalization

A command string that may be a path must be reduced to its **bare executable name** before a PATH probe, and every consumer of that command must share the same normalization. A probe that calls `which("./kimi")` looks for a file named `./kimi` in the current directory, not `kimi` on PATH — so relative-path ACP registry commands (`./kimi`, `./opencode`, `./dist-package/cursor-agent`) are invisible until the file-name component is extracted (`Path::new(cmd).file_name()`). A registry entry with `binary: None` (e.g. `codex-acp`) is never probed; codex is served as a native CLI provider, not ACP.

## When to Apply

- Adding a native CLI provider.
- Writing PATH-probe code: reduce a possibly-relative command to its bare executable name.
- Diagnosing candidate-versus-ready drift.

## See also

- `local-environment-scan-safety-boundary.md` — scan safety
- `canvas-surface-implementation-pattern.md` — canvas adapter pattern (analogous generic-adapter approach)
