//! `nexus42 capability` — capability authoring surface (V1.172 P2, AR-41).
//!
//! Subcommands: `validate` and `install` are **daemon-free** (the author
//! loop needs no runtime); `list` is a thin HTTP client over
//! `GET /v1/daemon/orchestration/capabilities`.
//!
//! The descriptor contract is the shared `UserCapabilityDescriptor` from
//! `nexus-orchestration` (AR-34 — nexus42 already depends on the crate, so
//! the CLI reuses the exact validator instead of hand-rolling a second
//! copy); the manifest + `wasm_sha256` pairing reuse `nexus-module-manifest`
//! (AR-39 — the single content-hash path). The CLI deliberately does NOT
//! know the builtin name list (AR-41): collision is daemon-side admission
//! only, checked at daemon restart.
//!
//! Exit-code contract (AR-41, mirrors the AR-9 table of `compute`):
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | success |
//! | 1    | install I/O/home failures (generic CLI failure) |
//! | 2    | descriptor/manifest validation failure (field list; `--json` machine-readable) |
//! | 3    | `wasm_sha256` pairing mismatch |
//! | 4    | daemon unreachable (`list`) |
//!
//! The group carries no `connect-host` feature dependency — the default
//! daemon graph stays libp2p-free.

use clap::Subcommand;
use nexus_module_manifest::ModuleManifest;
use nexus_orchestration::capability::user_capability::{
    CapabilityDescriptorError, UserCapabilityDescriptor,
};
use serde_json::Value;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};

/// AR-41 exit codes (mirror the AR-9 vocabulary used by `compute`).
mod exit {
    /// Install I/O / home-resolution failure (generic CLI failure code).
    pub const IO: i32 = 1;
    /// Descriptor / manifest validation failure.
    pub const VALIDATION: i32 = 2;
    /// `wasm_sha256` pairing mismatch.
    pub const PAIRING: i32 = 3;
    /// Daemon unreachable (`list`).
    pub const DAEMON: i32 = 4;
}

