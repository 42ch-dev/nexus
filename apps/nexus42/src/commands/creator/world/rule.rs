//! Structured-rule author surface — `creator world rule add|list|deactivate`
//! (V1.166 PD-1 / AR-2 / AR-3, DR-64).
//!
//! The leaves run on the typed core World-rule seam
//! ([`crate::core`]): `create_world_rule` / `list_world_rules` /
//! `update_world_rule`. The core owns World ownership, the closed four-family
//! carrier grammar (member-aware `constraint.<member>` errors), the
//! `rul_<uuid v4 simple>` id minting and the row assembly; this module maps
//! the flags onto the generated request and renders the response.
//!
//! # Spoke vocabulary (core-validated)
//!
//! `kind` (core `rule` / `prohibition` / `style`) and `severity_hint` (core
//! `info` / `warning` / `error`) are open, non-empty strings stored verbatim.
//! `status` is **not** one of them: it is the core's closed `draft` / `active`
//! / `deprecated` grammar (AR-3), so any other value is refused by the core
//! instead of being stored. `statement` is the **human summary only** — it is
//! never parsed by the evaluator (PD-1). Machine evaluation reads
//! `extensions.nexus.constraint` (AR-2 carrier).
//!
//! # Ownership
//!
//! `add` and `deactivate` gate on the core's shared world-owner guard: a
//! foreign or missing World is a named 404/403 refusal, never a silent no-op.
//! `deactivate` is the core's `status = deprecated` update (PD-1 recovery lock
//! — no DELETE route, re-activation is a Non-Goal: authors add a new rule).

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{WorldRuleCreateRequest, WorldRuleResponse, WorldRuleUpdateRequest};
use nexus_core::{CoreService, Principal};
use serde_json::{Map, Value};

/// The spoke status written by `rule deactivate` (PD-1: spoke vocabulary —
/// do **not** invent `inactive`).
const DEPRECATED_STATUS: &str = "deprecated";

/// `creator world rule` subcommands.
#[derive(Debug, Subcommand)]
pub enum RuleCommand {
    /// Add a structured rule to a world (default status: active → auto-included
    /// by the check loop when `rule_refs` is empty; `--status draft` stages)
    Add {
        /// World ID (required, e.g. `wld_abc123`); must be owned by the active creator
        #[arg(long)]
        world_id: String,
        /// Human-stable rule name (`canonical_name`)
        #[arg(long)]
        name: String,
        /// Author classification (open string; core: rule / prohibition / style)
        #[arg(long, default_value = "rule")]
        kind: String,
        /// Human summary for list/UI. **Not** evaluated by the checker (PD-1)
        #[arg(long)]
        statement: String,
        /// Checker hint (open string; core: info / warning / error)
        #[arg(long, default_value = "warning")]
        severity: String,
        /// Target entry type (repeatable; empty = all types in check scope).
        /// Rejected alongside an `observer_cardinality` constraint (events
        /// carry no `entry_type` — AR-2).
        #[arg(long)]
        entry_type: Vec<String>,
        /// Rule status (the core's closed grammar: `draft` / `active` /
        /// `deprecated`; any other value is refused, never stored)
        #[arg(long, default_value = "active")]
        status: String,
        /// Structured constraint carrier as a JSON object string (AR-2:
        /// closed shapes; validated member-aware by the core, fail early)
        #[arg(long)]
        constraint: String,
    },

    /// List all rules of a world (all statuses — draft/deprecated included,
    /// so authors see what auto-include will skip)
    List {
        /// World ID (e.g. `wld_abc123`)
        #[arg(long)]
        world_id: String,
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Set a rule's status to `deprecated` (spoke vocabulary; re-activation
    /// is a Non-Goal — authors add a new rule)
    Deactivate {
        /// World ID (e.g. `wld_abc123`)
        #[arg(long)]
        world_id: String,
        /// Rule ID (e.g. `rul_abc123`)
        #[arg(long)]
        rule_id: String,
    },
}

/// Run a `creator world rule` subcommand.
///
/// Opens the direct-writer core, resolves the admitted principal, and closes
/// the writer before this command reports — on success and on failure.
///
/// # Errors
///
/// Returns `CliError` when the active creator/workspace is unset, the carrier
/// fails the core's AR-2 validation (add), or the core refuses the write
/// (ownership, missing rule, storage).
pub async fn run(cmd: RuleCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            RuleCommand::Add {
                world_id,
                name,
                kind,
                statement,
                severity,
                entry_type,
                status,
                constraint,
            } => {
                let rule = rule_add(
                    &core,
                    &principal,
                    &world_id,
                    &name,
                    &kind,
                    &statement,
                    &severity,
                    &entry_type,
                    &status,
                    &constraint,
                )
                .await?;
                Ok(Some(render_rule_add(&world_id, &rule)))
            }
            RuleCommand::List { world_id, json } => {
                rule_list(&core, &principal, &world_id, json).await
            }
            RuleCommand::Deactivate { world_id, rule_id } => {
                rule_deactivate(&core, &principal, &world_id, &rule_id).await
            }
        }
    }
    .await;
    // The leaves return their report; nothing reaches stdout until the shared
    // seam released the writer, so a refused or unsettleable write is never
    // reported as an added/listed/deactivated rule.
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

// ── Leaf logic ────────────────────────────────────────────────────────
//
// These take the open `&CoreService` plus its admitted `&Principal` so
// integration tests can drive them against a hermetic direct-core home
// without re-opening the writer (same seam as `world/kb/service.rs`).

