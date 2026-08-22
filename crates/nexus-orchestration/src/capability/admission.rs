//! User capability admission gates (V1.172 P1 T3, DR-10; AR-38/AR-39/AR-43).
//!
//! [`admit`] runs the fail-closed gates that decide whether a parsed
//! [`UserCapabilityDescriptor`] + capability dir may be registered **before**
//! the scan emits it (T4 wires this into [`crate::capability::scan`]; P0
//! registered discoverable stubs with no admission).
//!
//! Gate order is locked by AR-43: **collision → module file → hash → clamp**.
//! Rejection is per-capability and never a boot failure — a rejected
//! candidate is absent from the catalog and the registry (the caller records
//! `ScanOutcome.skipped` with the named reason). Clamping (AR-38) is the only
//! gate that **never** rejects: over-max overrides are tightened DOWN to the
//! host maxima, never silently raised.

use crate::capability::user_capability::{
    CapabilityDescriptorError, SandboxOverrides, UserCapabilityDescriptor,
};
use std::collections::HashSet;
use std::path::Path;

/// A descriptor that passed every admission gate (AR-43).
///
/// The descriptor's `sandbox` field carries the **clamped** overrides (AR-38);
/// every other field is the author's value as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedCapability {
    /// The admitted descriptor with sandbox clamped to the host maxima.
    pub descriptor: UserCapabilityDescriptor,
}

/// Reasons a candidate capability is refused admission (AR-43).
///
/// `Display`-message-only, like [`CapabilityDescriptorError`]: callers log the
/// message with the capability name and record a skip (AR-35 all-skips-logged).
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    /// The descriptor violates the AR-34 contract (surfaced before the
    /// AR-43 gates; see the `+ structural errors` note in the T3 brief).
    #[error("descriptor validation failed: {0}")]
    Descriptor(CapabilityDescriptorError),
    /// AR-43 gate 1: the declared name equals a builtin — builtin wins.
    #[error("{0}")]
    NameCollision(String),
    /// AR-43 gate 2: `manifest.json` or `<module-id>.wasm` missing in the
    /// capability dir.
    #[error("{0}")]
    ModuleNotFound(String),
    /// AR-43 gate 3: `wasm_sha256` pairing fails — the wasm bytes do not
    /// match the manifest's declared hash, the manifest declares no hash
    /// (fail-closed), or the descriptor's `wasmSha256` differs from the
    /// manifest-verified digest (AR-39).
    #[error("{0}")]
    HashMismatch(String),
}

