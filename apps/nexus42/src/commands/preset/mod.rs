//! `nexus42 preset` — canonical preset CLI group (PL-5, AR-24, AR-25).
//!
//! Top-level developer-facing surface for presets:
//! `list|show|validate|scaffold|trigger|patch`.
//!
//! - `validate <path>` is the positional local validator
//!   (`validate_preset_local`, moved here from `system`; V1.153 P3). It runs
//!   entirely in process: the daemon leg and the `--offline` synonym it
//!   shadowed were removed in v1.193 P1-T1.
//! - `list|show|scaffold` and `trigger` are typed `nexus-core` reads/writes
//!   (v1.193 P1-T2) — no daemon route and no HTTP client.
//! - `scaffold` authors a user preset bundle through
//!   `CoreService::scaffold_preset`.
//! - `run` was removed in v1.193 P1-T1: it only forwarded into
//!   `creator run`'s daemon-API path and had no complete direct core
//!   operation of its own (PL-5).
//! - `show <id>` prints the AR-20 profile (`CoreService::get_preset_profile`)
//!   with lanes + orchestration fields; declared signals are labeled
//!   **Declared, not delivered** (AR-25, locked trigger-lane vocabulary).
//! - `trigger <id>` prints trigger-lane classification only — never cron
//!   authoring, never a runner (cron authoring stays `creator works cron`
//!   until P2 owns the UI; PL-5 / PL-18).
//! - `--json` = the core DTO verbatim (camelCase); no CLI-local renaming
//!   (AR-25).
//!
//! The read/authoring leaves open the direct-writer core and await
//! `finish_direct` **before** printing, so no row is reported ahead of a
//! close that did not settle.

#![allow(clippy::print_literal)]

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::Result;
use crate::CliError;
use clap::Subcommand;
// The preset profile family is generated one type per module; the lane de-duplicates
// every `$ref` target to its standalone module, so import each type from the module
// that owns it.
use nexus_contracts::generated::core::orchestration_presets::preset_profile_exit_when::PresetProfileExitWhen;
use nexus_contracts::generated::core::orchestration_presets::preset_profile_lanes::PresetProfileLanes;
use nexus_contracts::generated::core::orchestration_presets::preset_profile_next::PresetProfileNext;
use nexus_contracts::generated::core::orchestration_presets::preset_profile_response::PresetProfileResponse;
// Same family note as above, for the grouped listing: the core returns the
// generated `preset_management` response (its `source` enum displays the real
// `embedded` / `system` / `user` label), so the CLI reads that type directly
// instead of the retired daemon client's private copy.
use nexus_contracts::generated::daemon_api::orchestration::workflow_document::{
    WorkflowDocument, WorkflowNodeKind,
};
use nexus_contracts::generated::daemon_api::preset_management::list_presets_response::ListPresetsResponse;
use nexus_contracts::ScaffoldPresetRequest;
use nexus_workflow_ir::Violation;

pub mod patch;

/// Newest Workflow IR document version this build can validate — the v0 draft
/// `nexus_preset::workflow_ir` emits. A schema bump has to move both.
const SUPPORTED_IR_VERSION: u64 = 0;

