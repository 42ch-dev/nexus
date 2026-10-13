# native cli provider adapter pattern — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/native-cli-provider-adapter-pattern.md`](../../../knowledge/architecture-patterns/native-cli-provider-adapter-pattern.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 16

```markdown
  - retired-implementation
```

### at pre-freeze line 22

```markdown
  - Reading the historical ACP bare-command extraction / scan-dedup guidance
```

### at pre-freeze line 25

```markdown
# Native CLI Provider Adapter Pattern + ACP Registry Bare-Command Extraction
```

### at pre-freeze line 27

```markdown
## V1.188 update — runtime identity, readiness and retained ownership
```

### at pre-freeze line 69

```markdown
[principal-bound run-event subscription](../../../knowledge/architecture-patterns/principal-bound-run-event-subscription.md);
the original daemon-writer record is archived ([daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) §20).
Derived from verified V1.188 P0–P4 runtime, recovery and review evidence;
the public first-run/Quick Start/live-model composition was explicitly deferred.
```

### at pre-freeze line 74

```markdown
## V1.168 update — external protocol clients replace self-written wire parsers
```

### at pre-freeze line 76

```markdown
Since V1.168, native providers do **not** own vendor wire formats. The provider is a thin `ProviderAdapter` shell; an external crates.io protocol client owns spawn + protocol:
```

### at pre-freeze line 84

```markdown
Hard-won rules from the V1.168 replacement (QC findings B-1..B-3, B-1..B-4):
```

### at pre-freeze line 91

```markdown
6. **Honest capability descriptors:** declare only what the client surface supports. As amended in V1.188, dsh uses `CapabilityDescriptor::dsh_limited()` with message-level `streaming: true`, `cancellation: false`, `session_restore: true`, `text_prompt: true` — do not borrow `native_cli_limited()` when the surface differs.
```

### at pre-freeze line 93

```markdown
8. **Discovery can have more than one route.** dsh-native resolves an explicit command, `DSH_RUNTIME_BIN`, or PATH `dsh`; catalog and boot must consume the same selected recipe. The old `dsh-jsonrpc-agent` route is retired.
9. No `NATIVE_PREFERRED_FAMILIES` row unless an ACP twin exists. (The table itself was removed with the ACP-host scan subsystem in v1.212, issue #391; catalog dedup at HEAD is per `provider_id` — `crates/nexus-agent-host/src/discovery/catalog.rs`.)
```

### at pre-freeze line 96

```markdown
The V1.116 guidance below is historical throughout: the session/lifecycle
sections there describe the retired self-written adapters, and the ACP
bare-command extraction / scan-dedup material describes the scan subsystem
removed in v1.212 (issue #391). Both are kept as design guidance.

## Historical (pre-V1.168): self-written adapters

### Per-invocation vs persistent process (retired)

- The old `claude-native` had a persistent empty-line-delimited mode — **deleted in V1.168**; the old `codex-native` used `codex exec --json` per-invocation with a hand-rolled JSONL parser — **replaced by the app-server client**.
- Hand-rolled `--session-id`/`--resume` argv assembly is gone; session continuity is crate-owned (`.session_id` / `.resume` / `thread_resume`).

### Session ID ownership (historical)

- ACP providers: host generates session ID.
- claude-native: host generates session ID (then passes it to the crate).
- codex-native (old): codex generated the session ID captured from JSONL; **now** the app-server thread id.
- dsh-native: host generates and reuses the session id via `start_session(Some(id))`.

## Context (V1.116)

Nexus supports two kinds of agent providers:
1. **ACP providers** — agents registered in the ACP registry, communicating via JSON-RPC
2. **Native CLI providers** — agents invoked directly as CLI processes (claude-native, codex-native; dsh-native since V1.168)

Native CLI providers exist for mainstream agent CLIs whose authors
want to use them directly, not through a community-provided ACP adapter. The
`claude-native` provider (Wave 1) was the first; `codex-native` (V1.116) was the
second; `dsh-native` (V1.168) is the third and first driven by a crates.io SDK client.

## Guidance

The following V1.116 adapter-construction/lifecycle notes are historical;
current native process and protocol ownership follows the SDK rules above.
The ACP bare-command extraction and native/ACP-twin dedup notes are historical
too: their helpers (`bare_command_name` / `probe_local_binary`) and the scan
handler that consumed `NATIVE_PREFERRED_FAMILIES` disappeared with the ACP-host
scan subsystem removed in v1.212 (issue #391).

### Adding a new native CLI provider
```