/// Admit a user capability per AR-43, in strict order:
///
/// 1. **Collision** — `descriptor.name` in `builtin_names` → reject
///    (builtin wins; AR-36/AR-43). Checked before any file I/O, so a
///    colliding name is rejected even when the dir is incomplete.
/// 2. **Module file** — `manifest.json` + `<module-id>.wasm` both present in
///    `dir` → else reject (`ModuleNotFound`).
/// 3. **Hash** — the module manifest's declared `wasm_sha256` must verify
///    against the `<module-id>.wasm` bytes **and** equal the descriptor's
///    `wasm.wasmSha256` (AR-39 — `nexus-module-manifest` is the single hash
///    path; no second implementation here). A manifest without a declared
///    hash is fail-closed (`HashMismatch`): the pairing cannot be verified.
/// 4. **Clamp** — sandbox overrides tightened to the `SandboxConfig::default()`
///    maxima via `min(override, host_default)` (AR-38). Clamping **never
///    rejects**; absent overrides stay absent (host defaults).
///
/// The returned [`AdmittedCapability`] carries the descriptor with its sandbox
/// clamped; rejection reasons are returned as [`AdmissionError`] (the caller
/// logs + records the skip — this function itself does not log).
///
/// # Errors
///
/// Returns [`AdmissionError`] for the first failing gate; `None` at each
/// check remains the identity-on-success case.
#[allow(clippy::implicit_hasher)]
pub fn admit(
    descriptor: &UserCapabilityDescriptor,
    dir: &Path,
    builtin_names: &HashSet<&str>,
) -> Result<AdmittedCapability, AdmissionError> {
    // Structural gate: an AR-34-invalid descriptor cannot be admitted (the
    // scan's `read_descriptor` already validates, but `admit` is the
    // standalone contract — defensive).
    descriptor.validate().map_err(AdmissionError::Descriptor)?;

    // AR-43 gate 1: collision (builtin wins).
    if builtin_names.contains(descriptor.name.as_str()) {
        return Err(AdmissionError::NameCollision(format!(
            "user capability '{}' collides with a builtin (builtin wins)",
            descriptor.name
        )));
    }

    // AR-43 gate 2: module file pairing presence.
    let manifest_path = dir.join("manifest.json");
    let wasm_path = dir.join(format!("{}.wasm", descriptor.wasm.module_id));
    if !manifest_path.is_file() {
        return Err(AdmissionError::ModuleNotFound(format!(
            "manifest.json not present in {}",
            dir.display()
        )));
    }
    if !wasm_path.is_file() {
        return Err(AdmissionError::ModuleNotFound(format!(
            "{}.wasm not present in {}",
            descriptor.wasm.module_id,
            dir.display()
        )));
    }

    // AR-43 gate 3: wasm_sha256 pairing (AR-39 — single hash path).
    // Read/parse failures here are pairing failures: the manifest's declared
    // hash is the only digest we can verify against.
    let manifest_json = std::fs::read(&manifest_path).map_err(|e| {
        AdmissionError::HashMismatch(format!("cannot read {}: {e}", manifest_path.display()))
    })?;
    let manifest: nexus_wasm_host::ModuleManifest = serde_json::from_slice(&manifest_json)
        .map_err(|e| {
            AdmissionError::HashMismatch(format!("cannot parse {}: {e}", manifest_path.display()))
        })?;
    let wasm_bytes = std::fs::read(&wasm_path).map_err(|e| {
        AdmissionError::HashMismatch(format!("cannot read {}: {e}", wasm_path.display()))
    })?;
    let manifest_hash = manifest.wasm_sha256.as_deref().ok_or_else(|| {
        AdmissionError::HashMismatch(format!(
            "{} does not declare wasm_sha256; pairing cannot be verified (fail-closed, AR-39)",
            manifest_path.display()
        ))
    })?;
    manifest.verify_wasm_sha256(&wasm_bytes).map_err(|e| {
        AdmissionError::HashMismatch(format!(
            "{e} (descriptor wasmSha256 {})",
            descriptor.wasm.wasm_sha256
        ))
    })?;
    if descriptor.wasm.wasm_sha256 != manifest_hash {
        return Err(AdmissionError::HashMismatch(format!(
            "descriptor wasmSha256 {} does not match manifest-verified hash {}",
            descriptor.wasm.wasm_sha256, manifest_hash
        )));
    }

    // AR-43 gate 4: clamp sandbox overrides to the host maxima — never
    // rejects (AR-38).
    let mut admitted_descriptor = descriptor.clone();
    if let Some(overrides) = &admitted_descriptor.sandbox {
        admitted_descriptor.sandbox = Some(clamp_sandbox(overrides));
    }
    Ok(AdmittedCapability {
        descriptor: admitted_descriptor,
    })
}

