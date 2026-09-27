//! Structured-rule author surface — `creator world rule add|list|deactivate|archive`
//! (V1.166 PD-1 / AR-2 / AR-3, DR-64; archive lifecycle V1.198 §13).
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
//! `status` is **not** one of them: it is the core's vocabulary — `draft` /
//! `active` / `deprecated` at create, `archived` joining them as the terminal
//! PATCH transition (V1.198 §13) — so any other value is refused by the core
//! instead of being stored. `statement` is the **human summary only** — it is
//! never parsed by the evaluator (PD-1). Machine evaluation reads
//! `extensions.nexus.constraint` (AR-2 carrier).
//!
//! # Ownership
//!
//! `add`, `deactivate` and `archive` gate on the core's shared world-owner
//! guard: a foreign or missing World is a named 404/403 refusal, never a
//! silent no-op. `deactivate` is the core's `status = deprecated` update
//! (PD-1 recovery lock — no DELETE route, re-activation is a Non-Goal:
//! authors add a new rule). `archive` is the core's terminal
//! `status = archived` transition (V1.198 §13): the row is retained, hidden
//! from the default read and accepted only as an archive-only repeat.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::{WorldRuleCreateRequest, WorldRuleResponse, WorldRuleUpdateRequest};
use nexus_core::{CoreService, Principal, RulePatchPresence};
use serde_json::{Map, Value};

/// The spoke status written by `rule deactivate` (PD-1: spoke vocabulary —
/// do **not** invent `inactive`).
const DEPRECATED_STATUS: &str = "deprecated";

/// The terminal status written by `rule archive` (V1.198 §13: the fourth
/// status, transition-only — `add --status archived` is refused).
const ARCHIVED_STATUS: &str = "archived";

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
        /// Rule status (the core's closed create grammar: `draft` / `active` /
        /// `deprecated`; `archived` is a transition, not an authoring state —
        /// any other value is refused, never stored)
        #[arg(long, default_value = "active")]
        status: String,
        /// Structured constraint carrier as a JSON object string (AR-2:
        /// closed shapes; validated member-aware by the core, fail early)
        #[arg(long)]
        constraint: String,
    },

    /// List a world's rules (every non-archived status — draft/deprecated
    /// included, so authors see what auto-include will skip; archived rows are
    /// omitted by default per the V1.198 default read)
    List {
        /// World ID (e.g. `wld_abc123`)
        #[arg(long)]
        world_id: String,
        /// Include retained `archived` rows (default: archived omitted; the
        /// omission is selected by the core, never filtered here)
        #[arg(long)]
        include_archived: bool,
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

    /// Archive a rule — the terminal `status = archived` transition (the row
    /// is retained but hidden from the default list; repetition succeeds).
    /// There is no restore or delete: reviving content means adding a new rule
    Archive {
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
            RuleCommand::List {
                world_id,
                include_archived,
                json,
            } => rule_list(&core, &principal, &world_id, include_archived, json).await,
            RuleCommand::Deactivate { world_id, rule_id } => {
                rule_deactivate(&core, &principal, &world_id, &rule_id).await
            }
            RuleCommand::Archive { world_id, rule_id } => {
                rule_archive(&core, &principal, &world_id, &rule_id).await
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

/// `creator world rule list` — the world's rules in store order
/// `canonical_name ASC, rule_id ASC` (AR-3). Every non-archived status is
/// listed — draft/deprecated included, so authors see what auto-include will
/// skip — while `archived` rows stay hidden unless `include_archived` is set
/// (V1.198 §13). The omission is selected once by the core and pushed into
/// SQL; this leaf never filters rows.
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
    include_archived: bool,
    json: bool,
) -> Result<Option<String>> {
    // V1.198 §13: the caller selects inclusion once; the core omits archived
    // rows in SQL before the 501-row probe (`--include-archived` reveals them).
    let response = core
        .list_world_rules(principal, world_id.to_string(), include_archived)
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
/// is the core's 404 naming only the id (AR-6) — never a silent no-op. An
/// archived (terminal) row also refuses this status exit by naming `status`
/// (V1.198 §13), so this leaf can never revive a tombstone.
/// Returns the report `run` prints once the writer settled.
///
/// # Errors
///
/// Returns the core's named refusal on a cross-author World (403), an
/// unknown/foreign rule id (404), the archived-tombstone status refusal, or a
/// storage error.
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
        // A direct caller names the fields it authored: this leaf writes
        // `status` alone (the generated DTO cannot state that itself).
        RulePatchPresence::from_supplied_keys(&["status"]),
    )
    .await
    .map_err(map_core_error)?;

    Ok(Some(format!(
        "✓ Rule deactivated: {rule_id} (status={DEPRECATED_STATUS})"
    )))
}

/// `creator world rule archive` — set a rule's status to `archived`
/// (V1.198 §13 terminal transition).
///
/// The same core `status` update seam as [`rule_deactivate`]: the
/// World-ownership guard runs first, an unknown or cross-World `rule_id` is
/// the core's 404 naming only the id (AR-6), and `archived` is written with
/// `status` as the only supplied member — the presence metadata the core's
/// tombstone guard requires. Repeating the command on an already-archived row
/// succeeds without a write; every other member or status value is refused by
/// the core, so no restore path exists through this leaf. Returns the report
/// `run` prints once the writer settled.
///
/// # Errors
///
/// Returns the core's named refusal on a cross-author World (403), an
/// unknown/foreign rule id (404), the archived-tombstone refusal for a
/// non-archive member, or a storage error.
pub async fn rule_archive(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    rule_id: &str,
) -> Result<Option<String>> {
    let request = WorldRuleUpdateRequest {
        status: Some(ARCHIVED_STATUS.to_string()),
        ..WorldRuleUpdateRequest::default()
    };

    core.update_world_rule(
        principal,
        world_id.to_string(),
        rule_id.to_string(),
        request,
        // Status-only presence: the archived tombstone accepts exactly this.
        RulePatchPresence::from_supplied_keys(&["status"]),
    )
    .await
    .map_err(map_core_error)?;

    Ok(Some(format!(
        "✓ Rule archived: {rule_id} (status={ARCHIVED_STATUS})"
    )))
}