#[derive(Debug, Subcommand)]
pub enum PresetCommand {
    /// List all discoverable presets (embedded + user + system)
    List {
        /// Filter by `run_intent` (e.g. `work_init`, `knowledge_ingest`)
        #[arg(long)]
        intent: Option<String>,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print the profile for a preset (lanes + orchestration fields)
    Show {
        /// Preset ID (embedded, user, or `_system.` qualified)
        id: String,
        /// Emit the daemon profile DTO verbatim (camelCase)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Validate a preset YAML/bundle at a given path (local, in process)
    Validate {
        /// Path to preset.yaml (or bundle directory)
        path: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Generate a strategy bundle from templates
    Scaffold {
        /// Name for the new user preset
        name: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print trigger-lane classification only (never cron authoring)
    Trigger {
        /// Preset ID (embedded, user, or `_system.` qualified)
        id: String,
        /// Emit machine-readable JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Patch a strategy canvas node (state, transition, or prompt template)
    ///
    /// CAS-guarded direct-core writes over the strategy canvas seam
    /// (V1.175 P1 Task 2, group 1). Every leaf takes `--base-revision`;
    /// a stale revision returns 409 `strategy_conflict` (current revision,
    /// node, conflicting path, recovery hint). Re-read the Strategy and
    /// reapply with the new revision.
    ///
    /// Retry guidance: on `strategy_conflict`, re-read the Strategy
    /// (e.g. `preset show`) and reapply with the new revision.
    Patch {
        #[command(subcommand)]
        command: patch::PatchCommand,
    },
}

/// Run the `nexus42 preset` command group.
///
/// # Errors
///
/// Returns an error if the delegated command fails.
pub async fn run(cmd: PresetCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        PresetCommand::List { intent, json } => list_presets(config, intent.as_deref(), json).await,
        PresetCommand::Show { id, json } => show_preset(config, &id, json).await,
        PresetCommand::Validate { path, json } => validate_preset(&path, json),
        PresetCommand::Scaffold { name, json } => scaffold_preset(config, &name, json).await,
        PresetCommand::Trigger { id, json } => trigger_preset(config, &id, json).await,
        PresetCommand::Patch { command } => patch::run(command, config).await,
    }
}

/// List all discoverable presets (embedded + user + system).
///
/// Moved from `system preset list` (V1.153) — the shared listing job, not
/// re-implemented (AR-24).
///
/// The display list is built from the typed core listing
/// (`CoreService::list_presets` — embedded + system + user groups,
/// W-002/F-001), so user presets appear and each row is labeled by its real
/// source. `--intent` filtering runs across all groups.
async fn list_presets(
    config: &CliConfig,
    intent_filter: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        // A failure here is a real core refusal — surface it instead of
        // silently degrading the list to no presets (F-001 reliability nit).
        let response = core
            .list_presets(&principal)
            .await
            .map_err(map_core_error)?;
        Ok(build_preset_rows(&response, intent_filter))
    }
    .await;
    let presets = finish_direct(&core, outcome).await?;

    if json_output {
        let output: Vec<serde_json::Value> = presets
            .iter()
            .map(|(id, source, intents)| {
                serde_json::json!({
                    "id": id,
                    "source": source,
                    "run_intents": intents,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if presets.is_empty() {
        println!("No presets found.");
    } else {
        println!("Presets:");
        for (id, source, intents) in &presets {
            let intents_str = if intents.is_empty() {
                String::new()
            } else {
                format!(" [{}]", intents.join(", "))
            };
            println!("  {id} ({source}){intents_str}");
        }
        println!("\n{} preset(s)", presets.len());
    }

    Ok(())
}

/// Flatten the grouped core listing into display rows
/// `(id, source, run_intents)`, optionally filtered by `run_intent`
/// (W-002/F-001). Pure over the core DTO — hermetically testable.
fn build_preset_rows(
    resp: &ListPresetsResponse,
    intent_filter: Option<&str>,
) -> Vec<(String, String, Vec<String>)> {
    let mut presets: Vec<(String, String, Vec<String>)> = Vec::new();

    for group in [&resp.embedded, &resp.system, &resp.user] {
        for summary in group {
            presets.push((
                summary.id.clone(),
                summary.source.to_string(),
                summary.run_intents.clone(),
            ));
        }
    }

    if let Some(intent) = intent_filter {
        presets.retain(|(_, _, intents)| intents.iter().any(|i| i == intent));
    }

    presets
}

/// Print the AR-20 profile for a preset (`CoreService::get_preset_profile`).
///
/// `--json` serializes the core DTO verbatim (camelCase, AR-25). Text
/// output names lanes + orchestration fields; declared signals are labeled
/// **Declared, not delivered**.
async fn show_preset(config: &CliConfig, id: &str, json: bool) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_preset_profile(&principal, id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let profile: PresetProfileResponse = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        print!("{}", format_profile_text(&profile));
    }
    Ok(())
}

/// Print trigger-lane classification only (AR-25) — never cron authoring,
/// never a runner.
async fn trigger_preset(config: &CliConfig, id: &str, json: bool) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_preset_profile(&principal, id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let profile: PresetProfileResponse = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile.lanes)?);
    } else {
        print!("{}", format_trigger_text(id, &profile.lanes));
    }
    Ok(())
}

/// Scaffold a user preset bundle from templates
/// (`CoreService::scaffold_preset`).
async fn scaffold_preset(config: &CliConfig, name: &str, json: bool) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.scaffold_preset(
            &principal,
            ScaffoldPresetRequest {
                name: name.to_string(),
            },
        )
        .await
        .map_err(map_core_error)
    }
    .await;
    let resp = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Scaffolded preset '{}' at {}", resp.id, resp.path);
    }
    Ok(())
}

/// Validate a preset YAML/bundle or Workflow IR document at a given path.
///
/// Validation runs in process through [`validate_preset_local`] — the sole
/// validator behind this command (v1.193 P1-T1). There is no daemon leg and
/// no `--offline` switch: `--offline` was a silent synonym for the local
/// path (D6) and the path is positional.
///
/// An invalid preset produces a non-zero exit so scripts (e.g.
/// `strategy-samples/validate.sh`) can rely on the exit code.
///
/// Moved from `system preset validate` (V1.153) — the shared validator job,
/// not re-implemented (AR-24).
fn validate_preset(path: &str, json_output: bool) -> Result<()> {
    let resp: serde_json::Value = validate_preset_local(path)?;

    let valid = resp
        .get("valid")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        print_validate_verdict(&resp);
    }

    if valid {
        Ok(())
    } else {
        let error_count = resp
            .get("errors")
            .and_then(|v| v.as_array())
            .map_or(0, std::vec::Vec::len);
        Err(CliError::Other(format!(
            "preset validation failed: {error_count} error(s)"
        )))
    }
}

/// Print the human-readable verdict for a validate response
/// (`{valid, id, version, state_count, errors, warnings}` — the shape
/// [`validate_preset_local`] returns).
fn print_validate_verdict(resp: &serde_json::Value) {
    let valid = resp
        .get("valid")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let errors: Vec<&str> = resp
        .get("errors")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |a| {
            a.iter().filter_map(serde_json::Value::as_str).collect()
        });
    let warnings: Vec<&str> = resp
        .get("warnings")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |a| {
            a.iter().filter_map(serde_json::Value::as_str).collect()
        });

    if valid {
        println!("✓ Valid preset");
    } else {
        println!("✗ Invalid preset ({} error(s))", errors.len());
    }
    for error in &errors {
        println!("  - {error}");
    }
    for warning in &warnings {
        println!("  - warning: {warning}");
    }
}

