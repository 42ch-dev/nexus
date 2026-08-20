//! `nexus42 compute` — compute-module authoring loop (V1.170 P0, AR-9).
//!
//! Subcommands: `build`, `validate`, `install` are **daemon-free** (the author
//! loop needs no runtime); `run` is a thin HTTP client over
//! `POST /v1/daemon/compute/run` (+ optional `--accept`).
//!
//! Exit-code contract (AR-9):
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | success |
//! | 1    | build/toolchain failure; install I/O/home errors; `run` --input read/parse failures (generic CLI failure) |
//! | 2    | manifest validation failure (field list; `--json` machine-readable) |
//! | 3    | `wasm_sha256` pairing mismatch (`validate --wasm`, `install`) |
//! | 4    | daemon unreachable / run rejected (daemon error surfaced verbatim) |
//!
//! The group carries no `connect-host` feature dependency — the default daemon
//! graph stays libp2p-free.

use clap::Subcommand;
use nexus_module_manifest::{inject_wasm_sha256, ModuleManifest};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// AR-9 exit codes.
mod exit {
    /// Build/toolchain failure; also the generic CLI failure code for
    /// install I/O/home errors and `run` --input read/parse failures
    /// (AR-9 reserves 2/3/4 for validation, pairing, and daemon failures).
    pub const BUILD: i32 = 1;
    /// Install I/O / home-resolution failure; `run` --input read/parse
    /// failures (generic CLI failure code).
    // NOTE (qc1 S-4): BUILD and IO deliberately alias at 1 — AR-9 assigns
    // one generic CLI-failure code to all local I/O/CLI classes; the two
    // names document the call sites' intent, not two distinct codes.
    pub const IO: i32 = 1;
    /// Manifest validation failure.
    pub const VALIDATION: i32 = 2;
    /// `wasm_sha256` pairing mismatch.
    pub const PAIRING: i32 = 3;
    /// Daemon unreachable / run rejected.
    pub const DAEMON: i32 = 4;
}