/// Capability subcommands (AR-41 table).
#[derive(Debug, Subcommand)]
pub enum CapabilityCommand {
    /// Validate a capability descriptor (and optionally its module pair).
    ///
    /// Daemon-free. Runs the shared descriptor validator (AR-34) plus —
    /// with `--module` — the module's `manifest.json` + `<module-id>.wasm`
    /// pairing via `nexus-module-manifest` (AR-39 single hash path).
    ///
    /// Exit codes (AR-41): 0 valid, 2 descriptor/manifest validation
    /// failure (field list, `--json` machine-readable), 3 `wasm_sha256`
    /// pairing mismatch. Collision with a builtin is checked at daemon
    /// restart — the CLI does not know the builtin name list.
    Validate {
        /// Path to the capability descriptor (`capability.json`).
        #[arg(long)]
        descriptor: PathBuf,
        /// Module dir containing `manifest.json` + `<module-id>.wasm` to
        /// also verify the AR-39 pairing.
        #[arg(long)]
        module: Option<PathBuf>,
        /// Emit machine-readable field-level errors (JSON).
        #[arg(long)]
        json: bool,
    },
    /// List registered capabilities (daemon-backed).
    ///
    /// Thin client over `GET /v1/daemon/orchestration/capabilities` (AR-41).
    /// Every row shows its `origin` (`builtin` or `user`) — no silent
    /// omission (AR-40). Exit 4 = daemon unreachable.
    List,
    /// Verify a descriptor + module trio and install it into
    /// `~/.nexus42/capabilities/<name>/` (AR-35 layout:
    /// `capability.json` + `manifest.json` + `<module-id>.wasm`).
    ///
    /// Daemon-free. Re-verifies descriptor + manifest + wasm pairing
    /// (AR-34/39) before copying. Collision with a built-in is checked at
    /// daemon restart (AR-41). No `run`, no `scaffold` (PL-7).
    ///
    /// Exit codes (AR-41): 2 = validation, 3 = pairing, 1 = I/O/home.
    Install {
        /// Path to the capability descriptor (`capability.json`).
        #[arg(long)]
        descriptor: PathBuf,
        /// Path to the compiled `<module-id>.wasm`.
        #[arg(long)]
        wasm: PathBuf,
        /// Path to the module's `manifest.json`.
        #[arg(long)]
        manifest: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Run the capability command group.
///
/// # Errors
///
/// Returns [`CliError::ComputeExit`] with the AR-41 exit code on failure
/// (the shared AR-9 exit-code plumbing maps 1/2/3/4 in `main.rs`).
pub async fn run(cmd: CapabilityCommand, config: &CliConfig, output_format: &str) -> Result<()> {
    match cmd {
        CapabilityCommand::Validate {
            descriptor,
            module,
            json,
        } => cmd_validate(&descriptor, module.as_deref(), json),
        CapabilityCommand::List => cmd_list(config, output_format).await,
        CapabilityCommand::Install {
            descriptor,
            wasm,
            manifest,
            json,
        } => cmd_install(&descriptor, &wasm, &manifest, json),
    }
}

// ─── capability validate ─────────────────────────────────────────────────

/// Validate a capability descriptor (and optionally its module pair).
///
/// Exit 2 on descriptor/manifest failure (field-level), exit 3 on pairing
/// mismatch.
fn cmd_validate(
    descriptor_path: &Path,
    module_dir: Option<&Path>,
    json_output: bool,
) -> Result<()> {
    let descriptor = read_descriptor(descriptor_path, json_output)?;

    if let Some(module_dir) = module_dir {
        // AR-35 layout: the module lives beside its manifest as
        // `<module-id>.wasm` + `manifest.json`.
        let manifest_path = module_dir.join("manifest.json");
        let wasm_path = module_dir.join(format!("{}.wasm", descriptor.wasm.module_id));
        verify_pairing(
            descriptor_path,
            &descriptor,
            &manifest_path,
            &wasm_path,
            json_output,
        )?;
    }

    if json_output {
        println!(
            "{}",
            serde_json::json!({ "valid": true, "descriptor": descriptor_path.display().to_string() })
        );
    } else {
        println!(
            "✓ Valid capability descriptor: {}",
            descriptor_path.display()
        );
        println!(
            "  Collision with a built-in is checked at daemon restart (the CLI does not \
             know the builtin list, AR-41)."
        );
    }
    Ok(())
}

// ─── capability install ───────────────────────────────────────────────────

/// Re-verify the trio and install it into `~/.nexus42/capabilities/<name>/`
/// (AR-35 layout: `capability.json` + `manifest.json` + `<module-id>.wasm`).
///
/// Install-path semantics (S-2, QC2): an existing `<name>/` dir is
/// **overwritten** — re-running install re-verifies the new trio first
/// (fail-closed) and then replaces the three files; install never skips an
/// existing dir. A hand-placed duplicate `<name>/` dir is resolved by the
/// daemon scan at boot — first-in-scan-order wins (AR-36).
///
/// Atomic-trio guarantee (S-2, QC3): verification runs before any write,
/// and the trio is copied into a sibling staging dir first; the final move
/// into place swaps the staged dir for the destination with two atomic
/// same-filesystem renames (current → backup, staging → dir), then drops
/// the backup. A failure in any copy phase leaves the destination
/// untouched — `<name>/` is always an old-complete or new-complete trio,
/// never a partial one.
///
/// Exit vocabulary (AR-41): 2 = descriptor/manifest validation failure
/// (incl. the F1 `module_id` identity mismatch), 3 = `wasm_sha256` pairing
/// mismatch, 1 = install I/O/home failure (generic CLI failure — the
/// codebase reserves 2/3/4 for validation, pairing, and daemon failures).
fn cmd_install(
    descriptor_path: &Path,
    wasm_path: &Path,
    manifest_path: &Path,
    json_output: bool,
) -> Result<()> {
    let descriptor = read_descriptor(descriptor_path, json_output)?;
    verify_pairing(
        descriptor_path,
        &descriptor,
        manifest_path,
        wasm_path,
        json_output,
    )?;

    let home = dirs::home_dir()
        .ok_or_else(|| capability_exit(exit::IO, "cannot resolve home directory"))?;
    let cap_root = nexus_home_layout::user_capabilities_dir(&home);
    let dir = cap_root.join(&descriptor.name);

    // AR-35 trio: the descriptor, the module manifest, and the wasm named
    // by the descriptor's `wasm.moduleId` (the source `--wasm` filename is
    // the author's staging name; the stored name is the module-id contract).
    //
    // Atomic-trio install (S-2, QC3): the trio is staged into a sibling
    // dir first, then swapped into place with two same-filesystem renames
    // (current dir → backup, staging → dir), and the backup dropped. Every
    // fallible I/O happens in the staging phase — on ANY error the
    // destination is untouched; the swap window is two atomic renames, so
    // `<name>/` is never a partial trio (old-complete or new-complete only).
    let staging = cap_root.join(format!(".{}.staging", descriptor.name));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| {
        capability_exit(
            exit::IO,
            format!("failed to create staging {}: {e}", staging.display()),
        )
    })?;
    let copy_result = (|| -> std::io::Result<()> {
        std::fs::copy(descriptor_path, staging.join("capability.json"))?;
        std::fs::copy(manifest_path, staging.join("manifest.json"))?;
        let wasm_name = format!("{}.wasm", descriptor.wasm.module_id);
        std::fs::copy(wasm_path, staging.join(&wasm_name))?;
        Ok(())
    })();
    if let Err(e) = copy_result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(capability_exit(
            exit::IO,
            format!("failed to stage capability trio: {e}"),
        ));
    }

