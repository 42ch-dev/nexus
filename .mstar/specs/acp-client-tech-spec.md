# ACP Client Integration — Technical Specification

**Status:** Shipped. Current implementation uses official `agent-client-protocol = "=0.11.1"` behind Nexus-owned DTOs; daemon-orchestrated ACP sessions are delegated to per-creator `nexus42 acp-worker` children, while the route-facing daemon `HostManager` currently registers installed native CLI providers. Sections 2–10 retain the original V1.0 design and migration record where not superseded by the current amendment below.
**Document class**: Master  

**Source Plan**: `2025-04-05-acp-client`
**Date**: 2026-04-06
**Last reconciled**: 2026-09-04 — current SDK pin, client trait authority, provider wiring, and ACP worker boundary through V1.183.


The shipped boundary is:

- `crates/nexus-acp-host/Cargo.toml` pins
  `agent-client-protocol = "=0.11.1"`.
- Official SDK types are confined to `nexus-acp-host`; the public
  `NexusAcpClient` trait uses Nexus-owned DTOs and includes both one-shot and
  streaming prompt operations without treating a method count as a contract.
- `nexus-agent-host::providers::acp` consumes `NexusAcpClient` /
  `AcpSdkAdapter`, but those provider-specific types do not cross the
  `HostFacade` boundary.
- The daemon runtime does not import SDK protocol types or execute ACP sessions
  in process. Daemon orchestration delegates those sessions to a per-creator
  `nexus42 acp-worker` child; the route-facing `HostManager` boot path currently
  registers installed `codex-native`, `claude-native`, and `dsh-native`
  adapters.

---

## Table of Contents

