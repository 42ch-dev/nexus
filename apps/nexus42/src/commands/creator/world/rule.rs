//! Structured-rule author surface — `creator world rule add|list|deactivate`
//! (V1.166 PD-1 / AR-2 / AR-3, DR-64).
//!
//! The CLI is the **only** write path for `spoke_rules` rows and the
//! **CLI-only validation gate** for the AR-2 constraint carrier (fail early:
//! malformed carriers are rejected at `add` with a message naming the
//! offending member; the daemon performs no check-time carrier validation).
//!
//! # Spoke vocabulary (verbatim — never nexus-coerced at rest)
//!
//! `kind` (core `rule` / `prohibition` / `style`), `status` (core `draft` /
//! `active` / `deprecated`), `severity_hint` (core `info` / `warning` /
//! `error`) are open strings stored verbatim. `statement` is the **human
//! summary only** — it is never parsed by the evaluator (PD-1). Machine
//! evaluation reads `extensions.nexus.constraint` (AR-2 carrier).
//!
//! # Ownership
//!
//! Writes (`add` / `deactivate`) gate on `narrative_write::is_world_owned`
//! (AR-3: the world-command write-guard precedent) — a foreign world is a
//! named reject, never a silent no-op. `deactivate` adds the per-rule guard:
//! `set_rule_status`'s `Ok(false)` (unknown id OR foreign rule) becomes a
//! named reject naming the `rule_id` (PD-1). `deactivate` writes the spoke
//! vocabulary `status = "deprecated"` — never `inactive`.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_local_db::narrative_write::is_world_owned;
use nexus_local_db::spoke_rules::{
    insert_rule, list_rules_by_world, set_rule_status, SpokeRuleRow,
};
use sqlx::SqlitePool;

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
        /// Rule status (open string; core: draft / active / deprecated)
        #[arg(long, default_value = "active")]
        status: String,
        /// Structured constraint carrier as a JSON object string (AR-2:
        /// six closed shapes; validated here, fail early)
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
/// Resolves the active workspace pool and creator, then delegates to the
/// hermetic logic functions below.
///
/// # Errors
///
/// Returns `CliError` if the active creator is unset, the database is
/// unavailable, the carrier fails AR-2 validation (add), or an ownership /
/// per-rule guard rejects the write.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: RuleCommand, config: &CliConfig) -> Result<()> {
    let creator_id = super::active_creator_id(config)?;
    let pool = super::open_workspace_pool(config).await?;
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
            rule_add(
                &pool,
                &creator_id,
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
            Ok(())
        }
        RuleCommand::List { world_id, json } => rule_list(&pool, &world_id, json).await,
        RuleCommand::Deactivate { world_id, rule_id } => {
            rule_deactivate(&pool, &creator_id, &world_id, &rule_id).await
        }
    }
}

// ── Hermetic logic functions ──────────────────────────────────────────
//
// These take an explicit `&SqlitePool` (+ `creator_id` where an owner gate is
// needed) so integration tests can drive them against a fresh temp DB without
// touching `$HOME`-resolved paths (same pattern as `world/kb/mod.rs`).

