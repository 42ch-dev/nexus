//! Shared CLI definitions for nexus42.
//!
//! This module contains the `Cli` struct and `Commands` enum so they can be
//! accessed from both the binary entry point (`main.rs`) and library modules
//! (e.g. `system::print_completion` for shell completion generation).

#[cfg(feature = "connect-host")]
use crate::commands::connect::ConnectCommand;
#[cfg(feature = "connect-client")]
use crate::commands::mcp::McpCommand;
use crate::commands::{
    acp::AcpCommand, acp_worker::AcpWorkerArgs, capability::CapabilityCommand,
    compute::ComputeCommand, creator::CreatorCommand, daemon::DaemonCommand,
    daemon_run::DaemonRunArgs, desktop::DesktopCommand, host_call::HostCallArgs, ops::OpsCommand,
    platform::PlatformCommand, preset::PresetCommand, sync::SyncCommand, system::SystemCommand,
};
use clap::{Parser, Subcommand};

/// Nexus CLI — creative world-building command-line interface
#[derive(Parser, Debug)]
#[command(
    name = "nexus42",
    version,
    about = "Nexus creative world-building CLI",
    long_about = "Nexus creative world-building CLI — creator-first.\n\n\
        Quick start:\n\
          nexus42 creator workspace init    Set up a new workspace\n\
          nexus42 creator works status      Show your active Work\n\n\
        Platform sync (requires login):\n\
          nexus42 platform sync pull        Pull bundles from platform\n\
          nexus42 platform sync push        Push local changes to platform\n\n\
        Advanced:\n\
          nexus42 daemon schedule --preset <id>  Start a preset-driven workflow",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Output format (text or json)
    // NOTE (qc1 S-3): this GLOBAL flag is a hard text|json gate for every
    // command — clap rejects any other value before dispatch. Future
    // commands needing a different output vocabulary must use a LOCAL arg
    // (the `acp registry list --format` precedent), NOT widen this
    // value_parser.
    #[arg(
        short = 'o',
        long = "output",
        global = true,
        default_value = "text",
        value_parser = ["text", "json"]
    )]
    output_format: String,
}

impl Cli {
    /// Returns whether verbose logging is enabled.
    #[must_use]
    pub const fn verbose(&self) -> bool {
        self.verbose
    }

    /// Returns the output format string.
    #[must_use]
    pub fn output_format(&self) -> &str {
        &self.output_format
    }

    /// Consumes `self` and returns the inner `Commands` enum, if any.
    #[must_use]
    pub fn into_command(self) -> Option<Commands> {
        self.command
    }
    /// Whether the invocation is the MCP stdio bridge (`mcp serve`).
    ///
    /// The child's stdout is the JSON-RPC transport: any tracing output on
    /// stdout corrupts the protocol, so the caller must route logging to
    /// stderr before initializing the subscriber (V1.174 P0 T5, AR-72).
    #[must_use]
    pub const fn is_mcp_serve(&self) -> bool {
        #[cfg(feature = "connect-client")]
        {
            matches!(
                &self.command,
                Some(Commands::Mcp {
                    command: McpCommand::Serve,
                })
            )
        }
        #[cfg(not(feature = "connect-client"))]
        {
            false
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Manage the daemon runtime
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Connect Host (DF-72 N-C2 read half) — peer surface for third-party reasoners
    ///
    /// Runs a `spoke-connect` node in a separate OS process: signed-hello
    /// handshake + allowlist + honest `HostCapabilityManifest`; caller
    /// identity is the authenticated session peer (spoke-connect 0.9.2
    /// `InvokeHandlerV2`). Serves `upsert` / `promote` / `relate` /
    /// `check` / `assemble` with fail-closed world scoping; `compute` /
    /// `project` / unknown ops are refused. Compiled only
    /// when the `connect-host` feature is enabled.
    #[cfg(feature = "connect-host")]
    Connect {
        #[command(subcommand)]
        command: ConnectCommand,
    },

    /// ACP capability plane (agents, registry, connectivity)
    Acp {
        #[command(subcommand)]
        command: AcpCommand,
    },

    /// Compute module authoring loop (V1.170 P0, AR-9) — build, validate,
    /// install, and run WASM compute modules.
    ///
    /// `build`, `validate`, and `install` are daemon-free (the author loop
    /// needs no runtime); `run` is a thin HTTP client over
    /// `POST /v1/daemon/compute/run` (+ `--accept`). The group carries no
    /// `connect-host` feature dependency — the default daemon graph stays
    /// libp2p-free.
    Compute {
        #[command(subcommand)]
        command: ComputeCommand,
    },

    /// Capability authoring surface (validate, list, install) — V1.172 P2
    /// (AR-41): `validate` and `install` are daemon-free (descriptor +
    /// manifest + wasm pairing via `nexus-module-manifest`, AR-39); `list`
    /// is a thin HTTP client over
    /// `GET /v1/daemon/orchestration/capabilities` (AR-40 provenance).
    /// No `run`, no `scaffold` (PL-7 — invocation is P1 dispatch; module
    /// scaffolding stays `nexus42 compute` + `modules/_template`). The
    /// group carries no `connect-host` feature dependency.
    ///
    /// Hidden from `--help` for the current release: the V1.35 command-
    /// surface lock (`.mstar/specs/cli-spec.md` §6) fixes the visible
    /// top-level groups to `creator|daemon|acp|platform|system` — same
    /// posture as `preset` (V1.35 lock resolution, AR-41).
    #[command(hide = true)]
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },

    /// Manage the Tauri desktop shell (build, sign, diagnostics)
    Desktop {
        #[command(subcommand)]
        command: DesktopCommand,
    },

    /// Platform interaction (auth, explore, context, publish, **sync**)
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },

