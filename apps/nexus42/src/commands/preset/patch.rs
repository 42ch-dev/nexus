//! Strategy patch leaves — `preset patch state|transition|prompt`
//! (V1.175 P1 Task 2, group 1).
//!
//! Thin daemon-HTTP leaves over the existing strategy canvas write routes
//! (AR-83 #1 / AR-84 group 1, F-9):
//! - `POST /v1/daemon/strategies/:strategy_id/states/:state_id/patch`
//! - `POST /v1/daemon/strategies/:strategy_id/transitions/patch`
//! - `POST /v1/daemon/strategies/:strategy_id/states/:state_id/prompt/patch`
//!
//! All writes are CAS-guarded: every request carries `--base-revision`
//! (the revision observed on the last canonical read). A stale revision
//! returns 409 `strategy_conflict` with the current revision, the
//! conflicting path, and a recovery hint — rendered by
//! `DaemonClient::parse_error_response` (all three fields, PL-5). Flock
//! contention between writers rides the same 409 family. `--help`
//! documents the re-read retry guidance.
//!
//! Conventions: human-readable default output, `--json` emits the daemon
//! `StrategyPatchResponse` DTO verbatim (generated contract types only —
//! AR-83 #2/#3); write bodies are typed long flags; prompt bodies come
//! from `--file <path>` or `-` for stdin.

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::canvas::strategy::{
    StrategyPatchPromptTemplateRequest, StrategyPatchPromptTemplateRequestSet,
    StrategyPatchResponse, StrategyPatchStateRequest, StrategyPatchStateRequestSet,
    StrategyPatchTransitionRequest, StrategyPatchTransitionRequestOp,
    StrategyPatchTransitionRequestTransitionKind,
};