/// Run preset validation in process — the sole validator behind
/// `nexus42 preset validate <path>` (v1.193 P1-T1).
///
/// The composition mirrors the retired daemon handler
/// `POST /v1/daemon/presets:validate`
/// (`loader_validate_manifest_compat` + `validate_path_safety` +
/// `validate_preset_semantic` + `validate_assets_in_bundle`) and returns the
/// same response shape, so verdicts recorded before the cutover stay
/// comparable.
///
/// The daemon handler — not `load_preset` — is deliberately the reference
/// composition: it parses the manifest directly and treats every
/// error-severity diagnostic as a failure, whereas `load_preset`
/// downgrades capability-arg-drift errors to warnings.
///
/// `path` may be a bundle directory (containing `preset.yaml`), a
/// `preset.yaml` file (asset checks run against its parent), a
/// standalone YAML file (no asset checks — same as the daemon's
/// `infer_bundle_root`), or a Workflow IR document (`*.ir.json`), which
/// is validated directly (§ CLI parity surface).
///
/// Moved from `system` (V1.153 P3) — the shared validator core, not
/// re-implemented (AR-24).
fn validate_preset_local(path: &str) -> Result<serde_json::Value> {
    use nexus_orchestration::CapabilityRegistry;
    use nexus_preset::{
        loader_validate_manifest_compat, validate_assets_in_bundle, validate_path_safety,
        validate_preset_semantic, yaml_value_depth, ValidationResult as PresetValidationResult,
        DEFAULT_MAX_YAML_DEPTH,
    };

    let file_path = std::path::Path::new(path);

    // An explicit `*.ir.json` file selects direct IR validation; every other
    // path form keeps resolving through the YAML bundle rules below.
    if file_path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().ends_with(".ir.json"))
    {
        if !file_path.exists() {
            return Err(CliError::Other(format!(
                "File not found: {}",
                file_path.display()
            )));
        }
        return validate_workflow_ir_file(file_path);
    }

    // Resolve the target file + optional bundle root (mirrors the daemon's
    // `infer_bundle_root`: only a file literally named `preset.yaml` has a
    // bundle root; a directory argument points at its preset.yaml).
    let (preset_yaml_path, bundle_root) = if file_path.is_dir() {
        (file_path.join("preset.yaml"), Some(file_path.to_path_buf()))
    } else {
        let is_bundle_manifest = file_path
            .file_name()
            .is_some_and(|name| name == "preset.yaml");
        (
            file_path.to_path_buf(),
            is_bundle_manifest
                .then(|| file_path.parent().map(std::path::Path::to_path_buf))
                .flatten(),
        )
    };

    if !preset_yaml_path.exists() {
        return Err(CliError::Other(format!(
            "File not found: {}",
            preset_yaml_path.display()
        )));
    }

    // Size limit (same 1 MiB limit the loader/daemon enforce).
    ensure_document_size(&preset_yaml_path, "Preset YAML")?;

    let yaml = std::fs::read_to_string(&preset_yaml_path)?;

    // Parse + depth check (same path as `load_preset_from_str_with_limits`).
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&yaml)
        .map_err(|e| CliError::Other(format!("preset YAML parse error: {e}")))?;
    let depth = yaml_value_depth(&yaml_value);
    if depth > DEFAULT_MAX_YAML_DEPTH {
        return Err(CliError::Other(format!(
            "Nesting depth ({depth}) exceeds maximum ({DEFAULT_MAX_YAML_DEPTH})"
        )));
    }
    let manifest: nexus_contracts::local::orchestration::preset::PresetManifest =
        serde_yaml::from_value(yaml_value)
            .map_err(|e| CliError::Other(format!("preset structural error: {e}")))?;

    let caps = CapabilityRegistry::with_builtins();
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // C2: loader-equivalent structural checks (same defects the runtime
    //     loader would reject).
    for problem in loader_validate_manifest_compat(&manifest, &caps) {
        errors.push(format!("{}: {}", problem.path, problem.error));
    }

    // C3 + A5 + A3: shared validation facade. Bundle asset checks only run
    //     when the target resolves to a bundle root.
    let asset_result = bundle_root
        .as_deref()
        .map_or_else(PresetValidationResult::default, |root| {
            validate_assets_in_bundle(&manifest, root)
        });
    for diagnostic in validate_path_safety(&manifest)
        .diagnostics
        .iter()
        .chain(
            validate_preset_semantic(&manifest, &caps)
                .diagnostics
                .iter(),
        )
        .chain(asset_result.diagnostics.iter())
    {
        push_diagnostic(diagnostic, &mut errors, &mut warnings);
    }

    Ok(validation_response(
        &manifest.preset.id,
        serde_json::json!(manifest.preset.version),
        manifest.states.len(),
        &errors,
        &warnings,
    ))
}

/// Validate a Workflow IR document directly (`... .ir.json`).
///
/// The document is parsed as IR and the same checks the YAML path runs are
/// applied to it, so both formats report the same diagnostics for the same
/// defect. An IR file carries no bundle root, so asset checks are skipped
/// exactly as for a standalone YAML file.
fn validate_workflow_ir_file(file_path: &std::path::Path) -> Result<serde_json::Value> {
    use nexus_orchestration::CapabilityRegistry;
    use nexus_preset::validation::{validate_workflow_ir, validate_workflow_ir_path_safety};
    use nexus_preset::{export_preset, loader_validate_manifest_compat};

    ensure_document_size(file_path, "Workflow IR")?;
    let json = std::fs::read_to_string(file_path)?;
    let document: WorkflowDocument = serde_json::from_str(&json)
        .map_err(|e| CliError::Other(format!("workflow IR parse error: {e}")))?;

    // A document from a newer IR version cannot be interpreted here, so no
    // check below it is trustworthy: refuse it instead of letting the
    // exporter's refusal fall silent (the wording mirrors `export_preset`).
    if document.ir_version > SUPPORTED_IR_VERSION {
        return Ok(validation_response(
            document.id.as_str(),
            serde_json::json!(document.version),
            outer_state_count(&document),
            &[format!(
                "ir_version: document ir_version {} is newer than supported {SUPPORTED_IR_VERSION}",
                document.ir_version
            )],
            &[],
        ));
    }

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Q2 structural checks the semantic facade cannot see (dangling edge and
    // dependency targets, duplicate node ids, nest bindings, nest depth). They
    // run on every IR document, including the IR-only shapes that have no
    // manifest projection for the loader-level checks to inspect.
    for violation in structural_violations(&document) {
        errors.push(format!("{}: {violation}", violation_path(&violation)));
    }

    let caps = CapabilityRegistry::with_builtins();

    // C2 is a manifest-level contract, so a manifest-expressible document is
    // projected back to run the same check the YAML path always runs; IR-only
    // shapes (joins, nest groups, pattern slots) have no manifest projection.
    if let Ok(manifest) = export_preset(&document) {
        for problem in loader_validate_manifest_compat(&manifest, &caps) {
            errors.push(format!("{}: {}", problem.path, problem.error));
        }
    }

    for diagnostic in validate_workflow_ir_path_safety(&document)
        .diagnostics
        .iter()
        .chain(validate_workflow_ir(&document, &caps).diagnostics.iter())
    {
        push_diagnostic(diagnostic, &mut errors, &mut warnings);
    }

    Ok(validation_response(
        document.id.as_str(),
        serde_json::json!(document.version),
        outer_state_count(&document),
        &errors,
        &warnings,
    ))
}

/// States the YAML verdict counts: outer states and terminals. Inner steps are
/// graph children of a state, not states of their own.
fn outer_state_count(document: &WorkflowDocument) -> usize {
    document
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                WorkflowNodeKind::OuterState | WorkflowNodeKind::Terminal
            )
        })
        .count()
}