/// Tighten sandbox overrides to the host maxima via `min(override, default)`
/// — the same semantics as `WasmEngine::resolve_sandbox` (compute.rs L71-80).
///
/// The maxima are read from [`nexus_wasm_host::SandboxConfig::default()`]
/// (AR-38: read, never duplicate). Absent fields stay absent (host defaults).
#[must_use]
fn clamp_sandbox(overrides: &SandboxOverrides) -> SandboxOverrides {
    let defaults = nexus_wasm_host::SandboxConfig::default();
    let max_wall_time_ms = u64::try_from(defaults.wall_time.as_millis()).unwrap_or(u64::MAX);
    SandboxOverrides {
        fuel: overrides.fuel.map(|fuel| fuel.min(defaults.fuel)),
        memory_mib: overrides
            .memory_mib
            .map(|memory_mib| memory_mib.min(defaults.memory_mib())),
        wall_time_ms: overrides
            .wall_time_ms
            .map(|wall_time_ms| wall_time_ms.min(max_wall_time_ms)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;
    use std::fmt::Write as _;

    /// 64 lowercase hex chars (valid per AR-34/AR-39 format rules).
    const FAKE_SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn sha256_hex(bytes: &[u8]) -> String {
        let digest = sha2::Sha256::digest(bytes);
        let mut hex = String::with_capacity(64);
        for b in digest {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    }

    /// Write `<dir>/manifest.json` + `<dir>/basic-mod.wasm` with a **real**
    /// matching sha (the AR-39 pairing). Returns the sha.
    fn write_module_pair(dir: &Path) -> String {
        let wasm = b"fake module bytes";
        let sha = sha256_hex(wasm);
        std::fs::write(dir.join("basic-mod.wasm"), wasm).unwrap();
        let manifest = format!(
            r#"{{
                "module_id": "basic-mod",
                "name": "Basic Mod",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": "{sha}"
            }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        sha
    }

    /// A valid descriptor JSON for `name` (moduleId `basic-mod`); `sandbox`
    /// is embedded when given.
    fn descriptor_json(name: &str, sha: &str, sandbox: Option<&str>) -> String {
        let sandbox_json = sandbox.map_or_else(String::new, |s| format!("\"sandbox\": {s},"));
        format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                {sandbox_json}
                "wasm": {{ "moduleId": "basic-mod", "wasmSha256": "{sha}" }}
            }}"#
        )
    }

    fn parse(json: &str) -> UserCapabilityDescriptor {
        serde_json::from_str(json).expect("descriptor JSON parses")
    }

    /// Stage `<root>/<name>/` with the full AR-35 trio; returns the descriptor
    /// parsed from the written capability.json.
    fn stage(root: &Path, name: &str, sandbox: Option<&str>) -> UserCapabilityDescriptor {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let sha = write_module_pair(&dir);
        let json = descriptor_json(name, &sha, sandbox);
        std::fs::write(dir.join("capability.json"), &json).unwrap();
        parse(&json)
    }

    fn builtins() -> HashSet<&'static str> {
        HashSet::from(["sync.pull", "narrative.compute"])
    }

    #[test]
    fn admit_valid_pair_succeeds_with_declared_sandbox() {
        let tmp = tempfile::tempdir().unwrap();
        let descriptor = stage(
            tmp.path(),
            "demo.pull",
            Some(r#"{ "fuel": 5000000, "memoryMib": 32, "wallTimeMs": 10000 }"#),
        );
        let admitted = admit(&descriptor, &tmp.path().join("demo.pull"), &builtins())
            .expect("valid trio admitted");
        // Overrides inside the maxima pass through untouched (not raised).
        let sandbox = admitted
            .descriptor
            .sandbox
            .as_ref()
            .expect("sandbox present");
        assert_eq!(sandbox.fuel, Some(5_000_000));
        assert_eq!(sandbox.memory_mib, Some(32));
        assert_eq!(sandbox.wall_time_ms, Some(10_000));
        // Everything else stays as authored.
        assert_eq!(admitted.descriptor.name, "demo.pull");
        assert_eq!(
            admitted.descriptor.wasm.wasm_sha256,
            descriptor.wasm.wasm_sha256
        );
    }

    #[test]
    fn admit_collision_rejects_builtin_name() {
        let tmp = tempfile::tempdir().unwrap();
        // `sync.pull` is a builtin — rejected by gate 1 even with a complete
        // dir (builtin wins, AR-43 order: collision first).
        let descriptor = stage(tmp.path(), "sync.pull", None);
        let err = admit(&descriptor, &tmp.path().join("sync.pull"), &builtins())
            .expect_err("builtin name must be rejected");
        assert!(
            matches!(err, AdmissionError::NameCollision(_)),
            "expected NameCollision, got {err:?}"
        );
        assert!(err.to_string().contains("sync.pull"));
    }

    /// AR-43 order: collision is gate 1 — a colliding name is rejected with
    /// `NameCollision` even when the module files are absent (never falls
    /// through to `ModuleNotFound`).
    #[test]
    fn admit_collision_fires_before_module_file_check() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sync.pull");
        std::fs::create_dir_all(&dir).unwrap();
        // Empty dir: no manifest.json, no wasm. Gate 1 must still win.
        let descriptor = parse(&descriptor_json("sync.pull", FAKE_SHA, None));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("collision first");
        assert!(
            matches!(err, AdmissionError::NameCollision(_)),
            "gate 1 fires before gate 2, got {err:?}"
        );
    }

    #[test]
    fn admit_missing_manifest_rejects_module_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        // Only the wasm is present; manifest.json is missing.
        std::fs::write(dir.join("basic-mod.wasm"), b"fake module bytes").unwrap();
        let descriptor = descriptor_json("demo.pull", &sha256_hex(b"fake module bytes"), None);
        let descriptor = parse(&descriptor);
        let err = admit(&descriptor, &dir, &builtins()).expect_err("missing manifest");
        assert!(
            matches!(err, AdmissionError::ModuleNotFound(_)),
            "expected ModuleNotFound, got {err:?}"
        );
        assert!(err.to_string().contains("manifest.json"));
    }

    #[test]
    fn admit_missing_wasm_rejects_module_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        // Only manifest.json is present (no `<module-id>.wasm`).
        let sha = sha256_hex(b"fake module bytes");
        let manifest = format!(
            r#"{{ "name": "Basic Mod", "version": "1.0.0",
                 "nexus_abi_version": 1, "required_key_block_types": [],
                 "compute_export": "compute", "init_export": "",
                 "wasm_sha256": "{sha}" }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        let descriptor = parse(&descriptor_json("demo.pull", &sha, None));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("missing wasm");
        assert!(
            matches!(err, AdmissionError::ModuleNotFound(_)),
            "expected ModuleNotFound, got {err:?}"
        );
        assert!(err.to_string().contains("basic-mod.wasm"));
    }

    /// Descriptor `wasmSha256` != manifest-verified digest → `HashMismatch`
    /// (AR-39 fail-closed pairing).
    #[test]
    fn admit_hash_mismatch_descriptor_vs_manifest_rejects() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        let _ = write_module_pair(&dir); // real hash of the written bytes
        let wrong = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let descriptor = parse(&descriptor_json("demo.pull", wrong, None));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("hash mismatch");
        assert!(
            matches!(err, AdmissionError::HashMismatch(_)),
            "expected HashMismatch, got {err:?}"
        );
        assert!(
            err.to_string()
                .contains("does not match manifest-verified hash"),
            "named message: {err}"
        );
    }

    /// Wasm bytes not matching the manifest's declared hash → `HashMismatch`
    /// from `verify_wasm_sha256` (bytes/declaration mismatch).
    #[test]
    fn admit_hash_mismatch_wasm_vs_manifest_rejects() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        // Declare a hash that does NOT match the bytes actually written.
        let declared = sha256_hex(b"different bytes");
        let manifest = format!(
            r#"{{ "module_id": "basic-mod", "name": "Basic Mod", "version": "1.0.0",
                 "nexus_abi_version": 1, "required_key_block_types": [],
                 "compute_export": "compute", "init_export": "",
                 "wasm_sha256": "{declared}" }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("basic-mod.wasm"), b"fake module bytes").unwrap();
        let descriptor = parse(&descriptor_json("demo.pull", &declared, None));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("wasm/manifest mismatch");
        assert!(
            matches!(err, AdmissionError::HashMismatch(_)),
            "expected HashMismatch, got {err:?}"
        );
        assert!(
            err.to_string()
                .contains("wasm does not match manifest wasm_sha256"),
            "named reason: {err}"
        );
    }

    /// A manifest without a declared `wasm_sha256` is unverifiable → rejected
    /// fail-closed under gate 3 (AR-39: single hash path).
    #[test]
    fn admit_manifest_without_declared_hash_rejects_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("basic-mod.wasm"), b"fake module bytes").unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{ "module_id": "basic-mod", "name": "Basic Mod", "version": "1.0.0",
                 "nexus_abi_version": 1, "required_key_block_types": [],
                 "compute_export": "compute", "init_export": "" }"#,
        )
        .unwrap();
        let descriptor = parse(&descriptor_json(
            "demo.pull",
            &sha256_hex(b"fake module bytes"),
            None,
        ));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("no declared hash");
        assert!(
            matches!(err, AdmissionError::HashMismatch(_)),
            "unverifiable pairing rejects fail-closed, got {err:?}"
        );
        assert!(err.to_string().contains("does not declare wasm_sha256"));
    }

    /// Clamping never rejects: over-max overrides are tightened DOWN to the
    /// `SandboxConfig::default()` maxima (AR-38) — not rejected, not raised.
    #[test]
    fn admit_clamps_over_max_overrides_down_never_rejects() {
        let tmp = tempfile::tempdir().unwrap();
        let descriptor = stage(
            tmp.path(),
            "demo.pull",
            Some(r#"{ "fuel": 99000000, "memoryMib": 999, "wallTimeMs": 999999 }"#),
        );
        let admitted = admit(&descriptor, &tmp.path().join("demo.pull"), &builtins())
            .expect("clamping never rejects");
        let defaults = nexus_wasm_host::SandboxConfig::default();
        let sandbox = admitted
            .descriptor
            .sandbox
            .as_ref()
            .expect("sandbox preserved");
        assert_eq!(sandbox.fuel, Some(defaults.fuel));
        assert_eq!(sandbox.memory_mib, Some(defaults.memory_mib()));
        assert_eq!(
            sandbox.wall_time_ms,
            Some(u64::try_from(defaults.wall_time.as_millis()).unwrap_or(u64::MAX))
        );
    }

    /// Clamping preserves absent overrides: a descriptor without a sandbox
    /// stays `None` (host defaults) — admission never invents values.
    #[test]
    fn admit_absent_sandbox_stays_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let descriptor = stage(tmp.path(), "demo.pull", None);
        let admitted =
            admit(&descriptor, &tmp.path().join("demo.pull"), &builtins()).expect("admitted");
        assert!(
            admitted.descriptor.sandbox.is_none(),
            "absent sandbox stays absent"
        );
    }

    /// Structural gate: an AR-34-invalid descriptor surfaces as the
    /// `Descriptor` variant before any AR-43 gate runs.
    #[test]
    fn admit_invalid_descriptor_surfaces_descriptor_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("demo.pull");
        std::fs::create_dir_all(&dir).unwrap();
        let sha = write_module_pair(&dir);
        // "BadName" parses but fails AR-34 validation (uppercase).
        let descriptor = parse(&descriptor_json("BadName", &sha, None));
        let err = admit(&descriptor, &dir, &builtins()).expect_err("invalid name");
        assert!(
            matches!(err, AdmissionError::Descriptor(_)),
            "expected Descriptor error, got {err:?}"
        );
    }
}