/// `preset patch` subcommands.
#[derive(Debug, Subcommand)]
pub enum PatchCommand {
    /// Patch a state node (rename via --label, or update --description).
    ///
    /// CAS-guarded: `--base-revision` must match the Strategy's current
    /// revision. On 409 `strategy_conflict`, re-read the Strategy and
    /// reapply with the new revision.
    State {
        /// Strategy / user preset ID (bundle directory name).
        strategy_id: String,
        /// State ID to patch.
        state_id: String,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `strategy_conflict`, re-read the Strategy (e.g. `preset show`)
        /// and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// New state id — renames the state and rewrites all references
        /// (next targets, initial, branches).
        #[arg(long)]
        label: Option<String>,
        /// New state description.
        #[arg(long)]
        description: Option<String>,
        /// Emit machine-readable JSON (the `StrategyPatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Rewire a transition (create or update).
    ///
    /// CAS-guarded: `--base-revision` must match the Strategy's current
    /// revision. On 409 `strategy_conflict`, re-read the Strategy and
    /// reapply with the new revision.
    Transition {
        /// Strategy / user preset ID (bundle directory name).
        strategy_id: String,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `strategy_conflict`, re-read the Strategy (e.g. `preset show`)
        /// and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// Source state ID whose outgoing transition is patched.
        #[arg(long, value_name = "STATE_ID")]
        source_state: String,
        /// Operation: `create` or `update` (default: `update`).
        #[arg(long, value_enum, default_value_t = TransitionOpArg::Update)]
        op: TransitionOpArg,
        /// Old target state (required for `--op update`).
        #[arg(long, value_name = "STATE_ID")]
        old_target: Option<String>,
        /// New target state (required for `--op create`).
        #[arg(long, value_name = "STATE_ID")]
        new_target: Option<String>,
        /// Branch condition expression (e.g. `_context._judge_result == true`).
        #[arg(long)]
        condition: Option<String>,
        /// Create form: `next` (linear), `branch` (conditional rule), or
        /// `default` (conditional default target).
        #[arg(long, value_enum)]
        transition_kind: Option<TransitionKindArg>,
        /// Emit machine-readable JSON (the `StrategyPatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Patch a state's prompt template.
    ///
    /// CAS-guarded: `--base-revision` must match the Strategy's current
    /// revision. On 409 `strategy_conflict`, re-read the Strategy and
    /// reapply with the new revision.
    Prompt {
        /// Strategy / user preset ID (bundle directory name).
        strategy_id: String,
        /// State ID whose prompt template is patched.
        state_id: String,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `strategy_conflict`, re-read the Strategy (e.g. `preset show`)
        /// and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// Template path inside the bundle (e.g. `prompts/start.md`).
        #[arg(long, value_name = "PATH")]
        template_ref: String,
        /// Template body source: a file path, or `-` to read stdin.
        #[arg(long, value_name = "PATH")]
        file: String,
        /// Emit machine-readable JSON (the `StrategyPatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `--op` value for transition patches.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TransitionOpArg {
    /// Insert a new outgoing transition.
    Create,
    /// Rewire an existing outgoing transition.
    Update,
}

/// `--transition-kind` value for transition creates.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TransitionKindArg {
    /// Linear scalar `next` target.
    Next,
    /// Conditional `rules` branch.
    Branch,
    /// Conditional `default` target.
    Default,
}

impl TransitionOpArg {
    const fn to_generated(self) -> StrategyPatchTransitionRequestOp {
        match self {
            Self::Create => StrategyPatchTransitionRequestOp::Create,
            Self::Update => StrategyPatchTransitionRequestOp::Update,
        }
    }
}

impl TransitionKindArg {
    const fn to_generated(self) -> StrategyPatchTransitionRequestTransitionKind {
        match self {
            Self::Next => StrategyPatchTransitionRequestTransitionKind::Next,
            Self::Branch => StrategyPatchTransitionRequestTransitionKind::Branch,
            Self::Default => StrategyPatchTransitionRequestTransitionKind::Default,
        }
    }
}

/// Run a `preset patch` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (missing required flags) or any
/// daemon API / network failure (409 `strategy_conflict`, 400
/// `strategy_invalid`, 404, … — all named, non-zero exit).
pub async fn run(cmd: PatchCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        PatchCommand::State {
            strategy_id,
            state_id,
            base_revision,
            label,
            description,
            json,
        } => {
            patch_state(
                &client,
                &strategy_id,
                &state_id,
                base_revision,
                label.as_deref(),
                description.as_deref(),
                json,
            )
            .await
        }
        PatchCommand::Transition {
            strategy_id,
            base_revision,
            source_state,
            op,
            old_target,
            new_target,
            condition,
            transition_kind,
            json,
        } => {
            patch_transition(
                &client,
                &strategy_id,
                base_revision,
                &source_state,
                op,
                old_target.as_deref(),
                new_target.as_deref(),
                condition.as_deref(),
                transition_kind,
                json,
            )
            .await
        }
        PatchCommand::Prompt {
            strategy_id,
            state_id,
            base_revision,
            template_ref,
            file,
            json,
        } => {
            patch_prompt(
                &client,
                &strategy_id,
                &state_id,
                base_revision,
                &template_ref,
                &file,
                json,
            )
            .await
        }
    }
}

/// `preset patch state <strategy_id> <state_id> --base-revision N
/// [--label <id>] [--description <text>]` — patch a state node
/// (`POST /v1/daemon/strategies/:strategy_id/states/:state_id/patch`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when neither `--label` nor
/// `--description` is given, or `CliError` for daemon / network failures
/// (409 `strategy_conflict`, 400 `strategy_invalid`, 404, …).
async fn patch_state(
    client: &DaemonClient,
    strategy_id: &str,
    state_id: &str,
    base_revision: u64,
    label: Option<&str>,
    description: Option<&str>,
    json: bool,
) -> Result<()> {
    if label.is_none() && description.is_none() {
        return Err(CliError::Other(
            "provide at least one of --label or --description".to_string(),
        ));
    }
    let req = StrategyPatchStateRequest {
        strategy_id: strategy_id.to_string(),
        state_id: state_id.to_string(),
        base_revision,
        set: StrategyPatchStateRequestSet {
            label: label.map(str::to_string),
            description: description.map(str::to_string),
        },
    };
    let resp: StrategyPatchResponse = client
        .post(
            &format!("/v1/daemon/strategies/{strategy_id}/states/{state_id}/patch"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Patched state '{state_id}' in Strategy '{strategy_id}'.");
        render_patch_response(&resp);
    }
    Ok(())
}

/// `preset patch transition <strategy_id> --base-revision N --source-state
/// <id> [--op create|update] [--old-target <id>] [--new-target <id>]
/// [--condition <expr>] [--transition-kind next|branch|default]` — rewire
/// a transition (`POST /v1/daemon/strategies/:strategy_id/transitions/patch`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when a required flag for the chosen
/// `--op` is missing, or `CliError` for daemon / network failures (409
/// `strategy_conflict`, 400 `strategy_invalid`, 404, …).
#[allow(clippy::too_many_arguments)] // CLI param plumbing — house pattern
async fn patch_transition(
    client: &DaemonClient,
    strategy_id: &str,
    base_revision: u64,
    source_state: &str,
    op: TransitionOpArg,
    old_target: Option<&str>,
    new_target: Option<&str>,
    condition: Option<&str>,
    transition_kind: Option<TransitionKindArg>,
    json: bool,
) -> Result<()> {
    // CLI-side required-flag checks mirror the daemon's field errors so
    // scripts fail fast with named messages (PL-5).
    match op {
        TransitionOpArg::Create => {
            if new_target.is_none() {
                return Err(CliError::Other(
                    "--new-target is required when --op create".to_string(),
                ));
            }
        }
        TransitionOpArg::Update => {
            if old_target.is_none() {
                return Err(CliError::Other(
                    "--old-target is required when --op update".to_string(),
                ));
            }
        }
    }
    let req = StrategyPatchTransitionRequest {
        strategy_id: strategy_id.to_string(),
        base_revision,
        source_state_id: source_state.to_string(),
        old_target: old_target.map(str::to_string),
        new_target: new_target.map(str::to_string),
        condition: condition.map(str::to_string),
        transition_kind: transition_kind.map(TransitionKindArg::to_generated),
        op: op.to_generated(),
    };
    let resp: StrategyPatchResponse = client
        .post(
            &format!("/v1/daemon/strategies/{strategy_id}/transitions/patch"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Patched transition from '{source_state}' in Strategy '{strategy_id}'.");
        render_patch_response(&resp);
    }
    Ok(())
}

/// `preset patch prompt <strategy_id> <state_id> --base-revision N
/// --template-ref <path> --file <path>|'-'` — patch a state's prompt
/// template (`POST /v1/daemon/strategies/:strategy_id/states/:state_id/prompt/patch`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when `--file` cannot be read, or
/// `CliError` for daemon / network failures (409 `strategy_conflict`, 400
/// `strategy_invalid`, 404, …).
async fn patch_prompt(
    client: &DaemonClient,
    strategy_id: &str,
    state_id: &str,
    base_revision: u64,
    template_ref: &str,
    file: &str,
    json: bool,
) -> Result<()> {
    let body = if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(file)
            .map_err(|e| CliError::Other(format!("cannot read --file '{file}': {e}")))?
    };
    let req = StrategyPatchPromptTemplateRequest {
        strategy_id: strategy_id.to_string(),
        state_id: state_id.to_string(),
        base_revision,
        template_ref: template_ref.to_string(),
        set: StrategyPatchPromptTemplateRequestSet { body },
    };
    let resp: StrategyPatchResponse = client
        .post(
            &format!("/v1/daemon/strategies/{strategy_id}/states/{state_id}/prompt/patch"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "Patched prompt template '{template_ref}' for state '{state_id}' in \
             Strategy '{strategy_id}'."
        );
        render_patch_response(&resp);
    }
    Ok(())
}

/// Render a `StrategyPatchResponse` for human output.
fn render_patch_response(resp: &StrategyPatchResponse) {
    println!("  new_revision: {}", resp.new_revision);
    for effect in &resp.side_effects {
        println!("  {effect}");
    }
    if !resp.validation_summary.warnings.is_empty() {
        println!("  warnings:");
        for warning in &resp.validation_summary.warnings {
            println!("    - {warning}");
        }
    }
}