/// Q2 structural violations worth reporting for one document.
///
/// `GraphCycle` is deliberately dropped: outer-state loops are legal for
/// imported presets, so rejecting them would refuse valid workflows. Every
/// other violation (dangling edge/dependency, duplicate node, nest id, parent,
/// child and depth) is a real defect.
fn structural_violations(document: &WorkflowDocument) -> Vec<Violation> {
    nexus_workflow_ir::validate(&[document])
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter(|violation| !matches!(violation, Violation::GraphCycle { .. }))
        .collect()
}

/// Location prefix for a structural violation, in the same `path: message`
/// shape the loader and facade diagnostics use.
fn violation_path(violation: &Violation) -> String {
    match violation {
        Violation::DuplicateDocumentId(_) => "id".to_string(),
        Violation::DuplicateNodeId { node, .. } => format!("nodes.{node}"),
        Violation::DanglingEdge { edge, .. } => format!("edges.{edge}"),
        Violation::DanglingDependency { node, .. } => format!("nodes.{node}.depends_on"),
        Violation::DuplicateNestId { nest, .. } => format!("nests.{nest}"),
        Violation::UnknownNestParent { nest, .. }
        | Violation::NestParentNotComposable { nest, .. } => {
            format!("nests.{nest}.parent_node")
        }
        Violation::IncompleteNestChild { nest, .. } => format!("nests.{nest}.child"),
        // The message carries the nest id / cycle path / depth, so one prefix
        // suffices for the document-wide nest violations.
        Violation::EmptyNestId { .. }
        | Violation::NestCycle { .. }
        | Violation::NestDepthExceeded { .. } => "nests".to_string(),
        Violation::GraphCycle { .. } => "edges".to_string(),
    }
}

/// Enforce the loader's document size limit before reading a preset file.
///
/// `label` names the document kind in the error, so both input formats share
/// one gate without changing the message the YAML path already emitted.
fn ensure_document_size(path: &std::path::Path, label: &str) -> Result<()> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > nexus_preset::DEFAULT_MAX_YAML_SIZE as u64 {
        return Err(CliError::Other(format!(
            "{label} exceeds maximum size ({} bytes, limit is {} bytes)",
            metadata.len(),
            nexus_preset::DEFAULT_MAX_YAML_SIZE
        )));
    }
    Ok(())
}

/// Fold one facade diagnostic into the verdict's error/warning strings.
fn push_diagnostic(
    diagnostic: &nexus_preset::ValidationDiagnostic,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    match diagnostic.severity {
        nexus_preset::DiagnosticSeverity::Error => {
            errors.push(format!("{}: {}", diagnostic.path, diagnostic.message));
        }
        nexus_preset::DiagnosticSeverity::Warning => {
            warnings.push(format!("{}: {}", diagnostic.path, diagnostic.message));
        }
    }
}

/// Build the verdict both input formats return — same shape as the daemon
/// endpoint (warnings omitted when empty, matching the daemon's
/// `skip_serializing_if`).
fn validation_response(
    id: &str,
    version: serde_json::Value,
    state_count: usize,
    errors: &[String],
    warnings: &[String],
) -> serde_json::Value {
    let mut resp = serde_json::Map::new();
    resp.insert("valid".to_string(), serde_json::json!(errors.is_empty()));
    resp.insert("id".to_string(), serde_json::json!(id));
    resp.insert("version".to_string(), version);
    resp.insert("state_count".to_string(), serde_json::json!(state_count));
    resp.insert("errors".to_string(), serde_json::json!(errors));
    if !warnings.is_empty() {
        resp.insert("warnings".to_string(), serde_json::json!(warnings));
    }
    serde_json::Value::Object(resp)
}

// ---------------------------------------------------------------------------
// Text renderers (pure over the daemon DTO — hermetically testable)
// ---------------------------------------------------------------------------