/// Compute module subcommands (AR-9 table).
#[derive(Debug, Subcommand)]
pub enum ComputeCommand {
    /// Build a compute module daemon-free: cargo build → locate the wasm
    /// artifact → inject `wasm_sha256` → stage the pair under
    /// `<module-dir>/dist/<module_id>/`.
    Build {
        /// Path to the module's `manifest.json` (module dir = manifest parent).
        #[arg(long)]
        manifest: PathBuf,
        /// Build the release profile (default: debug).
        #[arg(long)]
        release: bool,
    },
    /// Validate a module manifest, optionally verifying `wasm_sha256`
    /// pairing against the compiled `.wasm`.
    Validate {
        /// Path to the module's `manifest.json`.
        #[arg(long)]
        manifest: PathBuf,
        /// Also verify `wasm_sha256` pairing against this `.wasm` file.
        #[arg(long)]
        wasm: Option<PathBuf>,
        /// Emit machine-readable field-level errors (JSON).
        #[arg(long)]
        json: bool,
    },
    /// Re-verify pairing and install a compiled pair into
    /// `~/.nexus42/modules/<id>/` (`<id>/<id>.wasm` + `<id>/manifest.json` —
    /// the daemon's `warm_dir` scan contract).
    Install {
        /// Module id (must match the manifest's `module_id`).
        #[arg(long)]
        module_id: String,
        /// Path to the module's `manifest.json`.
        #[arg(long)]
        manifest: PathBuf,
        /// Path to the compiled `.wasm`.
        #[arg(long)]
        wasm: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run an installed module through the daemon (the one daemon-backed
    /// subcommand). Thin HTTP client over `POST /v1/daemon/compute/run` +
    /// optional `POST /v1/daemon/compute/runs/:run_id/accept`.
    ///
    /// Output format comes from the CLI-wide `--output text|json` flag
    /// (AR-9 `[--output text|json]` — the global flag is propagated into the
    /// subcommand, so no duplicate `--output` long name exists here).
    Run {
        /// World id to run against.
        #[arg(long)]
        world: String,
        /// Input JSON fixture: a `ComputeInput` envelope (its `invocation`
        /// field is sent) or a raw `invocation_params` object. Local
        /// read/parse failures exit 1; daemon failures exit 4.
        #[arg(long)]
        input: PathBuf,
        /// Installed module id (default: read `module_id` from a
        /// `manifest.json` beside the input fixture).
        #[arg(long)]
        module_id: Option<String>,
        /// Accept the run's proposals after a successful run.
        #[arg(long)]
        accept: bool,
    },
}

/// Output mode for `compute run` — resolved from the CLI-wide `--output`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutput {
    Text,
    Json,
}

/// Run the compute command group.
///
/// # Errors
///
/// Returns [`CliError::ComputeExit`] with the AR-9 exit code on failure.
pub async fn run(cmd: ComputeCommand, config: &CliConfig, output_format: &str) -> Result<()> {
    match cmd {
        ComputeCommand::Build { manifest, release } => cmd_build(&manifest, release),
        ComputeCommand::Validate {
            manifest,
            wasm,
            json,
        } => cmd_validate(&manifest, wasm.as_deref(), json),
        ComputeCommand::Install {
            module_id,
            manifest,
            wasm,
            json,
        } => cmd_install(&module_id, &manifest, &wasm, json),
        ComputeCommand::Run {
            world,
            input,
            module_id,
            accept,
        } => {
            let output = match output_format {
                "json" => RunOutput::Json,
                _ => RunOutput::Text,
            };
            cmd_run(config, &world, &input, module_id.as_deref(), accept, output).await
        }
    }
}

// ─── compute build ─────────────────────────────────────────────────────────

/// Build a module daemon-free and stage the pair under `dist/<module_id>/`.
#[allow(clippy::too_many_lines)]
fn cmd_build(manifest_path: &Path, release: bool) -> Result<()> {
    let module_dir = manifest_path.parent().ok_or_else(|| {
        compute_exit(
            exit::BUILD,
            format!(
                "--manifest must be a file path, got `{}`",
                manifest_path.display()
            ),
        )
    })?;

    let manifest_bytes = read_json_file(exit::BUILD, manifest_path, "manifest")?;
    let manifest: ModuleManifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        compute_exit(
            exit::BUILD,
            format!("failed to parse {}: {e}", manifest_path.display()),
        )
    })?;
    if let Err(errs) = manifest.validate() {
        return Err(compute_exit(
            exit::VALIDATION,
            format!("manifest invalid: {}", errs.join("; ")),
        ));
    }
    let module_id = &manifest.module_id;

    // The manifest-supplied id becomes a directory name under dist/ — apply
    // the same single-path-component safety rule as install before staging
    // (I4): a value containing `../` or an absolute path must not escape the
    // intended dist tree.
    nexus_home_layout::validate_run_id_safe(module_id).map_err(|e| {
        compute_exit(
            exit::VALIDATION,
            format!("invalid manifest.module_id {module_id:?}: {e}"),
        )
    })?;

    // Same invocation the wasm-host build.rs uses (compile_module).
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--target")
        .arg("wasm32-unknown-unknown")
        .current_dir(module_dir);
    if release {
        cmd.arg("--release");
    }
    let output = cmd.output().map_err(|e| {
        compute_exit(
            exit::BUILD,
            format!(
                "failed to invoke `cargo` to build module `{module_id}` in {}: {e} — is `cargo` on PATH?",
                module_dir.display()
            ),
        )
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_missing_target_error(&stderr) {
            return Err(compute_exit(
                exit::BUILD,
                format!(
                    "wasm32-unknown-unknown target not installed — required to compile \
                     module `{module_id}`.\nFix: rustup target add wasm32-unknown-unknown"
                ),
            ));
        }
        return Err(compute_exit(
            exit::BUILD,
            format!("failed to compile module `{module_id}`:\n{stderr}"),
        ));
    }

    // Locate the artifact. cdylib artifact names use underscores (crate name
    // `basic-combat` → `basic_combat.wasm`); CARGO_TARGET_DIR may relocate
    // the target dir (same resolution the wasm-host build.rs uses).
    let profile = if release { "release" } else { "debug" };
    let crate_name =
        crate_name_from_cargo_toml(module_dir).unwrap_or_else(|| module_id.replace('-', "_"));
    let artifact = artifact_path(module_dir, profile, &crate_name);
    if !artifact.is_file() {
        return Err(compute_exit(
            exit::BUILD,
            format!(
                "expected artifact not found at `{}` — the module build produced no \
                 wasm artifact (check `[lib] crate-type = [\"cdylib\"]` and the package name)",
                artifact.display()
            ),
        ));
    }

    // Stage the pair under dist/<module_id>/; the source manifest is never
    // mutated (wasm-host build.rs precedent).
    let dist_dir = module_dir.join("dist").join(module_id);
    std::fs::create_dir_all(&dist_dir).map_err(|e| {
        compute_exit(
            exit::BUILD,
            format!("failed to create {}: {e}", dist_dir.display()),
        )
    })?;
    let dest_wasm = dist_dir.join(format!("{module_id}.wasm"));
    let dest_manifest = dist_dir.join("manifest.json");
    std::fs::copy(&artifact, &dest_wasm).map_err(|e| {
        compute_exit(
            exit::BUILD,
            format!(
                "failed to copy {} → {}: {e}",
                artifact.display(),
                dest_wasm.display()
            ),
        )
    })?;
    std::fs::copy(manifest_path, &dest_manifest).map_err(|e| {
        compute_exit(
            exit::BUILD,
            format!(
                "failed to copy {} → {}: {e}",
                manifest_path.display(),
                dest_manifest.display()
            ),
        )
    })?;
    inject_wasm_sha256(module_id, &dest_wasm, &dest_manifest)
        .map_err(|e| compute_exit(exit::BUILD, e))?;

    println!(
        "built module `{module_id}` (v{}) — {}",
        manifest.version,
        artifact.display()
    );
    println!(
        "staged pair under {} (wasm_sha256 injected)",
        dist_dir.display()
    );
    Ok(())
}

// ─── compute validate ──────────────────────────────────────────────────────