/// `creator world rule add` — create a structured rule row.
///
/// Validates the AR-2 carrier (the CLI-only gate, fail early), mints the
/// `rul_<32-hex>` id (uuid v4 simple) **before** the insert (AR-2), guards
/// world ownership, and inserts the full row. `extensions_json` is written
/// fresh as `{"nexus": {"constraint": <carrier verbatim>}}` (rules carry no
/// other nexus keys today).
///
/// Returns the minted `rule_id`. Emits a soft stderr warning when `--status`
/// is outside the documented core set (draft / active / deprecated) — the
/// value is still stored verbatim (PD-1, no coercion at rest); the warning
/// flags the auto-include footgun (S-002).
///
/// # Errors
///
/// Returns `CliError::Other` naming the offending member for a malformed
/// `--constraint` (prefixed `--constraint: `), a named reject when the
/// active creator does not own the world, a reject when `--entry-type` is
/// combined with an `observer_cardinality` carrier, or a database error.
#[allow(clippy::too_many_arguments)]
// ^ justification: mirrors run_event_add's flat field surface; grouping the
// ten PD-1 flags into a struct would add indirection for the two callers
// (CLI + tests).
pub async fn rule_add(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
    name: &str,
    kind: &str,
    statement: &str,
    severity: &str,
    entry_types: &[String],
    status: &str,
    constraint_json: &str,
) -> Result<String> {
    require_owned_world(pool, creator_id, world_id).await?;

    // AR-2 CLI gate: --constraint must parse as JSON, then as a closed carrier.
    let carrier: serde_json::Value = serde_json::from_str(constraint_json)
        .map_err(|e| CliError::Other(format!("--constraint: invalid JSON: {e}")))?;
    let constraint = nexus_spoke_adapter::constraint::parse_carrier_json(&carrier)
        .map_err(|e| CliError::Other(format!("--constraint: {e}")))?;

    // AR-2 targeting interplay: observer_cardinality applies to timeline
    // events (no entry_type) — combining with --entry-type is rejected early,
    // never silently ignored.
    if matches!(
        constraint,
        nexus_spoke_adapter::constraint::Constraint::ObserverCardinality { .. }
    ) && !entry_types.is_empty()
    {
        return Err(CliError::Other(
            "--entry-type cannot be combined with an observer_cardinality constraint: \
             observer_cardinality applies to timeline events, which carry no entry_type"
                .to_string(),
        ));
    }

    // AR-2 id minting: rul_ ++ uuid v4 simple (32 hex, no hyphens) — minted in
    // the CLI before insert_rule (full-row insert).
    let rule_id = format!("rul_{}", uuid::Uuid::new_v4().simple());
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default();

    let row = SpokeRuleRow {
        rule_id: rule_id.clone(),
        world_id: world_id.to_string(),
        schema_version: 1,
        canonical_name: name.to_string(),
        kind: kind.to_string(),
        statement: Some(statement.to_string()),
        description: None,
        target_entry_types_json: serde_json::to_string(entry_types)?,
        severity_hint: Some(severity.to_string()),
        status: Some(status.to_string()),
        source_anchor_json: None,
        // AR-2 CLI row assembly: the namespace is written fresh at create.
        extensions_json: serde_json::json!({ "nexus": { "constraint": carrier } }).to_string(),
        created_at: Some(now_epoch),
        updated_at: Some(now_epoch),
    };
    insert_rule(pool, &row)
        .await
        .map_err(|e| CliError::Other(format!("Failed to insert rule '{rule_id}': {e}")))?;

    // S-002: `--status` is an open string stored verbatim (PD-1 — never
    // coerced at rest), but the AR-1 auto-include filter matches exactly
    // `status == "active"`. Warn when it's outside the documented core set
    // so a typo'd status doesn't silently create a never-included rule.
    if !matches!(status, "draft" | "active" | "deprecated") {
        eprintln!(
            "Warning: --status {status:?} is outside the documented core set \
             (draft / active / deprecated) — stored verbatim (PD-1), but the rule \
             will never auto-include (the AR-1 filter matches exactly 'active')."
        );
    }

    println!("✓ Rule added: {rule_id}");
    println!("  World:       {world_id}");
    println!("  Name:        {name}");
    println!("  Kind:        {kind}");
    println!("  Status:      {status}");
    println!("  Severity:    {severity}");
    if !entry_types.is_empty() {
        println!("  Entry types: {}", entry_types.join(", "));
    }
    println!("  Constraint:  {}", constraint.family());
    Ok(rule_id)
}