    /// System management (presets, diagnostics, config, identity, etc.)
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },

    /// Preset strategy surface (list, show, validate, scaffold, run, trigger)
    ///
    /// Canonical developer-facing preset group (PL-5, AR-24). `system preset`
    /// remains a working compatibility alias for one release (PL-6).
    ///
    /// Hidden from `--help` for the current release: the V1.35 command-surface
    /// lock (`.mstar/specs/cli-spec.md` §6) fixes the visible top-level groups
    /// to `creator|daemon|acp|platform|system` — no new parallel top-level
    /// groups. The `preset` group is a deliberate resolution of AR-24 (new
    /// canonical group) vs that lock: callable but not yet advertised, same
    /// posture as the deprecated `sync` alias (S-001).
    #[command(hide = true)]
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },

    /// Hidden: deprecated top-level sync alias — use `platform sync` instead.
    /// Kept callable for ≥1 iteration (V1.35) per cli-command-ia.md §5.
    #[command(hide = true)]
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },

    /// Hidden: ACP worker subprocess entry point (daemon-managed)
    #[command(hide = true)]
    AcpWorker(AcpWorkerArgs),

    /// Hidden: Internal daemon-run entry point (self-spawned by daemon start)
    #[command(hide = true)]
    DaemonRun(DaemonRunArgs),
    /// MCP server bridge (V1.174 P0 T5, AR-70/71/72) — tools-only stdio
    /// server; a client spawns `nexus42 mcp serve` as its own stateless
    /// child (Model A). Compiled only with the `connect-client` feature.
    ///
    /// Hidden from `--help`: the V1.35 command-surface lock fixes the
    /// visible top-level groups to `creator|daemon|acp|platform|system`;
    /// this is a machine-invoked child entry point like `acp-worker`.
    #[cfg(feature = "connect-client")]
    #[command(hide = true)]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },

    /// Debug-only: invoke a host tool through the daemon registry
    ///
    /// Low-level debugging entry. Sends a raw tool execution request to the
    /// daemon's host tool executor. Admission gates apply identically as for
    /// HTTP and worker caller paths.
    ///
    /// --args accepts a `JSON` string (e.g. `'{"work_id":"wrk_abc"}'`).
    /// Exit codes: 0=success, 1=admission denied, 2=tool error/failure.
    HostCall(HostCallArgs),

    /// Hidden: operator daemon-free inspection (V1.182 P1 BL-04) — `ops inspect`
    /// reads the workspace checkpoint store read-only; the V1.35 cli-spec §6
    /// visible-group lock forces hiding (same posture as `preset`).
    #[command(hide = true)]
    Ops {
        #[command(subcommand)]
        command: OpsCommand,
    },
}

/// Build the full `nexus42` clap `Command` for completion generation.
///
/// This is used by `system completion` to produce shell completion scripts.
#[must_use]
pub fn build_command() -> clap::Command {
    <Cli as clap::CommandFactory>::command()
}