1. [SDK Selection Decision](#1-sdk-selection-decision)
2. [Integration Architecture](#2-integration-architecture)
3. [Registry Integration Detailed Design](#3-registry-integration-detailed-design)
4. [Daemon API Contract Analysis](#4-daemon-api-contract-analysis)
5. [Skills / Capability Export](#5-skills--capability-export)
6. [CLI Command Detailed Design](#6-cli-command-detailed-design)
7. [Schema Definitions](#7-schema-definitions)
8. [ACP-R1 and ACP-R2 Resolution](#8-acp-r1-and-acp-r2-resolution)
9. [Test Strategy](#9-test-strategy)
10. [Refined Task Breakdown](#10-refined-task-breakdown)

---

## 1. SDK Selection Decision

### 1.1 Current dependency and boundary

The shipped ACP SDK is **`agent-client-protocol` 0.11.1**, exact-pinned in
`crates/nexus-acp-host/Cargo.toml`. `crates/nexus-acp-host/src/client.rs`
confines `agent_client_protocol::schema` types to `AcpSdkAdapter` conversion
and implementation code; consumers use nexus contract DTOs through
`NexusAcpClient`.

`crates/nexus-agent-host/src/providers/acp.rs` adapts that client boundary to
the normalized `ProviderAdapter` lifecycle (initialize, session creation,
prompt/stream, cancel, shutdown). The daemon runtime does not directly link
the SDK. No fixed client-method count is normative: the trait definition and
its adapter implementation are the source authority as protocol support
evolves.

---

## 2. Integration Architecture

### 2.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    nexus42 CLI                          │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────┐    │
│  │ Commands │──▶│ ACP Mod  │──▶│ NexusAcpClient   │    │
│  │ (agent/*)│   │          │   │ (adapter trait)   │    │
│  └──────────┘   │ registry │   └────────┬─────────┘    │
│                 │ client   │            │              │
│                 │ skills   │            │ stdio        │
│                 └────┬─────┘            │ (JSON-RPC)    │
│                      │                  │              │
└──────────────────────┼──────────────────┼──────────────┘
                        │                  │
                        │ HTTP GET         │ stdin/stdout
                        ▼                  ▼
               ┌─────────────┐    ┌─────────────────┐
               │ ACP CDN     │    │ Agent Subprocess│
               │ (registry)  │    │ (e.g. Claude,   │
               │             │    │  Codex, Cline)  │
               └─────────────┘    └─────────────────┘

┌─────────────────────────────────────────────────────────┐
│                   daemon runtime (historical V1.0)      │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────┐    │
│  │ HTTP     │──▶│ Handlers │──▶│ WorkspaceState   │    │
│  │ Router   │   │          │   │ (SQLite)         │    │
│  │ (axum)   │   │ + ACP    │   └──────────────────┘    │
│  └──────────┘   │ proxy    │                           │
│                 │ routes   │   (daemon runtime is NOT an     │
│                 └──────────┘    ACP Agent/Server)       │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Module Layout

All ACP-related code lives in `apps/nexus42/src/acp/`:

```
apps/nexus42/src/acp/
├── mod.rs          # Public API, re-exports
├── client.rs       # NexusAcpClient trait + AcpSdkAdapter impl
├── registry.rs     # Registry manifest fetcher + local cache
├── skills.rs       # Capability set definition + skills manifest
├── error.rs        # ACP-specific error types
└── transport.rs    # Subprocess spawn + stdio pipe management
```

New CLI command module:
```
apps/nexus42/src/commands/
└── agent.rs        # agent list/show/install/run/probe subcommands
```

### 2.3 Process Model

**Agent Subprocess Lifecycle:**

```
nexus42 agent run <agent-ref>
  │
  ├─ 1. Resolve agent-ref → manifest (from registry cache)
  ├─ 2. Determine launch command (npx or binary)
  ├─ 3. Spawn subprocess via tokio::process::Command
  │      - stdin/stdout pipes for JSON-RPC
  │      - stderr inherited (for agent logging)
  │      - environment variables forwarded
  ├─ 4. ACP Client connects via stdin/stdout
  │      - initialize → capabilities exchange
  │      - authenticate (if agent requires)
  │      - session/new or session/load
  ├─ 5. Interactive prompt loop
  │      - User types message → session/prompt
  │      - Agent streams response (markdown)
  │      - Agent requests tools → grant/deny (V1.0: auto-grant with warning)
  ├─ 6. On exit/cancel: send cancel notification, wait for graceful shutdown
  └─ 7. Clean up subprocess
```

**Key Implementation Details:**

- The `tokio::task::LocalSet` requirement: ACP SDK futures are `!Send`, requiring `spawn_local`. The CLI's `#[tokio::main]` creates a multi-threaded runtime by default. We must use `tokio::task::LocalSet` within the agent session to bridge this gap.
- **Timeout**: Default 30-second timeout for `initialize`, 5-minute for `session/prompt` (configurable).
- **Error handling**: Non-zero exit code, broken pipe, timeout — all map to `AcpError` variants with user-friendly messages.
- **Daemon relationship**: daemon runtime is **NOT** involved in the ACP communication path. The CLI spawns and talks to agents directly. Daemon-mediated tool access, session persistence, and permission policy — **durable roadmap:** DR-20 (daemon-mediated tool access + permission policy engine), DR-21 (ACP session persistence).

### 2.4 Connection Management

```
struct AcpSession {
    agent_id: String,           // e.g. "claude-acp"
    agent_version: String,      // e.g. "0.18.0"
    session_id: Option<String>, // ACP session ID
    child: tokio::process::Child,
    client: Box<dyn NexusAcpClient>,  // adapter over SDK Client
}
```

**Lifecycle**:
1. **Create**: Spawn process, initialize, session/new
2. **Use**: session/prompt in a loop (interactive or single-shot)
3. **Destroy**: cancel notification, SIGTERM, wait 5s, SIGKILL if needed

### 2.5 Dependency on Daemon

**V1.0**: nexus42 communicates with agents directly via stdio. No daemon involvement.

**V1.1+ (deferred)**: The daemon could provide:
- A proxy for agent tool calls (e.g., file system access through daemon's workspace-aware handlers)
- Session persistence (agent state across CLI invocations)
- Permission policy enforcement (centralized `request_permission` handling)

This is captured as a residual finding (see §10, Task 5 notes).

---

## 3. Registry Integration Detailed Design

### 3.1 Registry Data Model

The ACP Registry at `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` returns:

```json
{
  "version": "1.0.0",
  "agents": [
    {
      "id": "claude-acp",
      "name": "Claude Agent",
      "version": "0.18.0",
      "description": "ACP wrapper for Anthropic's Claude",
      "repository": "https://github.com/zed-industries/claude-agent-acp",
      "authors": ["Anthropic"],
      "license": "proprietary",
      "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg",
      "distribution": {
        "npx": {
          "package": "@zed-industries/claude-agent-acp@0.18.0"
        }
      }
    }
  ],
  "extensions": []
}
```

**Current registry agents (16 total as of 2026-04-06):**
`auggie`, `claude-acp`, `cline`, `codebuddy-code`, `codex-acp`, `corust-agent`, `factory-droid`, `gemini`, `github-copilot`, `junie-acp`, `kimi`, `mistral-vibe`, `opencode`, `qoder`, `qwen-code`, `stakpak`

### 3.2 Caching Strategy

**Cache Directory**: `$HOME/.nexus42/registry/`

```
$HOME/.nexus42/registry/
├── cache.json          # Full registry response (fetched manifest)
├── cache_meta.json     # Fetch timestamp, ETag, version
└── agents/
    └── <agent-id>/     # Per-agent launch info (installed binaries)
        └── meta.json   # Installation status, local path, version
```

**Cache Policy:**

| Scenario | Behavior |
|----------|---------|
| Cache exists, < 24h old | Use cache, no network |
| Cache exists, >= 24h old | Fetch in background, use cache immediately (stale-while-revalidate) |
| Cache exists, no network | Use cache (offline mode) |
| No cache, no network | Error: "Unable to fetch agent registry. Check network connection." |

**Implementation:**

```rust
// Pseudocode for cache logic
struct RegistryCache {
    cache_dir: PathBuf,      // $HOME/.nexus42/registry/
    meta: CacheMeta,         // { fetched_at, version }
}

impl RegistryCache {
    fn max_age() -> Duration { Duration::from_secs(24 * 3600) }

    async fn get_or_fetch(&self) -> Result<Registry> {
        if let Some(cached) = self.load_cached() {
            if cached.age() < Self::max_age() {
                return Ok(cached.registry);
            }
            // Stale-while-revalidate: spawn background refresh
            // but return cached data immediately
        }
        // No cache or expired beyond tolerance: fetch
        self.fetch_and_cache().await
    }
}
```

### 3.3 Agent Discovery Flow

```
nexus42 agent list
  │
  ├─ Load cached registry (or fetch)
  ├─ Filter/format for display
  └─ Output: table of available agents

nexus42 agent show <agent-ref>
  │
  ├─ Resolve agent-ref (partial match on id or name)
  ├─ Load from registry cache
  └─ Output: full agent details + installation status
```

### 3.4 Agent Installation Flow

For V1.0, installation is **lazy** — agents are launched on demand. No pre-installation step.

**For `npx`-based agents:**
- Requires `node` and `npm` (or `npx`) on PATH
- First launch may be slow (npm download + install)
- `nexus42 agent run <npx-agent>` spawns: `npx <package> --acp`

**For `binary`-based agents:**
- First launch downloads the platform-appropriate archive from `distribution.binary.<platform>.archive`
- Extracts to `$HOME/.nexus42/agents/<agent-id>/bin/`
- Subsequent launches use the cached binary
- V1.0: no automatic update mechanism; manual `nexus42 agent install --update <agent-id>` for refresh

---

## 4. Daemon API Contract Analysis

### 4.1 Question: Does nexus42 need a Daemon API for agent communication?

**Short answer: No for V1.0. Direct stdio between CLI and agent.**

**Analysis:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Direct stdio** (Recommended) | CLI spawns agent, communicates via stdin/stdout JSON-RPC | Simple, matches ACP spec, no extra infra | Agent cannot access daemon services |
| **B: Daemon-mediated** | CLI → daemon HTTP → agent stdio | Centralized, daemon can enforce policies | Adds latency, complexity, violates "daemon runtime is not ACP server" |
| **C: Daemon API as tool server** | Agent calls Daemon API for workspace/file access | Rich tool access | V1.1+ scope, requires tool permission handling |

**Decision: Option A for V1.0.**

The ACP protocol is designed for direct stdio communication. The existing `DaemonClient` in `apps/nexus42/src/api/daemon_client.rs` provides HTTP access to the daemon for CLI-internal use (health checks, sync, etc.), but agents do NOT talk to the daemon in V1.0.

### 4.2 V1.0 Daemon API Additions (Minimal)

No new Daemon API endpoints were required for the original V1.0 ACP integration. Current CLI-internal daemon access uses the `/v1/daemon/*` namespace; agents do not receive raw daemon access merely by participating in ACP.

### 4.3 V1.1+ Daemon API Expansion (Deferred)

> **Durable roadmap:** DR-20 (daemon-mediated tool access + centralized permission policy engine). These endpoints are documented for future reference but NOT part of the V1.0 task breakdown.

---

## 5. Skills / Capability Export

### 5.1 What Capabilities Should nexus42 Expose?

In the ACP protocol, the **client** (nexus42) declares its capabilities during `initialize`. This tells the agent what the client supports. For V1.0, nexus42 should declare:

| Capability ID | Description | V1.0 |
|---------------|-------------|------|
| `file_system.read` | Client can read files and provide content to agent | **Yes** — via `fs/read_text_file` handler |
| `file_system.write` | Client can write files on behalf of agent | **Yes** — via `fs/write_text_file` handler |
| `terminal.create` | Client can create terminal sessions for agent | **Yes** — via `terminal/create` handler |
| `terminal.output` | Client can stream terminal output | **Yes** — via `terminal/output` handler |
| `terminal.release` | Client can release terminal sessions | **Yes** — via `terminal/release` handler |

> **Durable roadmap:** DR-22 (`terminal.kill` / `terminal.wait_for_exit`, `slash_commands`, `agent_plan`, persistent skills manifest, binary auto-update, `session.modes`).

### 5.2 Capability ID Registry (Frozen for V1.0)

These are the **frozen capability IDs** that nexus42 will declare during ACP `initialize`:

```rust
/// Frozen capability IDs for V1.0
pub mod capabilities {
    // File system capabilities
    pub const FILE_SYSTEM_READ: &str = "file_system.read";
    pub const FILE_SYSTEM_WRITE: &str = "file_system.write";

    // Terminal capabilities
    pub const TERMINAL_CREATE: &str = "terminal.create";
    pub const TERMINAL_OUTPUT: &str = "terminal.output";
    pub const TERMINAL_RELEASE: &str = "terminal.release";
}
```

**Rationale for included capabilities:**
- `file_system.read` / `file_system.write`: Essential for any coding agent. The agent needs to read project files and write modifications.
- `terminal.create` / `terminal.output` / `terminal.release`: Basic terminal support for agents that run commands.

**Rationale for deferred capabilities:**
- `terminal.kill` / `terminal.wait_for_exit`: Advanced terminal management — not needed for basic V1.0 workflow.
- `slash_commands`: Requires UI integration in the CLI prompt loop.
- `agent_plan`: Requires structured plan rendering in the CLI.
- `session.modes`: Requires mode switching logic in the CLI.

### 5.3 Skills Manifest (V1.0 Minimal)

For V1.0, nexus42 does NOT export a formal skills manifest file. The capabilities are declared dynamically during `initialize`. A persistent skills manifest (`$HOME/.nexus42/skills.json`) can be added in V1.1+ for multi-agent host integration — **durable roadmap:** DR-22 (persistent skills manifest).

---

## 6. CLI Command Detailed Design

### 6.1 Command Tree

```
nexus42 agent <subcommand>

Subcommands:
  list              List available agents from registry
  show <agent-ref>  Show details for a specific agent
  run <agent-ref>   Run an agent interactively
  probe [--registry|--agent <ref>]  Verify ACP connectivity (ACP-R2)
```

### 6.2 Command Specifications

#### `nexus42 agent list`

```bash
# Usage
nexus42 agent list [--format text|json] [--installed-only]

# Flags
--format, -f    Output format (default: text)
--installed-only  Show only locally installed binary agents

# Output (text)
╭─────────────────────┬──────────────┬───────────┬──────────────────────────────────╮
│ ID                  │ Version      │ Source    │ Description                      │
├─────────────────────┼──────────────┼───────────┼──────────────────────────────────┤
│ claude-acp          │ 0.18.0       │ npx       │ ACP wrapper for Anthropic's Claude│
│ codex-acp           │ 0.9.4        │ binary    │ ACP adapter for OpenAI's Codex   │
│ cline               │ 2.4.2        │ npx       │ Autonomous coding agent CLI      │
│ ...                 │ ...          │ ...       │ ...                              │
╰─────────────────────┴──────────────┴───────────┴──────────────────────────────────╯
16 agents available (registry v1.0.0, cached 2026-04-06T10:30:00Z)

# Output (json)
{
  "registry_version": "1.0.0",
  "cached_at": "2026-04-06T10:30:00Z",
  "agents": [
    {
      "id": "claude-acp",
      "name": "Claude Agent",
      "version": "0.18.0",
      "description": "ACP wrapper for Anthropic's Claude",
      "source": "npx",
      "installed": false,
      "license": "proprietary"
    }
  ]
}
```

#### `nexus42 agent show <agent-ref>`

```bash
# Usage
nexus42 agent show <agent-ref>

# agent-ref: partial match on id or name (e.g. "claude" matches "claude-acp")

# Output (text)
Agent: Claude Agent (claude-acp)
Version: 0.18.0
License: proprietary
Repository: https://github.com/zed-industries/claude-agent-acp
Description: ACP wrapper for Anthropic's Claude
Source: npx (@zed-industries/claude-agent-acp@0.18.0)
Installed: no
```

#### `nexus42 agent run <agent-ref>`

```bash
# Usage
nexus42 agent run <agent-ref> [--message <msg>] [--session <id>] [--cwd <path>]

# Flags
--message, -m <msg>    Send a single message and exit (non-interactive)
--session <id>         Resume an existing session
--cwd <path>           Working directory for agent (default: current directory)

# Interactive mode (default)
nexus42 agent run claude-acp
# → Spawns agent, enters interactive prompt loop:
#   User: refactor the auth module
#   Claude: I'll refactor the auth module. Here's my plan...
#   [Agent requests: fs/read_text_file → auto-granted]

# Single-shot mode
nexus42 agent run claude-acp -m "explain the sync module"
# → Sends message, prints response, exits
```

#### `nexus42 agent probe` (ACP-R2)

```bash
# Usage
nexus42 agent probe [--registry | --agent <agent-ref>]

# Flags
--registry              Probe ACP Registry connectivity (default)
--agent <agent-ref>     Probe a specific agent's ACP handshake

# Output (registry probe)
nexus42 agent probe --registry
✓ ACP Registry reachable
  URL: https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json
  Version: 1.0.0
  Agents: 16
  Latency: 142ms

# Output (agent probe)
nexus42 agent probe --agent claude-acp
✓ Agent probe successful
  Agent: claude-acp v0.18.0
  Distribution: npx (@zed-industries/claude-agent-acp@0.18.0)
  ACP initialize: OK
  Capabilities: [file_system.read, file_system.write, terminal.create, ...]
  Latency: 892ms (includes npm resolve time)
```

### 6.3 Integration with Existing Command Architecture

The new `Agent` command follows the exact pattern of existing commands (`DaemonCommand`, `SyncCommand`, etc.):

```rust
// In apps/nexus42/src/commands/mod.rs — add:
pub mod agent;

// In apps/nexus42/src/main.rs — add to Commands enum:
/// Agent management (ACP integration)
Agent {
    #[command(subcommand)]
    command: AgentCommand,
},

// In match block:
Some(Commands::Agent { command }) => commands::agent::run(command, &config).await,
```

---

## 7. Schema Definitions

### 7.1 New JSON Schema: Registry Manifest

File: `schemas/acp-runtime/registry-manifest.schema.json`

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://schemas.nexus42.io/acp-runtime/registry-manifest.schema.json",
  "title": "ACP Registry Manifest",
  "description": "Schema for the ACP Registry manifest response",
  "type": "object",
  "required": ["version", "agents"],
  "properties": {
    "version": {
      "type": "string",
      "description": "Registry format version"
    },
    "agents": {
      "type": "array",
      "items": {
        "$ref": "#/$defs/AgentEntry"
      }
    },
    "extensions": {
      "type": "array",
      "description": "Registry extensions (reserved)"
    }
  },
  "$defs": {
    "AgentEntry": {
      "type": "object",
      "required": ["id", "name", "version", "distribution"],
      "properties": {
        "id": { "type": "string" },
        "name": { "type": "string" },
        "version": { "type": "string" },
        "description": { "type": "string" },
        "repository": { "type": "string", "format": "uri" },
        "authors": { "type": "array", "items": { "type": "string" } },
        "license": { "type": "string" },
        "icon": { "type": "string", "format": "uri" },
        "distribution": { "$ref": "#/$defs/Distribution" }
      }
    },
    "Distribution": {
      "type": "object",
      "properties": {
        "npx": { "$ref": "#/$defs/NpxDistribution" },
        "binary": { "$ref": "#/$defs/BinaryDistribution" }
      }
    },
    "NpxDistribution": {
      "type": "object",
      "required": ["package"],
      "properties": {
        "package": { "type": "string" },
        "args": { "type": "array", "items": { "type": "string" } },
        "env": { "type": "object", "additionalProperties": { "type": "string" } }
      }
    },
    "BinaryDistribution": {
      "type": "object",
      "properties": {
        "darwin-aarch64": { "$ref": "#/$defs/PlatformBinary" },
        "darwin-x86_64": { "$ref": "#/$defs/PlatformBinary" },
        "linux-aarch64": { "$ref": "#/$defs/PlatformBinary" },
        "linux-x86_64": { "$ref": "#/$defs/PlatformBinary" },
        "windows-aarch64": { "$ref": "#/$defs/PlatformBinary" },
        "windows-x86_64": { "$ref": "#/$defs/PlatformBinary" }
      }
    },
    "PlatformBinary": {
      "type": "object",
      "required": ["archive", "cmd"],
      "properties": {
        "archive": { "type": "string", "format": "uri" },
        "cmd": { "type": "string" },
        "args": { "type": "array", "items": { "type": "string" } }
      }
    }
  }
}
```

### 7.2 Codegen Impact

After creating the schema, run `pnpm run codegen` to generate Rust types in `crates/nexus-contracts/src/generated/` and TypeScript types in `packages/nexus-contracts/src/generated/`. The generated Rust types should be used in `apps/nexus42/src/acp/registry.rs`.

### 7.3 No New Daemon API Schema for V1.0

As decided in §4, no new Daemon API endpoint schema is needed for V1.0. The existing daemon endpoints remain unchanged.

---

## 8. ACP-R1 and ACP-R2 Resolution

### 8.1 ACP-R1: Missing Frozen Capability ID Contract Reference

**Status**: ✅ Resolved in this spec.

**Resolution**: §5.2 defines the frozen capability IDs that nexus42 will declare during ACP `initialize`. The capability set is intentionally minimal for V1.0 (6 capabilities) and can be expanded in V1.1+.

**Implementation action**: The `skills.rs` module in `apps/nexus42/src/acp/` must export these constants and use them when constructing the `initialize` request.

### 8.2 ACP-R2: Missing `nexus42 acp probe` Command

**Status**: ✅ Resolved in this spec.

**Resolution**: §6.2 defines the `nexus42 agent probe` command with two modes:
1. `--registry` (default): Verifies ACP Registry connectivity and reports latency/agent count
2. `--agent <ref>`: Probes a specific agent's ACP handshake (spawn, initialize, report capabilities, terminate)

**Implementation action**: Implemented as part of Task 3 in §10.

---

## 9. Test Strategy

### 9.1 Unit Tests

| Component | Tests | Location |
|-----------|-------|----------|
| `registry.rs` | Cache hit/miss/expiry, parsing, offline fallback | `apps/nexus42/src/acp/registry.rs` (#[cfg(test)]) |
| `skills.rs` | Capability constant correctness, manifest generation | `apps/nexus42/src/acp/skills.rs` (#[cfg(test)]) |
| `transport.rs` | Command construction, platform detection for binary dist | `apps/nexus42/src/acp/transport.rs` (#[cfg(test)]) |
| `error.rs` | Error variant display, conversion | `apps/nexus42/src/acp/error.rs` (#[cfg(test)]) |

### 9.2 Integration Tests

| Test | Description | Location |
|------|-------------|----------|
| Registry fetch | Fetch from CDN, parse, verify schema conformance | `apps/nexus42/tests/acp_registry.rs` |
| Cache roundtrip | Write cache, read back, verify expiry logic | `apps/nexus42/tests/acp_cache.rs` |
| Agent subprocess spawn | Spawn `echo` as fake agent, verify stdio pipe works | `apps/nexus42/tests/acp_transport.rs` |
| CLI command output | `nexus42 agent list --format json`, parse output | `apps/nexus42/tests/cli_agent.rs` |

### 9.3 Test Constraints

- **No real agent tests**: Do not depend on Claude, Codex, or any real agent in CI. Use mock subprocesses.
- **No network in unit tests**: Registry fetch tests should use a local HTTP mock server (or recorded fixtures).
- **Platform-specific**: Binary distribution tests require platform detection; use conditional compilation.

### 9.4 Manual Verification Checklist

```bash
# 1. Registry fetch
nexus42 agent list

# 2. Agent details
nexus42 agent show claude-acp

# 3. Probe registry
nexus42 agent probe --registry

# 4. Run agent (if npx available)
nexus42 agent run claude-acp -m "hello"

# 5. Verify cache
cat ~/.nexus42/registry/cache_meta.json
```

---

## 10. Refined Task Breakdown

> **Historical record:** Tasks 1–5 below preserve the original V1.0 plan.
> Current crate homes, SDK pin, trait surface, provider wiring, and worker
> delegation are defined by the amendment at the top of this document and
> by the current source tree.

### Task 1: ACP SDK Dependency + Adapter Trait (historical V1.0 plan)

**Scope**: Add `agent-client-protocol` crate, implement `NexusAcpClient` adapter trait.

**Files to create:**
- `apps/nexus42/src/acp/mod.rs`
- `apps/nexus42/src/acp/client.rs`
- `apps/nexus42/src/acp/error.rs`

**Files to modify:**
- `apps/nexus42/Cargo.toml` — originally planned `agent-client-protocol = "=0.10.4"` dependency; current pin is `=0.11.1` in `crates/nexus-acp-host/Cargo.toml`
- `apps/nexus42/src/main.rs` — add `mod acp;` and `Agent` command variant
- `apps/nexus42/src/commands/mod.rs` — add `pub mod agent;`

**Acceptance criteria:**
- [ ] `cargo build -p nexus42` succeeds with the ACP SDK dependency
- [ ] The planned V1.0 `NexusAcpClient` surface included `initialize()`, `create_session()`, `prompt()`, and `cancel()`; the current trait definition is authoritative and also supports streaming/configuration operations
- [ ] `AcpError` enum covers: connection failed, timeout, protocol error, agent crashed, not installed
- [ ] `AcpSdkAdapter` struct wraps the SDK's `Client` trait implementation
- [ ] Unit tests for error types pass

**Effort**: S — single focused module, well-understood SDK pattern

---

### Task 2: Registry Manifest Fetcher + Cache

**Scope**: Fetch registry from CDN, parse manifests, implement local caching with stale-while-revalidate.

**Files to create:**
- `apps/nexus42/src/acp/registry.rs`
- `schemas/acp-runtime/registry-manifest.schema.json`

**Acceptance criteria:**
- [ ] `RegistryCache` fetches from `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`
- [ ] Cache stored at `$HOME/.nexus42/registry/cache.json`
- [ ] 24-hour max age with stale-while-revalidate
- [ ] Offline fallback: returns cached data when network unavailable
- [ ] Registry manifest JSON Schema created and valid
- [ ] Unit tests: cache hit, cache miss, cache expired, offline fallback, parse valid/invalid manifests

**Effort**: M — involves HTTP fetching, file I/O, cache logic, schema creation

---

### Task 3: Agent CLI Commands (list, show, probe)

**Scope**: Implement `nexus42 agent list`, `nexus42 agent show <ref>`, `nexus42 agent probe`. Resolves ACP-R2.

**Files to create:**
- `apps/nexus42/src/commands/agent.rs`

**Acceptance criteria:**
- [ ] `nexus42 agent list` displays table of 16 agents
- [ ] `nexus42 agent list --format json` outputs valid JSON
- [ ] `nexus42 agent show claude-acp` displays full agent details
- [ ] `nexus42 agent show claude` works (partial match)
- [ ] `nexus42 agent probe --registry` verifies CDN connectivity
- [ ] `nexus42 agent probe --agent <ref>` spawns agent, performs initialize handshake

**Effort**: M — three commands with formatting, partial matching, and probe logic

---

### Task 4: Agent Subprocess Transport + `agent run`

**Scope**: Implement subprocess spawning, stdio pipe management, and the `nexus42 agent run` command.

**Files to create:**
- `apps/nexus42/src/acp/transport.rs`

**Acceptance criteria:**
- [ ] `nexus42 agent run claude-acp` spawns agent subprocess via npx
- [ ] ACP `initialize` handshake completes
- [ ] Interactive prompt loop works
- [ ] `--message <msg>` flag sends single message and exits
- [ ] Graceful shutdown on Ctrl+C
- [ ] `tokio::task::LocalSet` used correctly for `!Send` futures

**Effort**: L — complex lifecycle management, interactive I/O, multiple error paths

---

### Task 5: Skills/Capability Export

**Scope**: Define frozen capability IDs, declare them during ACP `initialize`. Resolves ACP-R1.

**Files to create:**
- `apps/nexus42/src/acp/skills.rs`

**Acceptance criteria:**
- [ ] `capabilities` module exports frozen IDs
- [ ] `initialize` request includes the V1.0 capability set

**Effort**: XS — constants + wiring, straightforward once Task 1 is complete

---

### Implementation Order

```
Task 1 (SDK + Adapter) ← no dependencies
    │
    ├─→ Task 2 (Registry + Cache) ← depends on error.rs from Task 1
    │       │
    │       └─→ Task 3 (CLI Commands) ← depends on registry.rs from Task 2
    │               │
    │               └─→ Task 6 (Tests) ← depends on all above
    │
    └─→ Task 4 (Transport + Run) ← depends on client.rs from Task 1
            │
            └─→ Task 5 (Skills) ← depends on client.rs from Task 1 (can parallel with Task 4)
                    │
                    └─→ Task 6 (Tests) ← final
```

**Parallelism**: Tasks 2 and 4 can proceed in parallel after Task 1. Task 5 can proceed in parallel with Task 4. Task 6 is the final integration point.

---

## Appendix A: ACP Protocol Lifecycle Reference

For implementer reference, the ACP protocol lifecycle:

1. **initialize**: Client sends `initialize` with capabilities. Agent responds with its capabilities.
2. **authenticate** (optional): If agent requires authentication.
3. **session/new** or **session/load**: Create or resume a session.
4. **session/prompt**: Send user message → agent streams response (markdown).
5. **cancel**: Cancel in-progress prompt.

**Agent → Client requests** (nexus42 must handle as client):
- `request_permission`: Agent asks permission to use a tool. V1.0: auto-grant with log warning.
- `fs/write_text_file`, `fs/read_text_file`: Agent reads/writes files. V1.0: auto-grant within workspace.
- `terminal/create`, `terminal/output`, `terminal/release`, `terminal/wait_for_exit`, `terminal/kill`: Terminal management. V1.0: `create`/`output`/`release` only.

## Appendix B: Residual Findings for V1.1+

> **Durable roadmap:** DR-20, DR-21, DR-22 (ACP-R3..R11: daemon-mediated tool access + permission policy, session persistence, `terminal.kill`/`terminal.wait_for_exit`, `slash_commands`, `agent_plan`, persistent skills manifest, binary auto-update, `session.modes`).
>
> Historical V1.0-era framing; ACP hosting now runs in acp-worker child processes — verify before picking up.