/// `creator world rule list` — all rules of a world, **all statuses**
/// (PD-1 list: `canonical_name ASC, rule_id ASC` at storage — AR-3).
///
/// `--json` emits the machine-readable summary array (kb list precedent).
///
/// # Errors
///
/// Returns `CliError::Other` if the storage list or JSON serialization fails.
pub async fn rule_list(pool: &SqlitePool, world_id: &str, json: bool) -> Result<()> {
    let rows = list_rules_by_world(pool, world_id)
        .await
        .map_err(|e| CliError::Other(format!("Failed to list rules for world {world_id}: {e}")))?;

    if json {
        let items: Vec<serde_json::Value> = rows.iter().map(rule_summary_json).collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No rules in world {world_id}.");
        return Ok(());
    }

    println!("Rules in world {world_id}:");
    println!(
        "{:<24} {:<28} {:<12} {:<10} {:<10} STATEMENT",
        "RULE_ID", "NAME", "KIND", "STATUS", "SEVERITY"
    );
    for row in &rows {
        println!(
            "{:<24} {:<28} {:<12} {:<10} {:<10} {}",
            row.rule_id,
            row.canonical_name,
            row.kind,
            row.status.as_deref().unwrap_or("-"),
            row.severity_hint.as_deref().unwrap_or("-"),
            row.statement.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

/// `creator world rule deactivate` — set a rule's status to `deprecated`
/// (spoke vocabulary; PD-1).
///
/// World-ownership is guarded first (`is_world_owned`); `set_rule_status`'s
/// `Ok(false)` — unknown id OR foreign rule — becomes a **named reject
/// naming the `rule_id`** (PD-1 foreign-world guard), never a silent no-op.
///
/// # Errors
///
/// Returns `CliError::Other` with a named message on a cross-author world or
/// an unknown/foreign rule id, or a database error.
pub async fn rule_deactivate(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
    rule_id: &str,
) -> Result<()> {
    require_owned_world(pool, creator_id, world_id).await?;

    let updated = set_rule_status(pool, world_id, rule_id, DEPRECATED_STATUS)
        .await
        .map_err(|e| CliError::Other(format!("Failed to deactivate rule '{rule_id}': {e}")))?;
    if !updated {
        return Err(CliError::Other(format!(
            "Rule '{rule_id}' not found in world '{world_id}' \
             (unknown or foreign rule id). \
             List rules with: nexus42 creator world rule list --world-id {world_id}"
        )));
    }

    println!("✓ Rule deactivated: {rule_id} (status={DEPRECATED_STATUS})");
    Ok(())
}

/// World-command write-guard: the active creator must own the world
/// (AR-3 via `narrative_write::is_world_owned` — the V1.67 shared admission
/// gate).
///
/// # Errors
///
/// Returns a named `CliError::Other` reject when `creator_id` does not own
/// `world_id` (missing world OR cross-author — the storage gate does not
/// distinguish; the message names both ids).
async fn require_owned_world(pool: &SqlitePool, creator_id: &str, world_id: &str) -> Result<()> {
    let owned = is_world_owned(pool, creator_id, world_id)
        .await
        .map_err(|e| CliError::Other(format!("World ownership check failed: {e}")))?;
    if !owned {
        return Err(CliError::Other(format!(
            "Active creator '{creator_id}' does not own world '{world_id}'; \
             rules can only be authored on worlds the active creator owns"
        )));
    }
    Ok(())
}

/// Build the JSON summary object for `--json` list output (kb list
/// precedent): carrier projected first-class, spoke vocabulary verbatim.
#[must_use]
pub fn rule_summary_json(row: &SpokeRuleRow) -> serde_json::Value {
    let constraint = serde_json::from_str::<serde_json::Value>(&row.extensions_json)
        .ok()
        .and_then(|v| {
            v.get("nexus")
                .and_then(|nexus| nexus.get("constraint"))
                .cloned()
        });
    serde_json::json!({
        "rule_id": row.rule_id,
        "canonical_name": row.canonical_name,
        "kind": row.kind,
        "status": row.status,
        "severity_hint": row.severity_hint,
        "statement": row.statement,
        "target_entry_types": serde_json::from_str::<Vec<String>>(&row.target_entry_types_json)
            .unwrap_or_default(),
        "constraint": constraint,
    })
}
