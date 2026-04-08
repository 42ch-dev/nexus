# ACP Client Integration — Technical Specification v1

**Source Plan**: `2025-04-05-acp-client`
**Author**: @architect
**Date**: 2026-04-06
**Status**: Active — authoritative input for Plan 12 (`2026-04-09-v1.1-acp-ux-permissions.md`)
**Supersedes**: Sections 3.3 of V1.0-phase1 Architecture Review

---

## Table of Contents

1. [SDK Selection Decision](#1-sdk-selection-decision)
2. [Integration Architecture](#2-integration-architecture)
3. [Registry Integration Detailed Design](#3-registry-integration-detailed-design)
4. [Local API Contract Analysis](#4-local-api-contract-analysis)
5. [Skills / Capability Export](#5-skills--capability-export)
6. [CLI Command Detailed Design](#6-cli-command-detailed-design)
7. [Schema Definitions](#7-schema-definitions)
8. [ACP-R1 and ACP-R2 Resolution](#8-acp-r1-and-acp-r2-resolution)
9. [Test Strategy](#9-test-strategy)
10. [Refined Task Breakdown](#10-refined-task-breakdown)

---

## 1. SDK Selection Decision

### 1.1 Candidates

| Criterion | `agent-client-protocol` v0.10.4 | `sacp` v10.1.0 / v11.0.0-alpha |
|-----------|-------------------------------|-------------------------------|
| **Publisher** | Zed Industries | Niko Matsakis |
| **Stability** | Stable release (crates.io) | Rapid iteration (10 breaking releases in 3 months) |
| **API Style** | Trait-based (`Client` trait, `Agent` trait) | Builder-based (`ClientToAgent::builder().run_until()`) |
| **Transport** | stdio (JSON-RPC 2.0) | stdio (JSON-RPC 2.0) |
| **Runtime** | Requires `tokio::task::LocalSet` + `spawn_local` (futures are `!Send`) | Likely similar constraint (unconfirmed) |
| **Production Users** | Zed Editor, Block/Goose | Unknown production adopters |
| **Registry Future** | Current official crate | To be renamed as `agent-client-protocol` v1.0 |
| **Ecosystem Fit** | Official ACP docs recommend this crate | Forward-looking, but alpha-quality API |

### 1.2 Decision: **`agent-client-protocol` v0.10.4 (Recommended)**

**Rationale:**

1. **Stability**: v0.10.4 is a published stable release on crates.io. The `sacp` crate has had 10 breaking releases in 3 months — adopting it means constant churn for a V1.0 product that must ship.
2. **Documentation**: Official ACP docs at `agentclientprotocol.com/libraries/rust` reference `agent-client-protocol` as the crate name. It has documented `Client` and `Agent` traits with runnable examples.
3. **Production Validation**: Used by Zed Editor and Block/Goose — real-world battle testing against actual agents in the registry.
4. **Upgrade Path**: When `sacp` becomes `agent-client-protocol` v1.0, we can evaluate migration. The adapter layer (see §2.2) isolates the SDK behind a thin trait, making the swap manageable.
5. **V1.0 Priority**: We need a working integration, not the most advanced API. Stability > feature completeness for V1.0.

**Risk Mitigation**:
- Wrap all SDK usage behind a `NexusAcpClient` trait in `crates/nexus42/src/acp/client.rs` (adapter pattern).
- Pin exact version in `Cargo.toml`: `agent-client-protocol = "=0.10.4"`.
- If the SDK becomes unmaintained or the v1.0 `sacp` rename happens before our GA, the adapter layer limits the migration surface.

### 1.3 Alternative: Deferred (if SDK proves insufficient)

If `agent-client-protocol` v0.10.4 lacks a critical feature (e.g., missing `session/load` support, broken `request_permission` handling), the fallback is to implement the JSON-RPC 2.0 stdio transport directly. The ACP protocol spec is public and well-documented — a thin `tokio::process::Command` + JSON-RPC client would suffice for V1.0's scope.

**This fallback is NOT recommended unless a concrete gap is discovered.** The SDK handles framing, notification multiplexing, and protocol versioning — all error-prone to re-implement.

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
│                   nexus42d Daemon                       │
│                                                         │
│  ┌──────────┐   ┌──────────┐   ┌──────────────────┐    │
│  │ HTTP     │──▶│ Handlers │──▶│ WorkspaceState   │    │
│  │ Router   │   │          │   │ (SQLite)         │    │
│  │ (axum)   │   │ + ACP    │   └──────────────────┘    │
│  └──────────┘   │ proxy    │                           │
│                 │ routes   │   (nexus42d is NOT an     │
│                 └──────────┘    ACP Agent/Server)       │
└─────────────────────────────────────────────────────────┘
```

### 2.2 Module Layout

All ACP-related code lives in `crates/nexus42/src/acp/`:

```
crates/nexus42/src/acp/
├── mod.rs          # Public API, re-exports
├── client.rs       # NexusAcpClient trait + AcpSdkAdapter impl
├── registry.rs     # Registry manifest fetcher + local cache
├── skills.rs       # Capability set definition + skills manifest
├── error.rs        # ACP-specific error types
└── transport.rs    # Subprocess spawn + stdio pipe management
```

New CLI command module:
```
crates/nexus42/src/commands/
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
- **Daemon relationship**: `nexus42d` is **NOT** involved in the ACP communication path. The CLI spawns and talks to agents directly. The daemon may expose Local API endpoints that agents can call (via `request_permission` tool grants), but this is V1.1+ scope.

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
|----------|----------|
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

## 4. Local API Contract Analysis

### 4.1 Question: Does nexus42 need a Local API for agent communication?

**Short answer: No for V1.0. Direct stdio between CLI and agent.**

**Analysis:**

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **A: Direct stdio** (Recommended) | CLI spawns agent, communicates via stdin/stdout JSON-RPC | Simple, matches ACP spec, no extra infra | Agent cannot access daemon services |
| **B: Daemon-mediated** | CLI → daemon HTTP → agent stdio | Centralized, daemon can enforce policies | Adds latency, complexity, violates "nexus42d is not ACP server" |
| **C: Local API as tool server** | Agent calls Local API for workspace/file access | Rich tool access | V1.1+ scope, requires tool permission handling |

**Decision: Option A for V1.0.**

The ACP protocol is designed for direct stdio communication. The existing `DaemonClient` in `crates/nexus42/src/api/daemon_client.rs` provides HTTP access to the daemon for CLI-internal use (health checks, sync, etc.), but agents do NOT talk to the daemon in V1.0.

### 4.2 V1.0 Local API Additions (Minimal)

No new Local API endpoints are required for V1.0 ACP integration. The existing daemon endpoints (`/v1/local/runtime/health`, `/v1/local/workspace`, etc.) are sufficient for CLI use.

### 4.3 V1.1+ Local API Expansion (Deferred)

The following endpoints may be added in V1.1+ to support agent tool access:

| Endpoint | Purpose | Deferred Reason |
|----------|---------|-----------------|
| `POST /v1/local/acp/tool/grant` | Grant tool permission for agent | Requires permission policy engine |
| `POST /v1/local/acp/tool/deny` | Deny tool permission | Requires UI for permission prompts |
| `GET /v1/local/acp/sessions` | List active agent sessions | Requires session persistence |
| `DELETE /v1/local/acp/sessions/{id}` | Terminate an agent session | Requires session management |

**These are documented for future reference but NOT part of the V1.0 task breakdown.**

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
| `terminal.kill` | Client can kill terminal sessions | **No** — deferred to V1.1 |
| `terminal.wait_for_exit` | Client can wait for terminal exit | **No** — deferred to V1.1 |
| `slash_commands` | Client supports slash command invocation | **No** — deferred to V1.1 |
| `agent_plan` | Client supports agent plan display | **No** — deferred to V1.1 |
| `session.modes` | Client supports mode switching (e.g., ask/act) | **No** — deferred to V1.1 |

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

For V1.0, nexus42 does NOT export a formal skills manifest file. The capabilities are declared dynamically during `initialize`. A persistent skills manifest (`$HOME/.nexus42/skills.json`) can be added in V1.1+ for multi-agent host integration.

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
// In crates/nexus42/src/commands/mod.rs — add:
pub mod agent;

// In crates/nexus42/src/main.rs — add to Commands enum:
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
        "id": {
          "type": "string",
          "description": "Unique agent identifier"
        },
        "name": {
          "type": "string",
          "description": "Human-readable agent name"
        },
        "version": {
          "type": "string",
          "description": "Agent version"
        },
        "description": {
          "type": "string",
          "description": "Agent description"
        },
        "repository": {
          "type": "string",
          "format": "uri",
          "description": "Agent source repository URL"
        },
        "authors": {
          "type": "array",
          "items": { "type": "string" }
        },
        "license": {
          "type": "string",
          "description": "Agent license identifier"
        },
        "icon": {
          "type": "string",
          "format": "uri",
          "description": "Agent icon URL"
        },
        "distribution": {
          "$ref": "#/$defs/Distribution"
        }
      }
    },
    "Distribution": {
      "type": "object",
      "description": "Agent distribution configuration",
      "properties": {
        "npx": {
          "$ref": "#/$defs/NpxDistribution"
        },
        "binary": {
          "$ref": "#/$defs/BinaryDistribution"
        }
      }
    },
    "NpxDistribution": {
      "type": "object",
      "required": ["package"],
      "properties": {
        "package": {
          "type": "string",
          "description": "npm package name with optional version (e.g. @scope/pkg@1.0.0)"
        },
        "args": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Additional CLI arguments"
        },
        "env": {
          "type": "object",
          "additionalProperties": { "type": "string" },
          "description": "Environment variables to set"
        }
      }
    },
    "BinaryDistribution": {
      "type": "object",
      "description": "Per-platform binary distribution",
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
        "archive": {
          "type": "string",
          "format": "uri",
          "description": "Download URL for platform-specific archive"
        },
        "cmd": {
          "type": "string",
          "description": "Command to execute within the archive"
        },
        "args": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Additional CLI arguments"
        }
      }
    }
  }
}
```

### 7.2 Codegen Impact

After creating the schema, run `pnpm run codegen` to generate Rust types in `crates/nexus-contracts/src/generated/` and TypeScript types in `packages/nexus-contracts/src/generated/`. The generated Rust types should be used in `crates/nexus42/src/acp/registry.rs`.

### 7.3 No New Local API Schema for V1.0

As decided in §4, no new Local API endpoint schema is needed for V1.0. The existing daemon endpoints remain unchanged.

---

## 8. ACP-R1 and ACP-R2 Resolution

### 8.1 ACP-R1: Missing Frozen Capability ID Contract Reference

**Status**: ✅ Resolved in this spec.

**Resolution**: §5.2 defines the frozen capability IDs that nexus42 will declare during ACP `initialize`. The capability set is intentionally minimal for V1.0 (6 capabilities) and can be expanded in V1.1+.

**Implementation action**: The `skills.rs` module in `crates/nexus42/src/acp/` must export these constants and use them when constructing the `initialize` request.

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
| `registry.rs` | Cache hit/miss/expiry, parsing, offline fallback | `crates/nexus42/src/acp/registry.rs` (#[cfg(test)]) |
| `skills.rs` | Capability constant correctness, manifest generation | `crates/nexus42/src/acp/skills.rs` (#[cfg(test)]) |
| `transport.rs` | Command construction, platform detection for binary dist | `crates/nexus42/src/acp/transport.rs` (#[cfg(test)]) |
| `error.rs` | Error variant display, conversion | `crates/nexus42/src/acp/error.rs` (#[cfg(test)]) |

### 9.2 Integration Tests

| Test | Description | Location |
|------|-------------|----------|
| Registry fetch | Fetch from CDN, parse, verify schema conformance | `crates/nexus42/tests/acp_registry.rs` |
| Cache roundtrip | Write cache, read back, verify expiry logic | `crates/nexus42/tests/acp_cache.rs` |
| Agent subprocess spawn | Spawn `echo` as fake agent, verify stdio pipe works | `crates/nexus42/tests/acp_transport.rs` |
| CLI command output | `nexus42 agent list --format json`, parse output | `crates/nexus42/tests/cli_agent.rs` |

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

### Task 1: ACP SDK Dependency + Adapter Trait

**Scope**: Add `agent-client-protocol` crate, implement `NexusAcpClient` adapter trait.

**Files to create:**
- `crates/nexus42/src/acp/mod.rs`
- `crates/nexus42/src/acp/client.rs`
- `crates/nexus42/src/acp/error.rs`

**Files to modify:**
- `crates/nexus42/Cargo.toml` — add `agent-client-protocol = "=0.10.4"` dependency
- `crates/nexus42/src/main.rs` — add `mod acp;` and `Agent` command variant
- `crates/nexus42/src/commands/mod.rs` — add `pub mod agent;`

**Acceptance criteria:**
- [ ] `cargo build -p nexus42` succeeds with the ACP SDK dependency
- [ ] `NexusAcpClient` trait is defined with methods: `initialize()`, `create_session()`, `prompt()`, `cancel()`
- [ ] `AcpError` enum covers: connection failed, timeout, protocol error, agent crashed, not installed
- [ ] `AcpSdkAdapter` struct wraps the SDK's `Client` trait implementation
- [ ] Unit tests for error types pass

**Effort**: S — single focused module, well-understood SDK pattern

---

### Task 2: Registry Manifest Fetcher + Cache

**Scope**: Fetch registry from CDN, parse manifests, implement local caching with stale-while-revalidate.

**Files to create:**
- `crates/nexus42/src/acp/registry.rs`
- `schemas/acp-runtime/registry-manifest.schema.json`

**Files to modify:**
- None (registry.rs is self-contained)

**Acceptance criteria:**
- [ ] `RegistryCache` fetches from `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`
- [ ] Cache stored at `$HOME/.nexus42/registry/cache.json` and `$HOME/.nexus42/registry/cache_meta.json`
- [ ] 24-hour max age with stale-while-revalidate (return cached, fetch in background)
- [ ] Offline fallback: returns cached data when network unavailable
- [ ] Registry manifest JSON Schema created and valid
- [ ] After `pnpm run codegen`, generated Rust types used in registry.rs (not hand-written)
- [ ] Unit tests: cache hit, cache miss, cache expired, offline fallback, parse valid/invalid manifests

**Effort**: M — involves HTTP fetching, file I/O, cache logic, schema creation

---

### Task 3: Agent CLI Commands (list, show, probe)

**Scope**: Implement `nexus42 agent list`, `nexus42 agent show <ref>`, `nexus42 agent probe`. Resolves ACP-R2.

**Files to create:**
- `crates/nexus42/src/commands/agent.rs`

**Files to modify:**
- `crates/nexus42/src/main.rs` — add `Agent` variant to `Commands` enum, wire to `commands::agent::run`
- `crates/nexus42/src/commands/mod.rs` — add `pub mod agent;`

**Acceptance criteria:**
- [ ] `nexus42 agent list` displays table of 16 agents with id, version, source, description
- [ ] `nexus42 agent list --format json` outputs valid JSON matching schema
- [ ] `nexus42 agent show claude-acp` displays full agent details
- [ ] `nexus42 agent show claude` works (partial match on id or name)
- [ ] `nexus42 agent probe --registry` verifies CDN connectivity, reports latency and agent count
- [ ] `nexus42 agent probe --agent <ref>` spawns agent, performs initialize handshake, reports capabilities
- [ ] Error handling: unknown agent-ref produces helpful error message
- [ ] Integration test: `nexus42 agent list --format json` parses as valid JSON

**Effort**: M — three commands with formatting, partial matching, and probe logic

---

### Task 4: Agent Subprocess Transport + `agent run`

**Scope**: Implement subprocess spawning, stdio pipe management, and the `nexus42 agent run` command.

**Files to create:**
- `crates/nexus42/src/acp/transport.rs`

**Files to modify:**
- `crates/nexus42/src/commands/agent.rs` — add `Run` subcommand handler
- `crates/nexus42/src/acp/client.rs` — wire subprocess transport to SDK adapter

**Acceptance criteria:**
- [ ] `nexus42 agent run claude-acp` spawns agent subprocess via npx
- [ ] `nexus42 agent run codex-acp` downloads and caches binary, spawns from cache
- [ ] ACP `initialize` handshake completes (capabilities exchange)
- [ ] `session/new` creates a new session
- [ ] Interactive prompt loop: user input → `session/prompt` → stream agent response
- [ ] `--message <msg>` flag sends single message and exits
- [ ] `--cwd <path>` sets working directory for agent subprocess
- [ ] Graceful shutdown on Ctrl+C: cancel notification → wait 5s → SIGTERM
- [ ] Error cases: agent not found, agent crashes, timeout, npx not available
- [ ] `tokio::task::LocalSet` used correctly for `!Send` futures from SDK
- [ ] Integration test: spawn `echo` as mock agent, verify stdio pipe communication

**Effort**: L — complex lifecycle management, interactive I/O, multiple error paths, platform-specific binary handling

---

### Task 5: Skills/Capability Export

**Scope**: Define frozen capability IDs, declare them during ACP `initialize`. Resolves ACP-R1.

**Files to create:**
- `crates/nexus42/src/acp/skills.rs`

**Files to modify:**
- `crates/nexus42/src/acp/client.rs` — include capabilities in initialize request

**Acceptance criteria:**
- [ ] `capabilities` module exports frozen IDs: `FILE_SYSTEM_READ`, `FILE_SYSTEM_WRITE`, `TERMINAL_CREATE`, `TERMINAL_OUTPUT`, `TERMINAL_RELEASE`
- [ ] `initialize` request includes the V1.0 capability set
- [ ] Unit tests verify capability constant values match ACP spec
- [ ] Document which capabilities are deferred to V1.1+ (with rationale)

**Effort**: XS — constants + wiring, straightforward once Task 1 is complete

---

### Task 6: Integration Tests + CI Verification

**Scope**: End-to-end tests for the ACP integration, CI verification.

**Files to create:**
- `crates/nexus42/tests/acp_registry.rs`
- `crates/nexus42/tests/acp_cache.rs`
- `crates/nexus42/tests/acp_transport.rs`
- `crates/nexus42/tests/cli_agent.rs`

**Files to modify:**
- `.github/workflows/ci.yml` — add ACP-related checks (schema validation for `schemas/acp-runtime/`)

**Acceptance criteria:**
- [ ] All integration tests pass
- [ ] `cargo test -p nexus42` passes (including new acp module tests)
- [ ] `cargo clippy -p nexus42 -- -D warnings` clean
- [ ] `cargo +nightly fmt --all -- --check` clean
- [ ] Schema `schemas/acp-runtime/registry-manifest.schema.json` passes validation
- [ ] `pnpm run codegen && git diff --exit-code` clean (generated types in sync)
- [ ] Manual: `nexus42 agent list` works on fresh clone with no cache

**Effort**: S — test harness is boilerplate; most complexity is in the test scenarios from Tasks 1–4

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

| ID | Title | Severity | Target |
|----|-------|----------|--------|
| ACP-R3 | Terminal kill/wait_for_exit capability | low | V1.1 |
| ACP-R4 | Slash commands UI integration | low | V1.1 |
| ACP-R5 | Agent plan display support | low | V1.1 |
| ACP-R6 | Session persistence across CLI invocations | medium | V1.1 |
| ACP-R7 | Permission policy engine (grant/deny UI) | medium | V1.1 |
| ACP-R8 | Daemon-mediated agent tool access | medium | V1.1 |
| ACP-R9 | Skills manifest file for multi-agent hosts | low | V1.1 |
| ACP-R10 | Binary agent auto-update mechanism | low | V1.1 |
| ACP-R11 | Session modes (ask/act) switching | low | V1.1 |

---

*End of technical specification. This document is the authoritative input for implementing the `2025-04-05-acp-client` plan.*
