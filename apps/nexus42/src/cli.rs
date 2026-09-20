//! Shared CLI definitions for nexus42.
//!
//! This module contains the `Cli` struct and `Commands` enum so they can be
//! accessed from both the binary entry point (`main.rs`) and library modules
//! (e.g. `system::print_completion` for shell completion generation).

#[cfg(feature = "connect-host")]
use crate::commands::connect::ConnectCommand;
use crate::commands::creator::CreatorCommand;
#[cfg(feature = "legacy-cli")]
use crate::commands::{
    acp::AcpCommand, capability::CapabilityCommand, compute::ComputeCommand, daemon::DaemonCommand,
    daemon_run::DaemonRunArgs, desktop::DesktopCommand, ops::OpsCommand, platform::PlatformCommand,
    preset::PresetCommand, system::SystemCommand,
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
    /// Commands whose stdout is machine-readable data (`ops inspect --output
    /// json`): tracing must be routed to stderr so diagnostics never corrupt
    /// the data stream.
    #[must_use]
    pub const fn is_data_output(&self) -> bool {
        #[cfg(feature = "legacy-cli")]
        {
            matches!(&self.command, Some(Commands::Ops { .. }))
        }
        #[cfg(not(feature = "legacy-cli"))]
        {
            // The `basic-cli` cohort carries no data-output command.
            false
        }
    }
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)] // clap derive requires non-boxed subcommands for flatten
pub enum Commands {
    /// Manage Creator entities (register, pair, credentials, workspace, soul, memory, kb)
    Creator {
        #[command(subcommand)]
        command: CreatorCommand,
    },

    /// Manage the daemon runtime
    #[cfg(feature = "legacy-cli")]
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Connect Host — peer surface for third-party reasoners (world-scoped ops,
    /// gated compute, host-level reads)
    ///
    /// Runs a `spoke-connect` node in a separate OS process: signed-hello
    /// handshake + allowlist + honest `HostCapabilityManifest`; caller
    /// identity is the authenticated session peer (spoke-connect 0.9.2
    /// `InvokeHandlerV2`). Serves `upsert` / `promote` / `relate` /
    /// `check` / `assemble` world-scoped, `compute` under the stored
    /// World/module/Actor gates over the host-local `~/.nexus42/modules/`
    /// store, and the two host-level reads
    /// `tools.nexus.list_observed_peers` / `tools.nexus.list_modules`
    /// (`commands/connect/mod.rs`). Only `project` and unknown ops are
    /// refused (`op_unsupported`). Compiled only
    /// when the `connect-host` feature is enabled.
    #[cfg(feature = "connect-host")]
    Connect {
        #[command(subcommand)]
        command: ConnectCommand,
    },

    /// ACP capability plane (agents, registry, connectivity)
    #[cfg(feature = "legacy-cli")]
    Acp {
        #[command(subcommand)]
        command: AcpCommand,
    },

    /// Compute module authoring loop (V1.170 P0, AR-9) — build, validate,
    /// and install WASM compute modules.
    ///
    /// `build`, `validate`, and `install` are daemon-free (the author loop
    /// needs no runtime). The group carries no `connect-host` feature
    /// dependency — the default daemon graph stays libp2p-free.
    #[cfg(feature = "legacy-cli")]
    Compute {
        #[command(subcommand)]
        command: ComputeCommand,
    },

    /// Capability authoring surface (validate, install) — V1.172 P2
    /// (AR-41): `validate` and `install` are daemon-free (descriptor +
    /// manifest + wasm pairing via `nexus-module-manifest`, AR-39).
    /// No `run`, no `scaffold` (PL-7 — invocation is P1 dispatch; module
    /// scaffolding stays `nexus42 compute` + `modules/_template`). The
    /// group carries no `connect-host` feature dependency.
    ///
    /// Hidden from `--help` for the current release: the V1.35 command-
    /// surface lock (`.mstar/specs/cli-spec.md` §6) fixes the visible
    /// top-level groups to `creator|daemon|acp|platform|system` — same
    /// posture as `preset` (V1.35 lock resolution, AR-41).
    #[command(hide = true)]
    #[cfg(feature = "legacy-cli")]
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },

    /// Manage the Electron desktop bundle (unsigned macOS packaging)
    #[cfg(feature = "legacy-cli")]
    Desktop {
        #[command(subcommand)]
        command: DesktopCommand,
    },

    /// Platform interaction (auth, context, **sync**)
    #[cfg(feature = "legacy-cli")]
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },

    /// System management (diagnostics, config, identity, completion, etc.)
    #[cfg(feature = "legacy-cli")]
    System {
        #[command(subcommand)]
        command: SystemCommand,
    },

    /// Preset strategy surface (list, show, validate, scaffold, trigger, patch)
    ///
    /// Canonical developer-facing preset group (PL-5, AR-24).
    ///
    /// Hidden from `--help` for the current release: the V1.35 command-surface
    /// lock (`.mstar/specs/cli-spec.md` §6) fixes the visible top-level groups
    /// to `creator|daemon|acp|platform|system` — no new parallel top-level
    /// groups. The `preset` group is a deliberate resolution of AR-24 (new
    /// canonical group) vs that lock: callable but not yet advertised.
    #[command(hide = true)]
    #[cfg(feature = "legacy-cli")]
    Preset {
        #[command(subcommand)]
        command: PresetCommand,
    },

    /// Hidden: Internal daemon-run entry point (self-spawned by daemon start)
    #[command(hide = true)]
    #[cfg(feature = "legacy-cli")]
    DaemonRun(DaemonRunArgs),

    /// Hidden: operator daemon-free inspection (V1.182 P1 BL-04) — `ops inspect`
    /// reads the workspace checkpoint store read-only; the V1.35 cli-spec §6
    /// visible-group lock forces hiding (same posture as `preset`).
    #[command(hide = true)]
    #[cfg(feature = "legacy-cli")]
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