/// Validate a manifest (and optionally its `wasm_sha256` pairing).
///
/// Exit 2 on manifest failure (field-level), exit 3 on pairing mismatch.
fn cmd_validate(manifest_path: &Path, wasm_path: Option<&Path>, json_output: bool) -> Result<()> {
    // A missing/unreadable manifest is a validation failure with the same
    // field-level shape as any other error — `--json` callers must get the
    // promised `{valid, manifest, errors}` document, not a bare message (I8).
    let manifest_bytes = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return fail_validation(
                manifest_path,
                &[FieldError {
                    field: "manifest".to_string(),
                    message: format!("cannot read {}: {e}", manifest_path.display()),
                }],
                json_output,
                exit::VALIDATION,
            );
        }
    };
    let manifest: ModuleManifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            return fail_validation(
                manifest_path,
                &[field_error(e.to_string())],
                json_output,
                exit::VALIDATION,
            );
        }
    };

    let mut errors: Vec<FieldError> = Vec::new();
    if let Err(errs) = manifest.validate() {
        errors.extend(errs.into_iter().map(field_error));
    }

    // qc2 W-1: the module id becomes a directory name under dist/ (build)
    // and ~/.nexus42/modules/<id>/ (install), so `validate` must not report
    // `valid: true` for a traversal id the authoring loop cannot stage —
    // build/install already reject it (exit 2); validate now agrees
    // (defense in depth, same field-level shape).
    if let Err(e) = nexus_home_layout::validate_run_id_safe(&manifest.module_id) {
        errors.push(FieldError {
            field: "module_id".to_string(),
            message: e,
        });
    }

    if let Some(wasm_path) = wasm_path {
        // qc2 S-1: `--wasm` requests pairing verification, so an absent
        // `wasm_sha256` cannot be paired — reject with the pairing exit
        // code (3) and a field-level error instead of silently passing
        // (mirrors `install`, which requires the hash, I10).
        if manifest.wasm_sha256.is_none() {
            return fail_validation(
                manifest_path,
                &[FieldError {
                    field: "wasm_sha256".to_string(),
                    message: "hash required for pairing verification".to_string(),
                }],
                json_output,
                exit::PAIRING,
            );
        }
        let wasm_bytes = match std::fs::read(wasm_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                return fail_validation(
                    manifest_path,
                    &[FieldError {
                        field: "wasm".to_string(),
                        message: format!("cannot read {}: {e}", wasm_path.display()),
                    }],
                    json_output,
                    exit::VALIDATION,
                );
            }
        };
        if let Err(pair_err) = manifest.verify_wasm_sha256(&wasm_bytes) {
            // AR-9: pairing mismatch is exit 3 (distinct from validation
            // failures, exit 2).
            return fail_validation(
                manifest_path,
                &[FieldError {
                    field: "wasm_sha256".to_string(),
                    message: pair_err,
                }],
                json_output,
                exit::PAIRING,
            );
        }
    }

    if errors.is_empty() {
        if json_output {
            println!(
                "{}",
                serde_json::json!({ "valid": true, "manifest": manifest_path.display().to_string() })
            );
        } else {
            println!("✓ Valid manifest: {}", manifest_path.display());
        }
        Ok(())
    } else {
        fail_validation(manifest_path, &errors, json_output, exit::VALIDATION)
    }
}

/// Print the failure verdict (text or `--json`) and return the AR-9 error.
///
/// `--json` output is machine-readable field-level errors:
/// `{"valid": false, "manifest": "<path>", "errors": [{"field", "message"}]}`.
fn fail_validation(
    manifest_path: &Path,
    errors: &[FieldError],
    json_output: bool,
    exit_code: i32,
) -> Result<()> {
    if json_output {
        println!("{}", validation_failure_json(manifest_path, errors));
    } else {
        println!("✗ Invalid manifest ({} error(s)):", errors.len());
        for e in errors {
            println!("  - {}: {}", e.field, e.message);
        }
    }
    Err(compute_exit(
        exit_code,
        format!("manifest validation failed: {} error(s)", errors.len()),
    ))
}

/// The machine-readable `--json` failure verdict (pure formatter so tests
/// pin the exact shape without capturing stdout).
fn validation_failure_json(manifest_path: &Path, errors: &[FieldError]) -> String {
    let errors_json: Vec<Value> = errors
        .iter()
        .map(|e| serde_json::json!({ "field": e.field, "message": e.message }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "valid": false,
        "manifest": manifest_path.display().to_string(),
        "errors": errors_json,
    }))
    .expect("json serialization cannot fail")
}

/// A field-level validation error.
struct FieldError {
    field: String,
    message: String,
}

/// Derive the field name from a validation message: the backticked serde
/// field (e.g. `missing field module_id`) or the leading token
/// (`nexus_abi_version must be 1 …`).
fn field_error(message: String) -> FieldError {
    let field = message
        .find('`')
        .and_then(|start| {
            // Extract the bytes BETWEEN the two backticks — slicing up to
            // the closing backtick and re-splitting yields `missing field `,
            // not the field name (C1).
            message[start + 1..]
                .find('`')
                .map(|end| message[start + 1..start + 1 + end].to_string())
        })
        .filter(|f| !f.is_empty())
        .or_else(|| {
            message
                .split_whitespace()
                .next()
                .map(|tok| tok.trim_end_matches(':').to_string())
        })
        .unwrap_or_else(|| "manifest".to_string());
    FieldError { field, message }
}