    let backup = cap_root.join(format!(".{}.backup", descriptor.name));
    let _ = std::fs::remove_dir_all(&backup);
    if dir.exists() {
        std::fs::rename(&dir, &backup).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging);
            capability_exit(
                exit::IO,
                format!("failed to move current {} aside: {e}", dir.display()),
            )
        })?;
    }
    if let Err(e) = std::fs::rename(&staging, &dir) {
        // Restore the previous trio before reporting the failure — the
        // destination must never be left without a complete install.
        if backup.exists() {
            let _ = std::fs::rename(&backup, &dir);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(capability_exit(
            exit::IO,
            format!("failed to move trio into {}: {e}", dir.display()),
        ));
    }
    let _ = std::fs::remove_dir_all(&backup);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "installed": true,
                "name": descriptor.name,
                "path": dir.display().to_string(),
            }))
            .expect("json serialization cannot fail")
        );
    } else {
        println!(
            "installed capability `{}` → {} (the daemon picks the trio up on next boot)",
            descriptor.name,
            dir.display()
        );
    }
    Ok(())
}

// ─── capability list ──────────────────────────────────────────────────────

/// Thin HTTP client over `GET /v1/daemon/orchestration/capabilities`.
///
/// Exit 4 = daemon unreachable (AR-41). Every row carries its `origin` —
/// a `user` capability MUST show it, builtins show `builtin` (no silent
/// omission, AR-40).
async fn cmd_list(config: &CliConfig, output_format: &str) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let resp: crate::api::models::CapabilityListResponse = client
        .get("/v1/daemon/orchestration/capabilities")
        .await
        .map_err(|e| {
            let hint = if matches!(e, CliError::Network(_)) {
                "\n  Hint: `capability list` is daemon-backed — start the daemon with \
                 `nexus42 daemon start`"
            } else {
                ""
            };
            capability_exit(exit::DAEMON, format!("capability list failed: {e}{hint}"))
        })?;

    if output_format == "json" {
        println!("{}", render_list_json(&resp.items));
    } else {
        print!("{}", render_list_text(&resp.items));
    }
    Ok(())
}

/// Text rendering (pure so tests pin the origin column without stdout).
fn render_list_text(items: &[crate::api::models::CapabilityRow]) -> String {
    if items.is_empty() {
        return "No capabilities registered.\n".to_string();
    }
    let mut out = String::from("Capabilities:\n");
    for row in items {
        let _ = writeln!(out, "  {} [{}]", row.name, row.origin);
    }
    let _ = writeln!(out, "\n{} capability(s)", items.len());
    out
}

/// JSON rendering (one document on stdout — AR-9 wire discipline).
fn render_list_json(items: &[crate::api::models::CapabilityRow]) -> String {
    serde_json::to_string_pretty(items).expect("json serialization cannot fail")
}

// ─── shared helpers ───────────────────────────────────────────────────────

/// Build a [`CliError::ComputeExit`] with an AR-41 exit code.
fn capability_exit(code: i32, message: impl Into<String>) -> CliError {
    CliError::ComputeExit {
        code,
        message: message.into(),
    }
}

/// A field-level validation error (mirrors `compute`'s `FieldError` so the
/// `--json` verdict shape stays consistent across authoring groups).
struct FieldError {
    field: String,
    message: String,
}

/// Print the failure verdict (text or `--json`) and build the exit-code
/// error. `--json` output is machine-readable field-level errors:
/// `{"valid": false, "descriptor": "<path>", "errors": [{"field", "message"}]}`.
fn fail_validation(
    descriptor_path: &Path,
    errors: &[FieldError],
    json_output: bool,
    exit_code: i32,
) -> CliError {
    if json_output {
        println!("{}", validation_failure_json(descriptor_path, errors));
    } else {
        println!("✗ Invalid capability ({} error(s)):", errors.len());
        for e in errors {
            println!("  - {}: {}", e.field, e.message);
        }
    }
    capability_exit(
        exit_code,
        format!("capability validation failed: {} error(s)", errors.len()),
    )
}

/// The machine-readable `--json` failure verdict (pure formatter so tests
/// pin the exact shape without capturing stdout).
fn validation_failure_json(descriptor_path: &Path, errors: &[FieldError]) -> String {
    let errors_json: Vec<Value> = errors
        .iter()
        .map(|e| serde_json::json!({ "field": e.field, "message": e.message }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "valid": false,
        "descriptor": descriptor_path.display().to_string(),
        "errors": errors_json,
    }))
    .expect("json serialization cannot fail")
}

