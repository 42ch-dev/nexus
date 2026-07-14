---
module: nexus-agent-host
date: 2026-07-14
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-13-v1.116-agent-detection-codex-native
tags: [native-cli-provider, agent-host, codex-native, claude-native, acp-registry, bare-command, scan-endpoint]
applies_when: adding a new native CLI provider to the agent-host, or debugging agent detection false negatives
---

# Native CLI Provider Adapter Pattern + ACP Registry Bare-Command Extraction

## Context

Nexus supports two kinds of agent providers:
1. **ACP providers** — agents registered in the ACP registry, communicating via JSON-RPC
2. **Native CLI providers** — agents invoked directly as CLI processes (claude-native, codex-native)

Native CLI providers exist for mainstream agent CLIs (claude, codex) whose authors
want to use them directly, not through a community-provided ACP adapter. The
`claude-native` provider (Wave 1) was the first; `codex-native` (V1.116) is the
second, establishing the pattern for future additions.

## Guidance

### Adding a new native CLI provider

1. **Add to `KNOWN_COMMANDS`** in `path_scan.rs`:
   ```rust
   const KNOWN_COMMANDS: &[(&str, &str)] = &[
       ("claude", "claude-native"),
       ("codex", "codex-native"),
       // ("gemini", "gemini-native"), // future
   ];
   ```

2. **Create the provider adapter** in `providers/native_cli/<name>.rs`:
   - Study the CLI's actual invocation protocol (flags vs subcommands, stdin/stdout, session resume support)
   - **Each CLI is different** — codex is subcommand-based (`codex exec --json`), claude is flag-based
   - Implement: `default_config()`, `launch()`, `execute()`, `shutdown()`, `descriptor()`, `probe()`
   - Use `CapabilityDescriptor::native_cli_limited()` (no set_model, no set_mode, no structured_tool_calls)

3. **Register in `providers/native_cli/mod.rs`**

4. **Add to `NATIVE_PREFERRED_FAMILIES`** in the scan handler for dedup:
   - When the native provider is installed, suppress the matching ACP registry entry
   - Example: `codex-native` installed → `codex-acp` suppressed

5. **The scan endpoint automatically includes native providers** (V1.116 merge).

### Per-invocation vs persistent process

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

**Fix:** `bare_command_name()` extracts the file-name component using
`Path::new(cmd).file_name()`. Applied in both the probe-key HashSet and the scan
handler's `platform_binary_commands()` for consistent matching.

### 2. codex-acp has no binary distribution

The registry entry for codex (`codex-acp`) has `binary: None`. It is never
probed. Product decision: codex is a native CLI provider (like claude-native),
not ACP.

## When to Apply

- Adding a 3rd/4th native CLI provider (gemini, cursor-native if needed)
- Debugging agent detection false negatives (check bare-cmd extraction first)
- Understanding why ACP registry agents with relative-path cmds are invisible

## Examples

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

## See also

- `local-environment-scan-safety-boundary.md` — PATH enrichment (V1.110) and scan safety
- `canvas-surface-implementation-pattern.md` — canvas adapter pattern (analogous generic-adapter approach)
- V1.116 P0 spec `agent-detection-codex-native.md` — AD-1 through AD-5 architect decisions