/// Render the human-readable profile text for `preset show`.
///
/// Declared signals are labeled **Declared, not delivered** (AR-25, locked
/// trigger-lane vocabulary).
fn format_profile_text(profile: &PresetProfileResponse) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "Preset: {} (v{})", profile.id, profile.version);
    let _ = writeln!(out, "Source hash: {}", profile.source_hash);
    let _ = writeln!(out);
    let _ = writeln!(out, "Trigger lanes:");
    write_lanes(&mut out, &profile.lanes);
    let _ = writeln!(out);
    let _ = writeln!(out, "States:");
    if profile.states.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for state in &profile.states {
            let _ = writeln!(out, "  {}", state.id);
            if let Some(desc) = &state.description {
                let _ = writeln!(out, "    description: {desc}");
            }
            if !state.enter.is_empty() {
                let _ = writeln!(out, "    enter:");
                for action in &state.enter {
                    let _ = writeln!(out, "      - {}: {}", action.kind, action.name);
                }
            }
            if let Some(exit_when) = &state.exit_when {
                let _ = writeln!(out, "    exit_when: {}", format_exit_when(exit_when));
            }
            if let Some(next) = &state.next {
                let _ = writeln!(out, "    next: {}", format_next(next));
            }
            let _ = writeln!(out, "    terminal: {}", yes_no(state.terminal));
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Roles:");
    if profile.roles.is_empty() {
        let _ = writeln!(out, "  (single-agent mode)");
    } else {
        for role in &profile.roles {
            let _ = writeln!(out, "  {}", role.id);
            let _ = writeln!(out, "    description: {}", role.description);
            let _ = writeln!(out, "    system_prompt_file: {}", role.system_prompt_file);
            if !role.recommended_skills.is_empty() {
                let _ = writeln!(
                    out,
                    "    recommended_skills: {}",
                    role.recommended_skills.join(", ")
                );
            }
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Required capabilities:");
    if profile.required_capabilities.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for cap in &profile.required_capabilities {
            let _ = writeln!(out, "  - {cap}");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Declared signals — Declared, not delivered:");
    if profile.signals.is_empty() {
        let _ = writeln!(out, "  (none)");
    } else {
        for signal in &profile.signals {
            let mut line = format!("  - {} (action: {})", signal.name, signal.action);
            if let Some(target) = &signal.target {
                let _ = write!(line, ", target: {target}");
            }
            let _ = writeln!(out, "{line}");
        }
    }
    out
}

/// Render the trigger-lane classification only (AR-25) — never cron
/// authoring.
fn format_trigger_text(id: &str, lanes: &PresetProfileLanes) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "Trigger lanes for {id}:");
    write_lanes(&mut out, lanes);
    out
}

/// Write the four locked trigger-lane rows (PL-3 vocabulary).
fn write_lanes(out: &mut String, lanes: &PresetProfileLanes) {
    use std::fmt::Write as _;
    let _ = writeln!(out, "  cron:        {}", yes_no(lanes.cron));
    let _ = writeln!(out, "  wall-clock:  {}", yes_no(lanes.wall_clock));
    let _ = writeln!(out, "  session:     {}", yes_no(lanes.session));
    let _ = writeln!(out, "  direct:      {}", yes_no(lanes.direct));
}

const fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Render one exit condition (`llm_judge` / `rule` / `graph_complete` /
/// `manual` / `timer`).
fn format_exit_when(exit_when: &PresetProfileExitWhen) -> String {
    match exit_when.kind.as_str() {
        "llm_judge" => {
            let mut s = "llm_judge".to_string();
            if let Some(t) = &exit_when.template_file {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (template: {t})"));
            }
            if let Some(j) = &exit_when.judge_capability {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (judge: {j})"));
            }
            if let Some(m) = &exit_when.min_interval {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (min_interval: {m})"));
            }
            s
        }
        "timer" => exit_when
            .duration
            .as_ref()
            .map_or_else(|| "timer".to_string(), |d| format!("timer (duration: {d})")),
        other => other.to_string(),
    }
}

/// Render one next-transition form (`linear` / `goNogo` / `labeled` /
/// `conditional` / `branches`).
fn format_next(next: &PresetProfileNext) -> String {
    match next.kind.as_str() {
        "linear" => next
            .target
            .as_ref()
            .map_or_else(|| "linear".to_string(), |t| format!("linear -> {t}")),
        "goNogo" => format!(
            "goNogo (go: {}, nogo: {})",
            next.go.as_deref().unwrap_or_default(),
            next.nogo.as_deref().unwrap_or_default()
        ),
        "labeled" => {
            let edges: Vec<String> = next
                .labeled
                .iter()
                .map(|e| format!("{} -> {}", e.label, e.target))
                .collect();
            format!("labeled [{}]", edges.join(", "))
        }
        "conditional" | "branches" => {
            let rules: Vec<String> = next
                .rules
                .iter()
                .chain(next.branches.iter())
                .map(|r| format!("{} -> {}", r.when, r.target))
                .collect();
            let mut s = format!("{} [{}]", next.kind, rules.join(", "));
            if let Some(d) = &next.default {
                let _ = std::fmt::Write::write_fmt(&mut s, format_args!(" (default: {d})"));
            }
            s
        }
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser;
    use nexus_contracts::generated::core::orchestration_presets::preset_profile_signal::PresetProfileSignal;
    use nexus_contracts::generated::core::orchestration_presets::preset_profile_state::PresetProfileState;

    /// Wrapper for parsing `PresetCommand` in tests.
    #[derive(Debug, clap::Parser)]
    #[command(subcommand_required = true, name = "preset")]
    struct PresetCli {
        #[command(subcommand)]
        command: PresetCommand,
    }

    #[test]
    fn preset_subcommands_parse() {
        for argv in [
            &["preset", "list"][..],
            &["preset", "show", "novel-writing"][..],
            &["preset", "validate", "some/path"][..],
            &["preset", "scaffold", "my-strategy"][..],
            &["preset", "trigger", "novel-writing"][..],
        ] {
            PresetCli::try_parse_from(argv)
                .unwrap_or_else(|e| panic!("{argv:?} should parse: {e}"));
        }
    }

    #[test]
    fn preset_show_json_flag_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "show", "novel-writing", "--json"]).unwrap();
        match cmd.command {
            PresetCommand::Show { id, json } => {
                assert_eq!(id, "novel-writing");
                assert!(json);
            }
            other => panic!("expected show, got {other:?}"),
        }
    }

    #[test]
    fn preset_trigger_json_flag_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "trigger", "_system.maintenance", "--json"])
            .unwrap();
        match cmd.command {
            PresetCommand::Trigger { id, json } => {
                assert_eq!(id, "_system.maintenance");
                assert!(json);
            }
            other => panic!("expected trigger, got {other:?}"),
        }
    }

    #[test]
    fn preset_scaffold_parses() {
        let cmd = PresetCli::try_parse_from(["preset", "scaffold", "my-strategy"]).unwrap();
        match cmd.command {
            PresetCommand::Scaffold { name, json } => {
                assert_eq!(name, "my-strategy");
                assert!(!json);
            }
            other => panic!("expected scaffold, got {other:?}"),
        }
    }

    #[test]
    fn preset_subcommand_required() {
        let result = PresetCli::try_parse_from(["preset"]);
        assert!(result.is_err());
    }

    // ── V1.175 P1 Task 2: strategy patch leaves ────────────────────────

    #[test]
    fn preset_patch_state_parses() {
        let cmd = PresetCli::try_parse_from([
            "preset",
            "patch",
            "state",
            "my-strategy",
            "start",
            "--base-revision",
            "3",
            "--label",
            "begin",
            "--description",
            "Renamed start",
            "--json",
        ])
        .unwrap();
        match cmd.command {
            PresetCommand::Patch { command } => match command {
                patch::PatchCommand::State {
                    strategy_id,
                    state_id,
                    base_revision,
                    label,
                    description,
                    json,
                } => {
                    assert_eq!(strategy_id, "my-strategy");
                    assert_eq!(state_id, "start");
                    assert_eq!(base_revision, 3);
                    assert_eq!(label.as_deref(), Some("begin"));
                    assert_eq!(description.as_deref(), Some("Renamed start"));
                    assert!(json);
                }
                other => panic!("expected state, got {other:?}"),
            },
            other => panic!("expected patch, got {other:?}"),
        }
    }

    #[test]
    fn preset_patch_transition_parses() {
        let cmd = PresetCli::try_parse_from([
            "preset",
            "patch",
            "transition",
            "my-strategy",
            "--base-revision",
            "2",
            "--source-state",
            "start",
            "--op",
            "create",
            "--new-target",
            "end",
            "--transition-kind",
            "branch",
            "--condition",
            "_context._judge_result == true",
        ])
        .unwrap();
        match cmd.command {
            PresetCommand::Patch { command } => match command {
                patch::PatchCommand::Transition {
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
                    assert_eq!(strategy_id, "my-strategy");
                    assert_eq!(base_revision, 2);
                    assert_eq!(source_state, "start");
                    assert!(matches!(op, patch::TransitionOpArg::Create));
                    assert_eq!(old_target, None);
                    assert_eq!(new_target.as_deref(), Some("end"));
                    assert_eq!(condition.as_deref(), Some("_context._judge_result == true"));
                    assert!(matches!(
                        transition_kind,
                        Some(patch::TransitionKindArg::Branch)
                    ));
                    assert!(!json);
                }
                other => panic!("expected transition, got {other:?}"),
            },
            other => panic!("expected patch, got {other:?}"),
        }
    }

    #[test]
    fn preset_patch_prompt_parses() {
        let cmd = PresetCli::try_parse_from([
            "preset",
            "patch",
            "prompt",
            "my-strategy",
            "start",
            "--base-revision",
            "1",
            "--template-ref",
            "prompts/start.md",
            "--file",
            "-",
        ])
        .unwrap();
        match cmd.command {
            PresetCommand::Patch { command } => match command {
                patch::PatchCommand::Prompt {
                    strategy_id,
                    state_id,
                    base_revision,
                    template_ref,
                    file,
                    json,
                } => {
                    assert_eq!(strategy_id, "my-strategy");
                    assert_eq!(state_id, "start");
                    assert_eq!(base_revision, 1);
                    assert_eq!(template_ref, "prompts/start.md");
                    assert_eq!(file, "-");
                    assert!(!json);
                }
                other => panic!("expected prompt, got {other:?}"),
            },
            other => panic!("expected patch, got {other:?}"),
        }
    }

    // ── W-002/F-001: list rows from the grouped core listing ────────────

    /// Grouped core-listing fixture with all three sources.
    fn grouped_fixture() -> ListPresetsResponse {
        use nexus_contracts::generated::daemon_api::preset_management::list_presets_response::{
            NexusPresetSummary, NexusPresetSummarySource,
        };

        fn summary(
            id: &str,
            source: NexusPresetSummarySource,
            run_intents: &[&str],
        ) -> NexusPresetSummary {
            NexusPresetSummary {
                id: id.to_string(),
                source,
                run_intents: run_intents.iter().map(|i| (*i).to_string()).collect(),
            }
        }

        ListPresetsResponse {
            embedded: vec![summary(
                "novel-writing",
                NexusPresetSummarySource::Embedded,
                &["work_init"],
            )],
            system: vec![summary(
                "_system.maintenance",
                NexusPresetSummarySource::System,
                &[],
            )],
            user: vec![summary(
                "my-strategy",
                NexusPresetSummarySource::User,
                &["work_continue"],
            )],
        }
    }

    #[test]
    fn list_rows_include_user_presets_with_real_source_labels() {
        // W-002/F-001: user presets must appear and each row must carry its
        // real source (not a blanket "embedded").
        let rows = build_preset_rows(&grouped_fixture(), None);
        assert_eq!(rows.len(), 3);
        assert!(rows.contains(&(
            "my-strategy".to_string(),
            "user".to_string(),
            vec!["work_continue".to_string()]
        )));
        assert!(rows.contains(&(
            "_system.maintenance".to_string(),
            "system".to_string(),
            vec![]
        )));
        assert!(rows.contains(&(
            "novel-writing".to_string(),
            "embedded".to_string(),
            vec!["work_init".to_string()]
        )));
    }

    #[test]
    fn list_rows_intent_filter_applies_across_groups() {
        // W-002/F-001: `--intent` filtering must work across all groups.
        let rows = build_preset_rows(&grouped_fixture(), Some("work_continue"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "my-strategy");
        assert_eq!(rows[0].1, "user");

        let rows = build_preset_rows(&grouped_fixture(), Some("work_init"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "novel-writing");

        // No match → empty.
        let rows = build_preset_rows(&grouped_fixture(), Some("nonexistent-intent"));
        assert_eq!(
            rows,
            [] as [(
                std::string::String,
                std::string::String,
                std::vec::Vec<std::string::String>
            ); 0]
        );
    }

    /// Minimal profile fixture for the text renderers.
    fn sample_profile() -> PresetProfileResponse {
        PresetProfileResponse {
            id: "novel-writing".to_string(),
            version: 9,
            source_hash: "a".repeat(64),
            lanes: PresetProfileLanes {
                cron: false,
                wall_clock: true,
                session: true,
                direct: true,
            },
            states: vec![PresetProfileState {
                id: "outline_chapter".to_string(),
                description: Some("Draft the chapter outline".to_string()),
                enter: vec![],
                exit_when: None,
                next: None,
                terminal: false,
            }],
            roles: vec![],
            required_capabilities: vec!["judge.llm".to_string()],
            signals: vec![PresetProfileSignal {
                name: "pause_signal".to_string(),
                action: "pause".to_string(),
                target: None,
            }],
        }
    }

    #[test]
    fn show_text_labels_declared_signals_not_delivered() {
        let text = format_profile_text(&sample_profile());
        assert!(
            text.contains("Declared, not delivered"),
            "show must carry the honest signal label, got: {text}"
        );
        assert!(text.contains("pause_signal"));
        assert!(text.contains("Trigger lanes:"));
        assert!(text.contains("wall-clock:  yes"));
        assert!(text.contains("Required capabilities:"));
        assert!(text.contains("judge.llm"));
    }

    #[test]
    fn show_text_omits_absent_manifest_fields() {
        let profile = sample_profile();
        // No roles, no enter actions, no exit_when/next → no invented rows.
        let text = format_profile_text(&profile);
        assert!(text.contains("(single-agent mode)"));
        assert!(!text.contains("exit_when:"));
        assert!(!text.contains("next:"));
        assert!(!text.contains("enter:"));
    }

    #[test]
    fn trigger_text_prints_lanes_only() {
        let lanes = PresetProfileLanes {
            cron: true,
            wall_clock: true,
            session: false,
            direct: true,
        };
        let text = format_trigger_text("novel-brainstorm", &lanes);
        assert!(text.contains("Trigger lanes for novel-brainstorm:"));
        assert!(text.contains("cron:        yes"));
        assert!(text.contains("wall-clock:  yes"));
        assert!(text.contains("session:     no"));
        assert!(text.contains("direct:      yes"));
        // Lanes only — no orchestration sections, no cron authoring.
        assert!(!text.contains("States:"));
        assert!(!text.contains("Declared"));
        assert!(!text.contains("cron add"));
    }

    #[test]
    fn show_json_serializes_dto_verbatim_camel_case() {
        let profile = sample_profile();
        let json = serde_json::to_value(&profile).unwrap();
        // camelCase wire keys (AR-25: no CLI-local renaming).
        assert!(json.get("sourceHash").is_some());
        assert!(json.get("source_hash").is_none());
        assert!(json.get("requiredCapabilities").is_some());
        assert!(json.get("wallClock").is_none());
        let lanes = json.get("lanes").unwrap();
        assert!(lanes.get("wallClock").is_some());
        assert!(lanes.get("wall_clock").is_none());
    }

    /// Minimal preset that passes the shared validation facade (semantic +
    /// assets + path safety). The bundle directory name must equal
    /// `preset.id` (`check_bundle_id_vs_directory`).
    const LOCAL_VALID_YAML: &str = r"
preset:
  id: tiny-valid
  version: 1
  kind: creator
  description: minimal valid fixture for offline validation
  requires_capabilities: []
  run_intents:
    - work_init
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";

    /// Broken copy: `initial` names a state that does not exist (structural
    /// error caught by `loader_validate_manifest_compat`).
    const LOCAL_BROKEN_YAML: &str = r"
preset:
  id: tiny-broken
  version: 1
  kind: creator
  description: broken fixture for offline validation
  requires_capabilities: []
  run_intents:
    - work_init
  initial: missing_state
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
";

    #[test]
    fn local_validate_accepts_valid_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("tiny-valid");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("preset.yaml"), LOCAL_VALID_YAML).unwrap();

        let resp = validate_preset_local(bundle.to_str().unwrap()).unwrap();
        assert_eq!(
            resp["valid"], true,
            "local validation should accept a valid bundle: {resp}"
        );
        assert_eq!(resp["errors"].as_array().unwrap().len(), 0);
        assert_eq!(resp["id"], "tiny-valid");
        assert_eq!(resp["state_count"], 2);
    }

    #[test]
    fn local_validate_rejects_broken_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("tiny-broken");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("preset.yaml"), LOCAL_BROKEN_YAML).unwrap();

        let resp = validate_preset_local(bundle.to_str().unwrap()).unwrap();
        assert_eq!(resp["valid"], false);
        let errors = resp["errors"].as_array().unwrap();
        assert_ne!(errors.as_slice(), &[] as &[serde_json::Value]);
        assert!(
            errors
                .iter()
                .any(|e| e.as_str().unwrap_or_default().contains("unknown state")),
            "expected an 'unknown state' diagnostic, got: {errors:?}"
        );
    }

    #[test]
    fn local_validate_accepts_standalone_yaml_file() {
        // A standalone YAML file (not named preset.yaml) skips asset checks.
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("strategy.yaml");
        std::fs::write(&file, LOCAL_VALID_YAML).unwrap();

        let resp = validate_preset_local(file.to_str().unwrap()).unwrap();
        assert_eq!(
            resp["valid"], true,
            "standalone YAML file should validate: {resp}"
        );
    }

    /// The P0 golden IR and its YAML sample must reach the same verdict through
    /// this one command: `*.ir.json` selects direct IR validation, YAML imports
    /// first, so a divergence means the two input formats disagree on a defect.
    #[test]
    fn local_validate_ir_parity_with_yaml_samples() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for sample in ["game-narrative", "react-trpg-turn"] {
            let yaml = repo_root.join(format!("strategy-samples/{sample}/preset.yaml"));
            let ir = repo_root.join(format!(
                "crates/nexus-preset/tests/fixtures/workflow-ir/{sample}.workflow.ir.json"
            ));

            let from_yaml = validate_preset_local(yaml.to_str().unwrap()).unwrap();
            let from_ir = validate_preset_local(ir.to_str().unwrap()).unwrap();
            for field in ["id", "version", "state_count", "valid", "errors"] {
                assert_eq!(from_ir[field], from_yaml[field], "{sample}: {field}");
            }
        }
    }

    /// The same defect injected into a YAML sample and its golden IR must not
    /// disagree: the IR leg projects manifest-expressible documents back for
    /// the loader-level checks, exactly as YAML runs them, and may add the
    /// IR-native structural detail for the same defect.
    #[test]
    fn local_validate_ir_reports_same_defects_as_yaml() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tmp = tempfile::tempdir().unwrap();

        let mut ir: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_root.join(
                "crates/nexus-preset/tests/fixtures/workflow-ir/game-narrative.workflow.ir.json",
            ))
            .unwrap(),
        )
        .unwrap();
        let edge = ir["edges"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|edge| edge["id"] == "routing->trigger_extract")
            .unwrap();
        edge["to"] = serde_json::json!("trigger_missing");
        let ir_path = tmp.path().join("broken.workflow.ir.json");
        std::fs::write(&ir_path, serde_json::to_string(&ir).unwrap()).unwrap();

        let yaml =
            std::fs::read_to_string(repo_root.join("strategy-samples/game-narrative/preset.yaml"))
                .unwrap()
                .replace("default: trigger_extract", "default: trigger_missing");
        let yaml_path = tmp.path().join("broken.preset.yaml");
        std::fs::write(&yaml_path, yaml).unwrap();

        let from_ir = validate_preset_local(ir_path.to_str().unwrap()).unwrap();
        let from_yaml = validate_preset_local(yaml_path.to_str().unwrap()).unwrap();
        assert_eq!(from_ir["valid"], serde_json::json!(false));
        assert_eq!(from_ir["warnings"], from_yaml["warnings"]);

        let strings = |resp: &serde_json::Value, field: &str| -> Vec<String> {
            resp[field]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        let ir_errors = strings(&from_ir, "errors");
        for error in strings(&from_yaml, "errors") {
            assert!(
                ir_errors.contains(&error),
                "IR is missing the YAML diagnostic `{error}`: {ir_errors:?}"
            );
        }
        // The IR leg additionally names the dangling edge itself.
        assert!(
            ir_errors
                .iter()
                .any(|error| error.contains("references unknown node `trigger_missing`")),
            "expected the dangling-edge diagnostic, got: {ir_errors:?}"
        );
    }

    /// An IR-only document has no manifest projection, so the Q2 structural
    /// checks are the only thing that can report its broken references.
    #[test]
    fn local_validate_ir_reports_structural_defects_without_manifest_projection() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut ir: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_root.join(
                "crates/nexus-preset/tests/fixtures/workflow-ir/game-narrative.workflow.ir.json",
            ))
            .unwrap(),
        )
        .unwrap();
        ir["nodes"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "nested_region",
            "kind": "nest_group",
            "effect_class": "pure",
        }));
        ir["edges"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "nested_region->ghost",
            "from": "nested_region",
            "to": "ghost",
            "kind": "linear",
        }));

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ir_only.workflow.ir.json");
        std::fs::write(&path, serde_json::to_string(&ir).unwrap()).unwrap();
        let path = path.to_str().unwrap();

        let resp = validate_preset_local(path).unwrap();
        assert_eq!(resp["valid"], serde_json::json!(false), "{resp}");
        assert!(
            resp["errors"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|error| error.contains("references unknown node `ghost`")),
            "expected the dangling-edge diagnostic, got: {resp}"
        );
        assert!(
            validate_preset(path, true).is_err(),
            "an invalid verdict must still exit non-zero"
        );
    }

    /// Outer-state loops are legal for imported presets: the Q2 graph-cycle
    /// check must not turn a legal cycle into a validation error.
    #[test]
    fn local_validate_ir_allows_outer_state_cycles() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut ir: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_root.join(
                "crates/nexus-preset/tests/fixtures/workflow-ir/game-narrative.workflow.ir.json",
            ))
            .unwrap(),
        )
        .unwrap();
        // scheduled_sweep -> sweep_extract already exists; close the loop.
        ir["edges"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "sweep_extract->scheduled_sweep",
            "from": "sweep_extract",
            "to": "scheduled_sweep",
            "kind": "conditional",
            "when": "_context.again == true",
        }));

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cycle.workflow.ir.json");
        std::fs::write(&path, serde_json::to_string(&ir).unwrap()).unwrap();

        let resp = validate_preset_local(path.to_str().unwrap()).unwrap();
        assert!(
            resp["errors"].as_array().is_none_or(|errors| {
                !errors
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .any(|error| error.contains("not a DAG"))
            }),
            "a legal outer-state cycle must not be reported: {resp}"
        );
    }

    /// A document from a newer IR version must fail closed instead of letting
    /// the exporter's refusal fall silent, and the verdict must keep the
    /// non-zero exit `validate_preset` maps it to.
    #[test]
    fn local_validate_ir_rejects_future_ir_version() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut ir: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_root.join(
                "crates/nexus-preset/tests/fixtures/workflow-ir/game-narrative.workflow.ir.json",
            ))
            .unwrap(),
        )
        .unwrap();
        ir["ir_version"] = serde_json::json!(1);

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("future.workflow.ir.json");
        std::fs::write(&path, serde_json::to_string(&ir).unwrap()).unwrap();
        let path = path.to_str().unwrap();

        let resp = validate_preset_local(path).unwrap();
        assert_eq!(resp["valid"], serde_json::json!(false), "{resp}");
        assert!(
            resp["errors"][0]
                .as_str()
                .unwrap_or_default()
                .contains("newer than supported 0"),
            "expected the exporter's ir_version refusal, got: {resp}"
        );
        assert!(
            validate_preset(path, true).is_err(),
            "an invalid verdict must still exit non-zero"
        );
    }

    /// The IR leg applies the same document size gate as the YAML leg.
    #[test]
    fn local_validate_ir_rejects_oversize_document() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("huge.workflow.ir.json");
        std::fs::write(&file, vec![b' '; nexus_preset::DEFAULT_MAX_YAML_SIZE + 1]).unwrap();

        let err = validate_preset_local(file.to_str().unwrap()).unwrap_err();
        assert!(
            err.to_string().contains("exceeds maximum size"),
            "expected the shared size gate error, got: {err}"
        );
    }

    #[test]
    fn local_validate_missing_path_errors() {
        let err = validate_preset_local("/nonexistent/definitely-missing").unwrap_err();
        assert!(
            err.to_string().contains("File not found"),
            "expected 'File not found' error, got: {err}"
        );
    }
}