/// Derive the field name from a validation message: the backticked serde
/// field (e.g. `missing field module_id`) or the leading token.
fn field_error(message: String) -> FieldError {
    let field = message
        .find('`')
        .and_then(|start| {
            // Extract the bytes BETWEEN the two backticks — slicing up to
            // the closing backtick and re-splitting yields `missing field `,
            // not the field name.
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
        .unwrap_or_else(|| "descriptor".to_string());
    FieldError { field, message }
}

/// The AR-34 field a descriptor error names — used to build field-level
/// `--json` verdicts from the typed error vocabulary.
const fn descriptor_field(err: &CapabilityDescriptorError) -> &'static str {
    match err {
        CapabilityDescriptorError::MissingField(field) => field,
        CapabilityDescriptorError::InvalidName(_) => "name",
        CapabilityDescriptorError::InvalidSchema(_) => "schema",
        CapabilityDescriptorError::InvalidSandbox(_) => "sandbox",
        CapabilityDescriptorError::InvalidWasmRef(_) => "wasm",
    }
}

/// Read + parse + validate a descriptor (AR-34) via the shared
/// `nexus-orchestration` type — exit 2 on any failure, `--json` field-level.
fn read_descriptor(descriptor_path: &Path, json_output: bool) -> Result<UserCapabilityDescriptor> {
    // A missing/unreadable descriptor is a validation failure with the same
    // field-level shape as any other error — `--json` callers must get the
    // promised `{valid, descriptor, errors}` document, not a bare message.
    let descriptor_bytes = match std::fs::read(descriptor_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(fail_validation(
                descriptor_path,
                &[FieldError {
                    field: "descriptor".to_string(),
                    message: format!("cannot read {}: {e}", descriptor_path.display()),
                }],
                json_output,
                exit::VALIDATION,
            ));
        }
    };
    let descriptor: UserCapabilityDescriptor = match serde_json::from_slice(&descriptor_bytes) {
        Ok(d) => d,
        Err(e) => {
            return Err(fail_validation(
                descriptor_path,
                &[field_error(e.to_string())],
                json_output,
                exit::VALIDATION,
            ));
        }
    };
    if let Err(err) = descriptor.validate() {
        return Err(fail_validation(
            descriptor_path,
            &[FieldError {
                field: descriptor_field(&err).to_string(),
                message: err.to_string(),
            }],
            json_output,
            exit::VALIDATION,
        ));
    }
    Ok(descriptor)
}