### at pre-freeze line 142

```markdown
       // ("gemini", "gemini-native"), // future
```

### at pre-freeze line 146

```markdown
2. **Create the provider adapter** in `providers/native_cli/<name>.rs`:
   - Study the CLI's actual invocation protocol (flags vs subcommands, stdin/stdout, session resume support)
   - **Each CLI is different** — codex is subcommand-based (`codex exec --json`), claude is flag-based
   - Implement: `default_config()`, `launch()`, `execute()`, `shutdown()`, `descriptor()`, `probe()`
   - Use `CapabilityDescriptor::native_cli_limited()` (no set_model, no set_mode, no structured_tool_calls)

3. **Register in `providers/native_cli/mod.rs`**

4. **Add to `NATIVE_PREFERRED_FAMILIES`** in the scan handler for dedup *(historical — the constant and the ACP-host scan handler were removed in v1.212, issue #391; dedup at HEAD is by `provider_id` in the catalog)*:
   - When the native provider is installed, suppress the matching ACP registry entry
   - Example: `codex-native` installed → `codex-acp` suppressed

5. **The scan endpoint automatically includes native providers** (V1.116 merge). *(Historical: the surviving `POST /v1/daemon/agent-host/scan` is served by host provider discovery — `crates/nexus-agent-host/src/discovery/path_scan.rs`.)*
```

### at pre-freeze line 160

```markdown
### Per-invocation vs persistent process
```

### at pre-freeze line 162

```markdown
- **claude-native**: persistent child process; multiple prompts over one process
- **codex-native**: per-invocation; `codex exec` exits after one prompt; resume via `codex exec resume <id>`
- The adapter must match the CLI's lifecycle model — do NOT assume one pattern fits all.

### Session ID ownership

- ACP providers: host generates session ID
- claude-native: host generates session ID
- **codex-native: codex generates session ID** (captured from JSONL `session_start` event)
- Adapter must document who owns the ID.

## Why This Matters

The Setup page is the first thing every new user sees. If installed agents show
as "not installed", users lose trust immediately. Two root causes were discovered
in V1.116:

### 1. ACP registry binary commands are relative paths

The ACP registry lists binary commands like `./kimi`, `./opencode`,
`./dist-package/cursor-agent`. The `probe_local_binary` function called
`which::which("./kimi")` which looks for a file named `./kimi` in the current
directory — NOT `kimi` on PATH.

**Fix (V1.116, historical):** `bare_command_name()` extracted the file-name
component using `Path::new(cmd).file_name()`, applied in both the probe-key
HashSet and the scan handler's `platform_binary_commands()` so probe keys and
matching agreed. That helper and its scan subsystem were removed in v1.212
(issue #391); the lesson survives as design guidance: a command string that may
be a path must be reduced to its bare executable name before a PATH probe, and
every consumer of that command must share the same normalization.

### 2. codex-acp has no binary distribution

The registry entry for codex (`codex-acp`) has `binary: None`. It is never
probed. Product decision: codex is a native CLI provider (like claude-native),
not ACP.
```

### at pre-freeze line 202

```markdown
- Adding a 3rd/4th native CLI provider (gemini, cursor-native if needed)
- Understanding why ACP registry agents with relative-path commands were invisible to PATH probing before V1.116 (the retired `bare_command_name()` fix)
- Writing PATH-probe code: reduce a possibly-relative command to its bare executable name

## Examples

Both snippets below are historical (removed with the ACP-host scan subsystem in
v1.212, issue #391); they illustrate the normalization and dedup rules that
still apply to any PATH-probe code.

### Bare-command extraction

```rust
pub fn bare_command_name(cmd: &str) -> String {
    std::path::Path::new(cmd)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| cmd.to_string())
}
// "./kimi" → "kimi"
// "./dist-package/cursor-agent" → "cursor-agent"
// "opencode" → "opencode"
```

### Scan endpoint dedup

```rust
const NATIVE_PREFERRED_FAMILIES: &[(&str, &str)] = &[
    ("codex-acp", "codex-native"),
    ("claude-acp", "claude-native"),
];
// When codex-native is installed, codex-acp entry is suppressed from scan response
```
```

### at pre-freeze line 238

```markdown
- `local-environment-scan-safety-boundary.md` — PATH enrichment (V1.110) and scan safety
```

### at pre-freeze line 240

```markdown
- V1.116 P0 spec `agent-detection-codex-native.md` — AD-1 through AD-5 architect decisions
```