// ─── compute install ───────────────────────────────────────────────────────

/// Re-verify pairing and copy the pair into `~/.nexus42/modules/<id>/`.
///
/// Exit vocabulary (I1): 2 = validation failure (bad module id, unreadable/
/// unparseable manifest, `--module-id`/manifest identity mismatch), 3 =
/// `wasm_sha256` pairing failure (absent or mismatched hash), 1 = install
/// I/O/home failure (generic CLI failure — AR-9 reserves 2/3/4 for
/// validation, pairing, and daemon failures).
fn cmd_install(
    module_id: &str,
    manifest_path: &Path,
    wasm_path: &Path,
    json_output: bool,
) -> Result<()> {
    // Path-traversal guard — the module id becomes a directory name.
    nexus_home_layout::validate_run_id_safe(module_id).map_err(|e| {
        compute_exit(
            exit::VALIDATION,
            format!("invalid module id {module_id:?}: {e}"),
        )
    })?;

    // Manifest read/parse failures are validation failures (exit 2).
    let manifest_bytes = read_json_file(exit::VALIDATION, manifest_path, "manifest")?;
    let manifest: ModuleManifest = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        compute_exit(
            exit::VALIDATION,
            format!("failed to parse {}: {e}", manifest_path.display()),
        )
    })?;

    // The advertised module-id/manifest identity must match (I2): staging a
    // manifest under a directory keyed by a different id would create an
    // invalid/ambiguous store pair the daemon loader cannot repair.
    if manifest.module_id != module_id {
        return Err(compute_exit(
            exit::VALIDATION,
            format!(
                "--module-id {module_id:?} does not match manifest.module_id {:?}",
                manifest.module_id
            ),
        ));
    }

    // AR-9 pairing: install REQUIRES a content hash (I10). An absent
    // `wasm_sha256` bypasses the pairing requirement and permits a
    // manifest/wasm mismatch; the staged `build` output always injects one.
    if manifest.wasm_sha256.is_none() {
        return Err(compute_exit(
            exit::PAIRING,
            "manifest has no wasm_sha256 — install requires a content hash; \
             run `nexus42 compute build` to stage a pair with the hash injected"
                .to_string(),
        ));
    }
    let wasm_bytes = std::fs::read(wasm_path).map_err(|e| {
        compute_exit(
            exit::IO,
            format!("failed to read {}: {e}", wasm_path.display()),
        )
    })?;

    // Re-verify pairing (AR-9): exit 3 on mismatch.
    manifest
        .verify_wasm_sha256(&wasm_bytes)
        .map_err(|e| compute_exit(exit::PAIRING, e))?;

    let home =
        dirs::home_dir().ok_or_else(|| compute_exit(exit::IO, "cannot resolve home directory"))?;
    let dir = nexus_home_layout::user_modules_dir(&home).join(module_id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| compute_exit(exit::IO, format!("failed to create {}: {e}", dir.display())))?;
    std::fs::copy(wasm_path, dir.join(format!("{module_id}.wasm")))
        .map_err(|e| compute_exit(exit::IO, format!("failed to install {module_id}.wasm: {e}")))?;
    std::fs::copy(manifest_path, dir.join("manifest.json"))
        .map_err(|e| compute_exit(exit::IO, format!("failed to install manifest.json: {e}")))?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "installed": true,
                "module_id": module_id,
                "path": dir.display().to_string(),
            }))
            .expect("json serialization cannot fail")
        );
    } else {
        println!(
            "installed module `{module_id}` → {} (daemon picks the pair up on next boot)",
            dir.display()
        );
    }
    Ok(())
}

// ─── compute run ───────────────────────────────────────────────────────────