/// Re-verify the AR-39 pairing: the module manifest validates, declares a
/// `wasm_sha256`, the wasm bytes hash to it, the descriptor's
/// `wasm.wasmSha256` equals the manifest-verified digest, and — the AR-41
/// identity cross-check (F1) — the manifest's `module_id` equals the
/// descriptor's `wasm.moduleId`. The descriptor `wasm.moduleId` names the
/// stored `<module-id>.wasm` (AR-35); a manifest declaring a different id
/// would install a silently dead trio (skipped at daemon boot, missing
/// `<manifest-module-id>.wasm`).
///
/// Exit 2 on descriptor/manifest validation failures (including the F1
/// identity mismatch); exit 3 on pairing failures (absent hash,
/// wasm/manifest mismatch, descriptor mismatch).
// Long: the pairing check is a single coherent gate sequence — each
// fail-closed arm is a field-level verdict; splitting would scatter the
// AR-39/AR-41 pairing contract.
#[allow(clippy::too_many_lines)]
fn verify_pairing(
    descriptor_path: &Path,
    descriptor: &UserCapabilityDescriptor,
    manifest_path: &Path,
    wasm_path: &Path,
    json_output: bool,
) -> Result<()> {
    // Manifest read/parse/validate failures are validation failures (exit 2).
    let manifest_bytes = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Err(fail_validation(
                descriptor_path,
                &[FieldError {
                    field: "manifest".to_string(),
                    message: format!("cannot read {}: {e}", manifest_path.display()),
                }],
                json_output,
                exit::VALIDATION,
            ));
        }
    };
    let manifest: ModuleManifest = match serde_json::from_slice(&manifest_bytes) {
        Ok(m) => m,
        Err(e) => {
            return Err(fail_validation(
                descriptor_path,
                &[field_error(e.to_string())],
                json_output,
                exit::VALIDATION,
            ));
        }
    };
    if let Err(errs) = manifest.validate() {
        let errors: Vec<FieldError> = errs.into_iter().map(field_error).collect();
        return Err(fail_validation(
            descriptor_path,
            &errors,
            json_output,
            exit::VALIDATION,
        ));
    }

    // F1 (QC W-1, all 3 seats): the module-id identity must match. The
    // descriptor's `wasm.moduleId` and the manifest's `module_id` are the
    // SAME contract — the `<module-id>.wasm` store name (AR-35). A trio
    // whose manifest declares a different id (hashes otherwise consistent)
    // would install with exit 0 and be skipped silently at daemon boot
    // (missing `<manifestModuleId>.wasm`). Fail closed at exit 2, before
    // any copy, mirroring the `compute install` gate (I2,
    // compute/mod.rs L518-525). Field is the descriptor's `wasm.moduleId`;
    // the message carries BOTH ids so `--json` callers get the mismatch
    // values (S-1, QC2).
    if descriptor.wasm.module_id != manifest.module_id {
        return Err(fail_validation(
            descriptor_path,
            &[FieldError {
                field: "wasm.moduleId".to_string(),
                message: format!(
                    "descriptor wasm.moduleId {} does not match manifest.module_id {}",
                    descriptor.wasm.module_id, manifest.module_id
                ),
            }],
            json_output,
            exit::VALIDATION,
        ));
    }

    // AR-39 pairing — a manifest without a declared hash is unverifiable:
    // fail closed (exit 3), mirroring admission gate 3.
    let Some(manifest_hash) = manifest.wasm_sha256.as_deref() else {
        return Err(fail_validation(
            descriptor_path,
            &[FieldError {
                field: "wasm_sha256".to_string(),
                message: format!(
                    "{} does not declare wasm_sha256; pairing cannot be verified (fail-closed, AR-39)",
                    manifest_path.display()
                ),
            }],
            json_output,
            exit::PAIRING,
        ));
    };

    let wasm_bytes = match std::fs::read(wasm_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            // The wasm is the pairing subject — an unreadable/missing wasm
            // means the pairing cannot be verified (AR-39 fail-closed).
            return Err(fail_validation(
                descriptor_path,
                &[FieldError {
                    field: "wasm".to_string(),
                    message: format!("cannot read {}: {e}", wasm_path.display()),
                }],
                json_output,
                exit::PAIRING,
            ));
        }
    };
    if let Err(pair_err) = manifest.verify_wasm_sha256(&wasm_bytes) {
        return Err(fail_validation(
            descriptor_path,
            &[FieldError {
                field: "wasm_sha256".to_string(),
                message: pair_err,
            }],
            json_output,
            exit::PAIRING,
        ));
    }
    if descriptor.wasm.wasm_sha256 != manifest_hash {
        return Err(fail_validation(
            descriptor_path,
            &[FieldError {
                field: "wasmSha256".to_string(),
                message: format!(
                    "descriptor wasmSha256 {} does not match manifest-verified hash {}",
                    descriptor.wasm.wasm_sha256, manifest_hash
                ),
            }],
            json_output,
            exit::PAIRING,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(64);
        for b in digest {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }

    /// Write a valid descriptor + module pair (manifest + `<module-id>.wasm`)
    /// under `root` and return the paths.
    struct Trio {
        descriptor: PathBuf,
        manifest: PathBuf,
        wasm: PathBuf,
        wasm_name: String,
    }

    fn write_trio(root: &Path, name: &str, module_id: &str, wasm_bytes: &[u8]) -> Trio {
        let sha = sha256_hex(wasm_bytes);
        let module_dir = root.join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        let manifest = module_dir.join("manifest.json");
        std::fs::write(
            &manifest,
            serde_json::json!({
                "module_id": module_id,
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": sha,
            })
            .to_string(),
        )
        .unwrap();
        let wasm_name = format!("{module_id}.wasm");
        let wasm = module_dir.join(&wasm_name);
        std::fs::write(&wasm, wasm_bytes).unwrap();
        let descriptor = root.join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": name,
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": module_id, "wasmSha256": sha },
            })
            .to_string(),
        )
        .unwrap();
        Trio {
            descriptor,
            manifest,
            wasm,
            wasm_name,
        }
    }

    fn write_descriptor_only(root: &Path, name: &str) -> PathBuf {
        let descriptor = root.join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": name,
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"unused") },
            })
            .to_string(),
        )
        .unwrap();
        descriptor
    }

    // ── group surface (AR-41: hidden, validate|list|install only) ─────────

    #[test]
    fn capability_group_is_hidden_without_run_or_scaffold() {
        let command = crate::cli::build_command();
        let cap = command
            .find_subcommand("capability")
            .expect("capability group registered");
        assert!(
            cap.is_hide_set(),
            "capability must be hidden (V1.35 lock posture, AR-41)"
        );
        let names: Vec<&str> = cap.get_subcommands().map(clap::Command::get_name).collect();
        for expected in ["validate", "list", "install"] {
            assert!(names.contains(&expected), "capability must have {expected}");
        }
        assert!(!names.contains(&"run"), "no run subcommand (PL-7)");
        assert!(
            !names.contains(&"scaffold"),
            "no scaffold subcommand (PL-7)"
        );
    }

    // ── validate ───────────────────────────────────────────────────────────

    #[test]
    fn validate_accepts_valid_descriptor_only() {
        let dir = tempfile::tempdir().unwrap();
        let descriptor = write_descriptor_only(dir.path(), "demo.pull");
        let result = cmd_validate(&descriptor, None, false);
        assert!(
            result.is_ok(),
            "descriptor-only validate must pass: {result:?}"
        );
    }

    #[test]
    fn validate_accepts_valid_descriptor_and_module_pair() {
        let dir = tempfile::tempdir().unwrap();
        let trio = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");
        let result = cmd_validate(
            &trio.descriptor,
            Some(dir.path().join("module").as_path()),
            false,
        );
        assert!(
            result.is_ok(),
            "validate with module pair must pass: {result:?}"
        );
    }

    #[test]
    fn validate_rejects_bad_descriptor_exit_two_json() {
        let dir = tempfile::tempdir().unwrap();
        // Uppercase violates the AR-34 name contract (`^[a-z0-9_]+$` per segment).
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "Bad.Name",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"x") },
            })
            .to_string(),
        )
        .unwrap();
        let err = cmd_validate(&descriptor, None, true).expect_err("bad descriptor must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "bad descriptor must exit 2, got {err}"
        );
    }

    #[test]
    fn validate_rejects_unknown_descriptor_field_exit_two() {
        let dir = tempfile::tempdir().unwrap();
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"x") },
                "sneaky": true,
            })
            .to_string(),
        )
        .unwrap();
        let err = cmd_validate(&descriptor, None, true).expect_err("unknown field must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "unknown field must exit 2, got {err}"
        );
    }

    #[test]
    fn validate_rejects_wasm_pairing_mismatch_exit_three() {
        let dir = tempfile::tempdir().unwrap();
        // Manifest declares the real wasm hash; the descriptor declares a
        // DIFFERENT hash → descriptor-vs-manifest mismatch (exit 3).
        let wasm_bytes = b"wasm module bytes";
        let real_sha = sha256_hex(wasm_bytes);
        let module_dir = dir.path().join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::json!({
                "module_id": "basic-mod",
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": real_sha,
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(module_dir.join("basic-mod.wasm"), wasm_bytes).unwrap();
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"other bytes") },
            })
            .to_string(),
        )
        .unwrap();

        let err = cmd_validate(&descriptor, Some(&module_dir), true)
            .expect_err("pairing mismatch must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 3, .. }),
            "pairing mismatch must exit 3, got {err}"
        );
    }

    #[test]
    fn validate_rejects_module_id_mismatch_exit_two() {
        // F1 (QC W-1): the descriptor's `wasm.moduleId` and the manifest's
        // `module_id` must agree — hashes alone do not establish the trio's
        // identity. A mismatch is a validation failure (exit 2), before any
        // copy, mirroring the `compute install` gate (I2).
        let dir = tempfile::tempdir().unwrap();
        let wasm_bytes = b"wasm module bytes";
        let sha = sha256_hex(wasm_bytes);
        let module_dir = dir.path().join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::json!({
                "module_id": "manifest-mod",
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": sha,
            })
            .to_string(),
        )
        .unwrap();
        // The staged wasm is named after the DESCRIPTOR's moduleId
        // (`descriptor-mod.wasm`, AR-35 store-name contract), but its
        // manifest declares `manifest-mod` — an inconsistent identity.
        std::fs::write(module_dir.join("descriptor-mod.wasm"), wasm_bytes).unwrap();
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "descriptor-mod", "wasmSha256": sha },
            })
            .to_string(),
        )
        .unwrap();

        let err = cmd_validate(&descriptor, Some(&module_dir), true)
            .expect_err("module-id mismatch must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "module-id mismatch must exit 2, got {err}"
        );
    }

    #[test]
    fn module_id_mismatch_error_json_carries_both_ids() {
        // S-1 (QC2): the `--json` field-level verdict for the F1 mismatch
        // carries BOTH ids in the message — consumers see the descriptor vs
        // manifest identity values, not just the field name.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capability.json");
        let out = validation_failure_json(
            &path,
            &[FieldError {
                field: "wasm.moduleId".to_string(),
                message: "descriptor wasm.moduleId descriptor-mod does not match \
                          manifest.module_id manifest-mod"
                    .to_string(),
            }],
        );
        let parsed: Value = serde_json::from_str(&out).expect("single JSON document");
        assert_eq!(parsed["errors"][0]["field"], "wasm.moduleId");
        let message = parsed["errors"][0]["message"].as_str().unwrap();
        assert!(
            message.contains("descriptor-mod"),
            "message carries the descriptor id: {message}"
        );
        assert!(
            message.contains("manifest-mod"),
            "message carries the manifest id: {message}"
        );
    }

    #[test]
    fn validation_failure_json_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capability.json");
        let out = validation_failure_json(
            &path,
            &[FieldError {
                field: "name".to_string(),
                message: "invalid name".to_string(),
            }],
        );
        let parsed: Value = serde_json::from_str(&out).expect("single JSON document");
        assert_eq!(parsed["valid"], false);
        assert_eq!(parsed["descriptor"], path.display().to_string());
        assert_eq!(parsed["errors"][0]["field"], "name");
        assert_eq!(parsed["errors"][0]["message"], "invalid name");
    }

    // ── install ────────────────────────────────────────────────────────────

    #[test]
    fn install_copies_trio_to_capabilities_dir() {
        let _home = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        let trio = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");

        let result = cmd_install(&trio.descriptor, &trio.wasm, &trio.manifest, false);
        assert!(result.is_ok(), "install must succeed: {result:?}");

        let home = std::env::var("HOME").unwrap();
        let cap_dir = nexus_home_layout::user_capabilities_dir(Path::new(&home)).join("demo.pull");
        assert!(
            cap_dir.join("capability.json").is_file(),
            "capability.json installed"
        );
        assert!(
            cap_dir.join("manifest.json").is_file(),
            "manifest.json installed"
        );
        assert!(
            cap_dir.join(&trio.wasm_name).is_file(),
            "<module-id>.wasm installed as {}",
            trio.wasm_name
        );
    }

    #[test]
    fn install_rejects_pairing_mismatch_exit_three_no_dir() {
        let _home = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        // Manifest declares a hash that does not match the wasm bytes.
        let wasm_bytes = b"wasm module bytes";
        let module_dir = dir.path().join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::json!({
                "module_id": "basic-mod",
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": sha256_hex(b"other bytes"),
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(module_dir.join("basic-mod.wasm"), wasm_bytes).unwrap();
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"other bytes") },
            })
            .to_string(),
        )
        .unwrap();

        let err = cmd_install(
            &descriptor,
            &module_dir.join("basic-mod.wasm"),
            &module_dir.join("manifest.json"),
            false,
        )
        .expect_err("pairing mismatch must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 3, .. }),
            "pairing mismatch must exit 3, got {err}"
        );
        let home = std::env::var("HOME").unwrap();
        let cap_dir = nexus_home_layout::user_capabilities_dir(Path::new(&home)).join("demo.pull");
        assert!(!cap_dir.exists(), "no install dir on pairing failure");
    }

    #[test]
    fn install_rejects_module_id_mismatch_exit_two_no_dir() {
        // F1: a trio whose manifest declares a different module_id than the
        // descriptor's `wasm.moduleId` must fail closed at exit 2 BEFORE any
        // copy — no install dir, no staging residue.
        let _home = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        let wasm_bytes = b"wasm module bytes";
        let sha = sha256_hex(wasm_bytes);
        let module_dir = dir.path().join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::json!({
                "module_id": "manifest-mod",
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": sha,
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(module_dir.join("descriptor-mod.wasm"), wasm_bytes).unwrap();
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "descriptor-mod", "wasmSha256": sha },
            })
            .to_string(),
        )
        .unwrap();

        let err = cmd_install(
            &descriptor,
            &module_dir.join("descriptor-mod.wasm"),
            &module_dir.join("manifest.json"),
            false,
        )
        .expect_err("module-id mismatch must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 2, .. }),
            "module-id mismatch must exit 2, got {err}"
        );
        let home = std::env::var("HOME").unwrap();
        let cap_dir = nexus_home_layout::user_capabilities_dir(Path::new(&home)).join("demo.pull");
        assert!(
            !cap_dir.exists(),
            "no install dir on module-id mismatch (verify-first, F1)"
        );
    }

    #[test]
    fn install_reinstall_overwrites_previous_trio() {
        // S-2 (QC2): re-running install OVERWRITES the existing `<name>/`
        // dir — install never skips; the old trio is fully replaced by the
        // re-verified new trio (and no staging/backup residue remains).
        let _h = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        let trio = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");

        cmd_install(&trio.descriptor, &trio.wasm, &trio.manifest, false)
            .expect("first install succeeds");

        // Second, identical install over the same dir must also succeed.
        cmd_install(&trio.descriptor, &trio.wasm, &trio.manifest, false)
            .expect("reinstall over existing dir succeeds");

        let home = std::env::var("HOME").unwrap();
        let cap_root = nexus_home_layout::user_capabilities_dir(Path::new(&home));
        let cap_dir = cap_root.join("demo.pull");
        for f in ["capability.json", "manifest.json", "basic-mod.wasm"] {
            assert!(cap_dir.join(f).is_file(), "{f} present after reinstall");
        }
        assert!(
            !cap_root.join(".demo.pull.staging").exists(),
            "staging dir cleaned up after success"
        );
        assert!(
            !cap_root.join(".demo.pull.backup").exists(),
            "backup dir cleaned up after success"
        );
    }

    #[test]
    fn install_failure_leaves_no_partial_trio_or_residue() {
        // S-2 (QC3): a staging-phase failure (unreadable wasm) must leave the
        // destination untouched and no staging residue — no partial trio.
        let _h = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        let wasm_bytes = b"wasm module bytes";
        let sha = sha256_hex(wasm_bytes);
        let module_dir = dir.path().join("module");
        std::fs::create_dir_all(&module_dir).unwrap();
        std::fs::write(
            module_dir.join("manifest.json"),
            serde_json::json!({
                "module_id": "basic-mod",
                "name": "Test Module",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": sha,
            })
            .to_string(),
        )
        .unwrap();
        // The wasm file is missing — verify_pairing fails BEFORE the trio
        // is even staged (exit 3); no dir, no staging, no backup.
        let descriptor = dir.path().join("capability.json");
        std::fs::write(
            &descriptor,
            serde_json::json!({
                "name": "demo.pull",
                "inputSchema": "{\"type\":\"object\"}",
                "outputSchema": "{\"type\":\"object\"}",
                "wasm": { "moduleId": "basic-mod", "wasmSha256": sha },
            })
            .to_string(),
        )
        .unwrap();

        let err = cmd_install(
            &descriptor,
            &module_dir.join("basic-mod.wasm"),
            &module_dir.join("manifest.json"),
            false,
        )
        .expect_err("missing wasm must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 3, .. }),
            "missing wasm is a pairing failure (exit 3), got {err}"
        );
        let home = std::env::var("HOME").unwrap();
        let cap_root = nexus_home_layout::user_capabilities_dir(Path::new(&home));
        assert!(!cap_root.join("demo.pull").exists(), "no install dir");
        assert!(
            !cap_root.join(".demo.pull.staging").exists(),
            "no staging residue on failure"
        );
        assert!(
            !cap_root.join(".demo.pull.backup").exists(),
            "no backup residue on failure"
        );
    }

    #[test]
    fn install_io_failure_exits_one() {
        let _home = crate::testutil::isolated_home();
        let dir = tempfile::tempdir().unwrap();
        let trio = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");
        // Block the destination: `~/.nexus42` is a FILE, so create_dir_all fails.
        let home = std::env::var("HOME").unwrap();
        std::fs::write(Path::new(&home).join(".nexus42"), b"file").unwrap();

        let err = cmd_install(&trio.descriptor, &trio.wasm, &trio.manifest, false)
            .expect_err("I/O failure must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 1, .. }),
            "install I/O failure must exit 1, got {err}"
        );
    }

    // ── list ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_daemon_unreachable_exits_four() {
        let config = CliConfig {
            daemon_url: "http://127.0.0.1:1".to_string(),
            ..Default::default()
        };
        let err = cmd_list(&config, "text")
            .await
            .expect_err("unreachable daemon must fail");
        assert!(
            matches!(err, CliError::ComputeExit { code: 4, .. }),
            "daemon unreachable must exit 4, got {err}"
        );
    }

    #[tokio::test]
    async fn list_renders_user_origin_from_wire() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/daemon/orchestration/capabilities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [
                    {
                        "name": "demo.pull",
                        "inputSchema": "{\"type\":\"object\"}",
                        "outputSchema": "{\"type\":\"object\"}",
                        "origin": "user"
                    },
                    {
                        "name": "narrative.compute",
                        "inputSchema": "{\"type\":\"object\"}",
                        "outputSchema": "{\"type\":\"object\"}",
                        "origin": "builtin"
                    }
                ],
                "pagination": { "limit": 100, "has_more": false }
            })))
            .expect(1)
            .mount(&server)
            .await;

        let config = CliConfig {
            daemon_url: server.uri(),
            ..Default::default()
        };
        // Render the text path via the parsed model (pure row rendering is
        // covered by the shape assertions below).
        let client = DaemonClient::from_config(&config);
        let resp: crate::api::models::CapabilityListResponse = client
            .get("/v1/daemon/orchestration/capabilities")
            .await
            .expect("wire mock returns the list");
        assert_eq!(resp.items.len(), 2);
        let user = resp
            .items
            .iter()
            .find(|r| r.name == "demo.pull")
            .expect("user row");
        assert_eq!(user.origin, "user", "user capability carries origin=user");
        let builtin = resp
            .items
            .iter()
            .find(|r| r.name == "narrative.compute")
            .expect("builtin row");
        assert_eq!(builtin.origin, "builtin", "builtin carries origin=builtin");
        assert_eq!(
            builtin.input_schema, "{\"type\":\"object\"}",
            "camelCase inputSchema decoded"
        );
    }

    #[test]
    fn capability_row_origin_defaults_to_builtin() {
        // Pre-AR-40 daemons omit `origin`; the model must tolerate that
        // (schema default "builtin", AR-40 back-compat).
        let json = json!({
            "name": "sync.pull",
            "inputSchema": "{}",
            "outputSchema": "{}",
        });
        let row: crate::api::models::CapabilityRow =
            serde_json::from_value(json).expect("parses without origin");
        assert_eq!(row.origin, "builtin");
    }

    #[test]
    fn render_text_shows_origin_for_user_and_builtin() {
        // AR-40/PL-8: a user capability MUST show its origin; builtins show
        // `builtin` — no silent omission.
        let items = vec![
            crate::api::models::CapabilityRow {
                name: "demo.pull".to_string(),
                input_schema: "{}".to_string(),
                output_schema: "{}".to_string(),
                origin: "user".to_string(),
            },
            crate::api::models::CapabilityRow {
                name: "sync.pull".to_string(),
                input_schema: "{}".to_string(),
                output_schema: "{}".to_string(),
                origin: "builtin".to_string(),
            },
        ];
        let text = render_list_text(&items);
        assert!(
            text.contains("demo.pull [user]"),
            "user origin shown: {text}"
        );
        assert!(
            text.contains("sync.pull [builtin]"),
            "builtin origin shown: {text}"
        );
        assert!(text.contains("2 capability(s)"));
    }

    #[test]
    fn render_text_empty_list() {
        assert_eq!(render_list_text(&[]), "No capabilities registered.\n");
    }
}