/// Parse the AR-2 `--constraint` argument into the wire carrier map.
///
/// Only the shape the generated request requires is checked here — a JSON
/// **object**. The closed four-family carrier grammar stays owned by the
/// spoke adapter behind the core seam, so a malformed carrier is still
/// rejected member-aware (`constraint.<member>`), never by a second parser in
/// the CLI.
fn parse_constraint_object(constraint_json: &str) -> Result<Map<String, Value>> {
    let value: Value = serde_json::from_str(constraint_json)
        .map_err(|e| CliError::Other(format!("--constraint: invalid JSON: {e}")))?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(CliError::Other(
            "--constraint: constraint must be a JSON object".to_string(),
        )),
    }
}

/// `creator world rule add` — create a structured rule through the core.
///
/// The core validates the AR-2 carrier (member-aware, fail early), the
/// `observer_cardinality` × `--entry-type` pair, the meta-field values and the
/// AR-1 status set, mints the `rul_<32-hex>` id and inserts the full row.
///
/// Returns the created row (the core's own projection); [`render_rule_add`]
/// renders it for the caller once the writer settled.
///
/// # Errors
///
/// Returns a named `CliError` for a malformed `--constraint` (invalid JSON or
/// a non-object root), the core's `invalid input (constraint.<member>)` for a
/// carrier outside the closed grammar, the core's World-ownership refusal
/// (403) or a storage error.
#[allow(clippy::too_many_arguments)]
// ^ justification: mirrors the flat `rule add` flag surface; grouping the
// ten PD-1 flags into a struct would add indirection for the two callers
// (CLI + tests).
pub async fn rule_add(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    name: &str,
    kind: &str,
    statement: &str,
    severity: &str,
    entry_types: &[String],
    status: &str,
    constraint_json: &str,
) -> Result<WorldRuleResponse> {
    let request = WorldRuleCreateRequest {
        canonical_name: name.to_string(),
        constraint: parse_constraint_object(constraint_json)?,
        kind: Some(kind.to_string()),
        severity_hint: Some(severity.to_string()),
        statement: statement.to_string(),
        status: Some(status.to_string()),
        target_entry_types: entry_types.to_vec(),
    };

    core.create_world_rule(principal, world_id.to_string(), request)
        .await
        .map_err(map_core_error)
}

/// Render one created rule — the human report `run` prints once the writer
/// settled.
fn render_rule_add(world_id: &str, rule: &WorldRuleResponse) -> String {
    let mut lines = vec![
        format!("✓ Rule added: {}", rule.rule_id),
        format!("  World:       {world_id}"),
        format!("  Name:        {}", rule.canonical_name),
        format!("  Kind:        {}", rule.kind),
        format!("  Status:      {}", rule.status.as_deref().unwrap_or("-")),
        format!(
            "  Severity:    {}",
            rule.severity_hint.as_deref().unwrap_or("-")
        ),
    ];
    if !rule.target_entry_types.is_empty() {
        lines.push(format!(
            "  Entry types: {}",
            rule.target_entry_types.join(", ")
        ));
    }
    lines.push(format!(
        "  Constraint:  {}",
        rule.constraint
            .get("family")
            .and_then(Value::as_str)
            .unwrap_or("-")
    ));
    lines.join("\n")
}

/// `creator world rule list` — all rules of a world, **all statuses**
/// (PD-1 list: store order `canonical_name ASC, rule_id ASC` — AR-3).
///
/// `--json` emits the core's `WorldRulesListResponseRulesItem` array
/// verbatim; the human table projects the same fields. Returns the report
/// `run` prints once the writer settled.
///
/// # Errors
///
/// Returns the core's World-ownership refusal (403/404) or a storage error,
/// or `CliError` if the JSON serialization fails.
pub async fn rule_list(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    json: bool,
) -> Result<Option<String>> {
    let response = core
        .list_world_rules(principal, world_id.to_string())
        .await
        .map_err(map_core_error)?;

    if json {
        return Ok(Some(serde_json::to_string_pretty(&response.rules)?));
    }

    if response.rules.is_empty() {
        return Ok(Some(format!("No rules in world {world_id}.")));
    }

    let mut lines = vec![
        format!("Rules in world {world_id}:"),
        format!(
            "{:<24} {:<28} {:<12} {:<10} {:<10} STATEMENT",
            "RULE_ID", "NAME", "KIND", "STATUS", "SEVERITY"
        ),
    ];
    for row in &response.rules {
        lines.push(format!(
            "{:<24} {:<28} {:<12} {:<10} {:<10} {}",
            row.rule_id,
            row.canonical_name,
            row.kind,
            row.status.as_deref().unwrap_or("-"),
            row.severity_hint.as_deref().unwrap_or("-"),
            row.statement.as_deref().unwrap_or(""),
        ));
    }
    if response.truncated {
        lines.push("\n(truncated — more rules exist beyond the 500-row safety cap)".to_string());
    }
    Ok(Some(lines.join("\n")))
}

/// `creator world rule deactivate` — set a rule's status to `deprecated`
/// (spoke vocabulary; PD-1).
///
/// This is the core's `status = deprecated` update: the World-ownership guard
/// runs first and a `rule_id` that is unknown **or** belongs to another World
/// is the core's 404 naming only the id (AR-6) — never a silent no-op.
/// Returns the report `run` prints once the writer settled.
///
/// # Errors
///
/// Returns the core's named refusal on a cross-author World (403), an
/// unknown/foreign rule id (404), or a storage error.
pub async fn rule_deactivate(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    rule_id: &str,
) -> Result<Option<String>> {
    let request = WorldRuleUpdateRequest {
        status: Some(DEPRECATED_STATUS.to_string()),
        ..WorldRuleUpdateRequest::default()
    };

    core.update_world_rule(
        principal,
        world_id.to_string(),
        rule_id.to_string(),
        request,
    )
    .await
    .map_err(map_core_error)?;

    Ok(Some(format!(
        "✓ Rule deactivated: {rule_id} (status={DEPRECATED_STATUS})"
    )))
}