/// Thin HTTP client over `POST /v1/daemon/compute/run` (+ `--accept`).
///
/// Local `--input` read/parse failures (and fixture module-id resolution
/// failures) exit 1 (generic CLI failure, qc2 S-3 / qc3 F-004); daemon
/// unreachable / run rejected exit 4 (AR-9), with the daemon error surfaced
/// verbatim.
#[allow(clippy::too_many_lines)]
async fn cmd_run(
    config: &CliConfig,
    world: &str,
    input_path: &Path,
    module_id: Option<&str>,
    accept: bool,
    output: RunOutput,
) -> Result<()> {
    let input_bytes = std::fs::read(input_path).map_err(|e| {
        compute_exit(
            exit::IO,
            format!("failed to read --input {}: {e}", input_path.display()),
        )
    })?;
    let input_value: Value = serde_json::from_slice(&input_bytes).map_err(|e| {
        compute_exit(
            exit::IO,
            format!("--input {} is not valid JSON: {e}", input_path.display()),
        )
    })?;
    // A ComputeInput envelope carries module params in `invocation`; a raw
    // params file is used as-is.
    let invocation_params = input_value
        .get("invocation")
        .cloned()
        .unwrap_or_else(|| input_value.clone());
    let invocation_params = invocation_params.as_object().ok_or_else(|| {
        compute_exit(
            exit::IO,
            "--input must be a JSON object (or a ComputeInput envelope whose \
             `invocation` field is an object)"
                .to_string(),
        )
    })?;

    let module_id = match module_id {
        Some(id) => id.to_string(),
        None => resolve_module_id_from_fixture(input_path)?,
    };

    let client = DaemonClient::from_config(config);
    let body = serde_json::json!({
        "world_id": world,
        "module_id": module_id,
        "invocation_params": invocation_params,
    });
    let resp: Value = client
        .post_raw("/v1/daemon/compute/run", &body)
        .await
        .map_err(|e| {
            let hint = if matches!(e, CliError::Network(_)) {
                "\n  Hint: `compute run` is daemon-backed — start the daemon with `nexus42 daemon start`"
            } else {
                ""
            };
            compute_exit(exit::DAEMON, format!("compute run failed: {e}{hint}"))
        })?;

    // AR-9: a rejected/failed run exits 4 with the daemon error surfaced
    // verbatim; accept is never called for a failed or missing run id (I6).
    let run_id = resp.get("run_id").and_then(Value::as_str).unwrap_or("");
    let status = resp.get("status").and_then(Value::as_str).unwrap_or("");
    if run_id.is_empty() {
        return Err(compute_exit(
            exit::DAEMON,
            "compute run failed: daemon response has no run_id".to_string(),
        ));
    }
    if status != "succeeded" {
        let detail = resp.get("error").map_or_else(
            || "no error detail in response".to_string(),
            Value::to_string,
        );
        return Err(compute_exit(
            exit::DAEMON,
            format!("compute run rejected: status {status:?} — {detail}"),
        ));
    }

    if accept {
        let accept_body = serde_json::json!({});
        let accept_resp: Value = client
            .post_raw(
                &format!("/v1/daemon/compute/runs/{run_id}/accept"),
                &accept_body,
            )
            .await
            .map_err(|e| compute_exit(exit::DAEMON, format!("accept failed: {e}")))?;
        match output {
            RunOutput::Json => {
                // Single JSON envelope (I5): exactly one machine-readable
                // document carrying both responses.
                println!("{}", run_json_output(&resp, Some(&accept_resp)));
            }
            RunOutput::Text => {
                print_run_text(&resp, run_id, &module_id, status);
                let applied = accept_resp
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("applied");
                let n_events = accept_resp
                    .get("timeline_event_ids")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
                println!("accepted run {run_id} ({applied}, {n_events} timeline event(s))");
            }
        }
    } else {
        match output {
            RunOutput::Json => println!("{}", run_json_output(&resp, None)),
            RunOutput::Text => print_run_text(&resp, run_id, &module_id, status),
        }
    }

    Ok(())
}

/// Render the run response as text (shared by the accept and no-accept
/// paths so the two cannot drift).
fn print_run_text(resp: &Value, run_id: &str, module_id: &str, status: &str) {
    println!("run {run_id} · module `{module_id}` · status {status}");
    if let Some(proposals) = resp.get("proposals") {
        if proposals.is_null() {
            println!("  proposals: none (check status/error)");
        } else {
            println!(
                "  proposals: {} state delta(s), {} timeline event(s), {} new key block(s)",
                proposals
                    .get("state_delta")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                proposals
                    .get("timeline_events")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                proposals
                    .get("new_key_blocks")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            );
        }
    }
    if let Some(err) = resp.get("error") {
        println!("  daemon error: {err}");
    }
}

/// Build the single JSON document printed for `compute run` (AR-9 wire
/// discipline: exactly one JSON value on stdout, even with `--accept`).
fn run_json_output(run_resp: &Value, accept_resp: Option<&Value>) -> String {
    let envelope = accept_resp.map_or_else(
        || run_resp.clone(),
        |accept| serde_json::json!({ "run": run_resp, "accept": accept }),
    );
    serde_json::to_string_pretty(&envelope).expect("json serialization cannot fail")
}

/// Resolve the module id from a `manifest.json` beside the input fixture
/// (fixture dir, then its parent — the module dir layout).
fn resolve_module_id_from_fixture(input_path: &Path) -> Result<String> {
    let mut dir = input_path.parent();
    for _ in 0..2 {
        let Some(d) = dir else { break };
        let candidate = d.join("manifest.json");
        if candidate.is_file() {
            if let Ok(bytes) = std::fs::read(&candidate) {
                if let Ok(m) = serde_json::from_slice::<ModuleManifest>(&bytes) {
                    return Ok(m.module_id);
                }
            }
        }
        dir = d.parent();
    }
    Err(compute_exit(
        exit::IO,
        "cannot resolve module id: pass --module-id, or place the --input \
         fixture next to a manifest.json (e.g. <module-dir>/fixtures/)"
            .to_string(),
    ))
}

// ─── shared helpers ────────────────────────────────────────────────────────

/// Build a [`CliError::ComputeExit`] with an AR-9 exit code.
fn compute_exit(code: i32, message: impl Into<String>) -> CliError {
    CliError::ComputeExit {
        code,
        message: message.into(),
    }
}

/// Read a JSON file, failing with `exit` on I/O error.
fn read_json_file(exit_code: i32, path: &Path, label: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| {
        compute_exit(
            exit_code,
            format!("failed to read {label} {}: {e}", path.display()),
        )
    })
}

/// Detects the rustc/cargo error emitted when the wasm sysroot is absent —
/// the overwhelmingly common cause of a module build failure (mirrors the
/// wasm-host build.rs `is_missing_target_error`).
fn is_missing_target_error(stderr: &str) -> bool {
    stderr.contains("can't find crate for `core`")
        || stderr.contains("can't find crate for `std`")
        || stderr.contains("does not have a standard library preinstalled")
        || stderr.contains("rust-std")
}

/// Resolve the cdylib artifact path, honoring `CARGO_TARGET_DIR` (same
/// resolution as the wasm-host build.rs).
fn artifact_path(module_dir: &Path, profile: &str, crate_name: &str) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || {
            module_dir
                .join("target")
                .join("wasm32-unknown-unknown")
                .join(profile)
                .join(format!("{crate_name}.wasm"))
        },
        |dir| {
            PathBuf::from(dir)
                .join("wasm32-unknown-unknown")
                .join(profile)
                .join(format!("{crate_name}.wasm"))
        },
    )
}

/// Read the module's `[package] name` from its `Cargo.toml` (cdylib artifact
/// names use underscores).
fn crate_name_from_cargo_toml(module_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(module_dir.join("Cargo.toml")).ok()?;
    let value: toml::Value = text.parse().ok()?;
    let name = value.get("package")?.get("name")?.as_str()?;
    Some(name.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn field_error_extracts_backticked_field() {
        let e = field_error("missing field `module_id` at line 1 column 8".to_string());
        assert_eq!(e.field, "module_id");
        assert_eq!(e.message, "missing field `module_id` at line 1 column 8");
    }

    #[test]
    fn field_error_falls_back_to_leading_token() {
        let e = field_error("nexus_abi_version must be 1 (ABI V1), got 2".to_string());
        assert_eq!(e.field, "nexus_abi_version");
        let e = field_error("wasm_sha256 must be 64 lowercase hex characters".to_string());
        assert_eq!(e.field, "wasm_sha256");
    }

    #[test]
    fn field_error_empty_message_uses_manifest() {
        assert_eq!(field_error(String::new()).field, "manifest");
    }

    #[test]
    fn field_error_extracts_unknown_field_backtick() {
        // serde unknown-field errors also backtick the field name.
        let e = field_error("unknown field `battle_report_kind`, expected one of ...".to_string());
        assert_eq!(e.field, "battle_report_kind");
    }

    #[test]
    fn run_json_output_accept_is_single_envelope() {
        // I5: `--output json --accept` must emit exactly ONE JSON document
        // carrying both responses — not two adjacent JSON values.
        let run = json!({"run_id": "run_1", "status": "succeeded"});
        let accept = json!({"run_id": "run_1", "status": "applied"});
        let out = run_json_output(&run, Some(&accept));
        let parsed: Value = serde_json::from_str(&out).expect("single JSON document");
        assert_eq!(parsed["run"]["run_id"], "run_1");
        assert_eq!(parsed["accept"]["status"], "applied");
    }

    #[test]
    fn run_json_output_without_accept_is_run_only() {
        let run = json!({"run_id": "run_1", "status": "succeeded"});
        let out = run_json_output(&run, None);
        let parsed: Value = serde_json::from_str(&out).expect("single JSON document");
        assert_eq!(parsed["run_id"], "run_1");
        assert!(parsed.get("accept").is_none());
    }

    #[test]
    fn validation_failure_json_shape() {
        // I8: the `--json` failure verdict is the promised
        // `{valid, manifest, errors}` shape with field-level errors.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("manifest.json");
        let out = validation_failure_json(
            &path,
            &[FieldError {
                field: "manifest".to_string(),
                message: "cannot read".to_string(),
            }],
        );
        let parsed: Value = serde_json::from_str(&out).expect("single JSON document");
        assert_eq!(parsed["valid"], false);
        assert_eq!(parsed["manifest"], path.display().to_string());
        assert_eq!(parsed["errors"][0]["field"], "manifest");
        assert_eq!(parsed["errors"][0]["message"], "cannot read");
    }

    #[test]
    fn cmd_validate_read_failure_exits_validation() {
        // I8: a missing/unreadable manifest is a validation failure (exit 2)
        // routed through the same failure path as field errors.
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.json");
        let err = cmd_validate(&missing, None, true).expect_err("missing manifest must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "read failure must exit 2, got {err}"
        );
    }

    fn valid_manifest_json(module_id: &str) -> serde_json::Value {
        json!({
            "module_id": module_id,
            "name": "Test Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": ["character"],
            "compute_export": "compute",
            "init_export": "init",
        })
    }

    #[test]
    fn cmd_install_rejects_module_id_mismatch() {
        // I2: `--module-id` must match `manifest.module_id` — a mismatch is
        // rejected before any copy (exit 2, validation).
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&valid_manifest_json("basic-combat")).expect("json"),
        )
        .expect("write manifest");
        let wasm_path = dir.path().join("module.wasm");
        std::fs::write(&wasm_path, b"wasm bytes").expect("write wasm");

        let err = cmd_install("alias", &manifest_path, &wasm_path, false)
            .expect_err("id mismatch must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "id mismatch must exit 2, got {err}"
        );
        // No store pair may be created.
        assert!(!dir.path().join(".nexus42").exists());
    }

    #[test]
    fn cmd_install_rejects_absent_hash() {
        // I10: install REQUIRES `wasm_sha256` — an absent hash bypasses the
        // AR-9 pairing requirement and must be rejected (exit 3, pairing).
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&valid_manifest_json("basic-combat")).expect("json"),
        )
        .expect("write manifest");
        let wasm_path = dir.path().join("module.wasm");
        std::fs::write(&wasm_path, b"wasm bytes").expect("write wasm");

        let err = cmd_install("basic-combat", &manifest_path, &wasm_path, false)
            .expect_err("absent hash must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 3, .. }),
            "absent hash must exit 3, got {err}"
        );
        assert!(!dir.path().join(".nexus42").exists());
    }

    #[test]
    fn cmd_build_rejects_path_traversal_module_id() {
        // I4: the manifest-supplied module id becomes a directory name under
        // dist/ — a traversal value must be rejected before staging (exit 2).
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&valid_manifest_json("../evil")).expect("json"),
        )
        .expect("write manifest");

        let err = cmd_build(&manifest_path, false).expect_err("traversal id must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "traversal module id must exit 2, got {err}"
        );
        assert!(!dir.path().join("dist").exists());
    }

    #[test]
    fn cmd_validate_rejects_path_traversal_module_id() {
        // qc2 W-1: `validate` must not report a traversal module id as
        // valid — the id becomes a directory name under dist/ and the
        // module store, so the authoring loop's validator agrees with
        // build/install (exit 2, field-level error on module_id).
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest_path = dir.path().join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&valid_manifest_json("../evil")).expect("json"),
        )
        .expect("write manifest");

        let err = cmd_validate(&manifest_path, None, true).expect_err("traversal id must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "traversal module id must exit 2, got {err}"
        );
        // The field-level module_id error shape is pinned by the hermetic
        // test (compute_cli.rs) and validation_failure_json tests.
    }

    #[tokio::test]
    async fn cmd_run_missing_input_exits_io_code() {
        // qc2 S-3 / qc3 F-004: a missing `--input` fixture is a LOCAL I/O
        // error (exit 1), NOT a daemon failure (exit 4) — the AR-9 exit
        // table reserves 4 for daemon-unreachable/run-rejected only.
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing-input.json");
        let config = CliConfig {
            daemon_url: "http://127.0.0.1:1".to_string(),
            ..Default::default()
        };
        let err = cmd_run(
            &config,
            "wld_combat",
            &missing,
            Some("basic-combat"),
            false,
            RunOutput::Text,
        )
        .await
        .expect_err("missing input must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 1, .. }),
            "missing input must exit 1, got {err}"
        );
    }

    #[tokio::test]
    async fn cmd_run_invalid_json_input_exits_io_code() {
        // qc2 S-3: an unparseable `--input` fixture is a local CLI error
        // (exit 1), not a daemon failure (exit 4).
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("input.json");
        std::fs::write(&fixture, b"not json").expect("write fixture");
        let config = CliConfig {
            daemon_url: "http://127.0.0.1:1".to_string(),
            ..Default::default()
        };
        let err = cmd_run(
            &config,
            "wld_combat",
            &fixture,
            Some("basic-combat"),
            false,
            RunOutput::Text,
        )
        .await
        .expect_err("invalid input JSON must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 1, .. }),
            "invalid input JSON must exit 1, got {err}"
        );
    }

    #[tokio::test]
    async fn cmd_run_failed_status_exits_daemon_code() {
        // I6: a `status: "failed"` run exits 4 with the daemon error surfaced
        // and accept is NEVER called.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/daemon/compute/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "run_id": "run_failed",
                "module_id": "basic-combat",
                "module_version": "1.0.0",
                "status": "failed",
                "error": {"code": "sandbox_limit", "message": "fuel exhausted"},
            })))
            .expect(1)
            .mount(&server)
            .await;

        // Accept must never be called for a failed run.
        Mock::given(method("POST"))
            .and(path("/v1/daemon/compute/runs/run_failed/accept"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(0)
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("input.json");
        std::fs::write(
            &fixture,
            serde_json::to_vec(&json!({"a": 1})).expect("json"),
        )
        .expect("write fixture");

        let config = CliConfig {
            daemon_url: server.uri(),
            ..Default::default()
        };
        let err = cmd_run(
            &config,
            "wld_combat",
            &fixture,
            Some("basic-combat"),
            true,
            RunOutput::Json,
        )
        .await
        .expect_err("failed run must exit 4");
        assert!(
            matches!(err, CliError::ComputeExit { code: 4, .. }),
            "failed run must exit 4, got {err}"
        );
        assert!(
            err.to_string().contains("fuel exhausted"),
            "daemon error must be surfaced verbatim: {err}"
        );
        server.verify().await;
    }

    #[tokio::test]
    async fn cmd_run_missing_run_id_exits_daemon_code() {
        // I6: a malformed successful response without a run id is rejected
        // (exit 4) — accept is never attempted with a placeholder id.
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/daemon/compute/run"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "succeeded",
                "proposals": {"state_delta": []},
            })))
            .expect(1)
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("input.json");
        std::fs::write(
            &fixture,
            serde_json::to_vec(&json!({"a": 1})).expect("json"),
        )
        .expect("write fixture");

        let config = CliConfig {
            daemon_url: server.uri(),
            ..Default::default()
        };
        let err = cmd_run(
            &config,
            "wld_combat",
            &fixture,
            Some("basic-combat"),
            true,
            RunOutput::Json,
        )
        .await
        .expect_err("missing run_id must exit 4");
        assert!(
            matches!(err, CliError::ComputeExit { code: 4, .. }),
            "missing run_id must exit 4, got {err}"
        );
        server.verify().await;
    }

    #[test]
    fn is_missing_target_error_detects_sysroot_absence() {
        assert!(is_missing_target_error(
            "error: can't find crate for `core`"
        ));
        assert!(is_missing_target_error("rust-std not found"));
        assert!(!is_missing_target_error("error[E0308]: mismatched types"));
    }

    #[test]
    #[serial_test::serial]
    fn artifact_path_uses_local_target_dir() {
        let original = std::env::var_os("CARGO_TARGET_DIR");
        std::env::remove_var("CARGO_TARGET_DIR");
        let dir = Path::new("/mods/basic-combat");
        let p = artifact_path(dir, "release", "basic_combat");
        assert_eq!(
            p,
            PathBuf::from(
                "/mods/basic-combat/target/wasm32-unknown-unknown/release/basic_combat.wasm"
            )
        );
        if let Some(v) = original {
            std::env::set_var("CARGO_TARGET_DIR", v);
        }
    }

    #[test]
    #[serial_test::serial]
    fn artifact_path_honors_cargo_target_dir() {
        let original = std::env::var_os("CARGO_TARGET_DIR");
        std::env::set_var("CARGO_TARGET_DIR", "/shared/cache");
        let p = artifact_path(Path::new("/mods/basic-combat"), "debug", "basic_combat");
        assert_eq!(
            p,
            PathBuf::from("/shared/cache/wasm32-unknown-unknown/debug/basic_combat.wasm")
        );
        if let Some(v) = original {
            std::env::set_var("CARGO_TARGET_DIR", v);
        }
    }

    #[test]
    fn crate_name_from_cargo_toml_reads_package_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"basic-combat\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        assert_eq!(
            crate_name_from_cargo_toml(dir.path()).as_deref(),
            Some("basic_combat")
        );
    }

    #[test]
    fn crate_name_from_missing_cargo_toml_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(crate_name_from_cargo_toml(dir.path()), None);
    }

    #[test]
    fn resolve_module_id_from_fixture_finds_manifest_in_module_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let module_dir = dir.path().join("mod");
        std::fs::create_dir_all(module_dir.join("fixtures")).expect("mkdir");
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::to_vec(&json!({
                "module_id": "basic-combat",
                "name": "Basic Combat",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": ["character"],
                "compute_export": "compute",
                "init_export": "init",
            }))
            .expect("json"),
        )
        .expect("write manifest");
        let fixture = module_dir.join("fixtures").join("combat-input.json");
        std::fs::write(&fixture, "{}").expect("write fixture");

        let id = resolve_module_id_from_fixture(&fixture).expect("resolves");
        assert_eq!(id, "basic-combat");
    }

    #[test]
    fn resolve_module_id_from_fixture_fails_without_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("input.json");
        std::fs::write(&fixture, "{}").expect("write fixture");
        assert!(resolve_module_id_from_fixture(&fixture).is_err());
    }

    #[tokio::test]
    async fn cmd_run_posts_run_and_accepts_through_daemon() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/daemon/compute/run"))
            .and(body_partial_json(json!({
                "world_id": "wld_combat",
                "module_id": "basic-combat",
                "invocation_params": {"attacker_id": "kb_atk", "defender_id": "kb_def"},
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "run_id": "run_1",
                "module_id": "basic-combat",
                "module_version": "1.0.0",
                "status": "succeeded",
                "truncated": false,
                "proposals": {
                    "schema_version": 1,
                    "state_delta": [],
                    "timeline_events": [],
                    "new_key_blocks": [],
                    "battle_report": {"kind": "combat"},
                },
                "error": null,
            })))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/daemon/compute/runs/run_1/accept"))
            .and(body_partial_json(json!({})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "applied": {"state_delta": []},
                "run_id": "run_1",
                "status": "applied",
                "timeline_event_ids": ["evt_0"],
            })))
            .expect(1)
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let fixture = dir.path().join("combat-input.json");
        std::fs::write(
            &fixture,
            serde_json::to_vec(&json!({
                "schema_version": 1,
                "world_ref": {"world_id": "wld_combat", "branch_id": "root", "timeline_head_event_id": "evt_0"},
                "key_blocks": [],
                "narrative_state": {},
                "invocation": {"attacker_id": "kb_atk", "defender_id": "kb_def"},
            }))
            .expect("json"),
        )
        .expect("write fixture");

        let config = CliConfig {
            daemon_url: server.uri(),
            ..Default::default()
        };
        cmd_run(
            &config,
            "wld_combat",
            &fixture,
            Some("basic-combat"),
            true,
            RunOutput::Json,
        )
        .await
        .expect("run + accept succeed");
        server.verify().await;
    }
}
