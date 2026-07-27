//! Build script for `nexus-wasm-host`.
//!
//! Compiles embedded WASM modules from source (`../../modules/<id>/`) into the
//! `embedded-modules/<id>/` tree on demand. The `embedded-modules/` directory is
//! **generated and gitignored** — it is never committed. This script runs before
//! the crate's `include_dir!` macro expands, so freshly compiled artifacts are
//! available at compile time.
//!
//! # Requirements
//!
//! Building this crate (or anything that depends on it) requires the
//! `wasm32-unknown-unknown` Rust target. Install it once with:
//!
//! ```text
//! rustup target add wasm32-unknown-unknown
//! ```
//!
//! # Caching
//!
//! A module is recompiled only when its embedded `.wasm` is missing or older
//! than the newest file under its source directory (`src/`, `Cargo.toml`,
//! `manifest.json`). `cargo:rerun-if-changed` directives keep this aligned with
//! Cargo's own change detection, so a clean incremental `cargo build` does not
//! pay the wasm-compile cost when nothing changed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

/// Module ids that must be embedded. Each id must match a source crate under
/// `../../modules/<id>/`.
const MODULE_IDS: &[&str] = &["basic-combat"];

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    // `modules/` is a sibling of `crates/` at the workspace root.
    let modules_root = manifest_dir.join("../../modules");
    let embedded_root = manifest_dir.join("embedded-modules");

    create_dir_all_or_die(&embedded_root);

    for &id in MODULE_IDS {
        build_module(id, &modules_root, &embedded_root);
    }
}

/// Compile one module's `.wasm` from source and stage its manifest, unless the
/// embedded copy is already up to date.
fn build_module(id: &str, modules_root: &Path, embedded_root: &Path) {
    let src_dir = modules_root.join(id);
    let src_manifest = src_dir.join("manifest.json");
    let src_cargo = src_dir.join("Cargo.toml");
    let src_code = src_dir.join("src");

    // Tell Cargo to re-run this script when the module's sources change. The
    // directory path watches recursive file additions/removals and mtimes.
    println!("cargo:rerun-if-changed={}", src_manifest.display());
    println!("cargo:rerun-if-changed={}", src_cargo.display());
    println!("cargo:rerun-if-changed={}", src_code.display());

    if !src_dir.is_dir() {
        die(&format!(
            "module source `{id}` not found at {}",
            src_dir.display()
        ));
    }

    let dest_dir = embedded_root.join(id);
    let dest_wasm = dest_dir.join(format!("{id}.wasm"));
    let dest_manifest = dest_dir.join("manifest.json");

    create_dir_all_or_die(&dest_dir);

    // Always mirror the manifest (tiny) so the embedded copy tracks the source.
    copy_or_die(&src_manifest, &dest_manifest, "manifest.json");

    if !is_fresh(&dest_wasm, &src_cargo, &src_manifest, &src_code) {
        compile_module(id, &src_dir);
        // cdylib artifact names use underscores (crate name `basic-combat` →
        // `basic_combat.wasm`); the embedded id keeps the dash.
        let artifact = src_dir
            .join("target/wasm32-unknown-unknown/release")
            .join(format!("{}.wasm", id.replace('-', "_")));
        copy_or_die(&artifact, &dest_wasm, &format!("{id}.wasm"));
    }
}

/// Returns `true` only if `artifact` exists and is no older than every source
/// file. Any read failure is treated as "stale" so the compile surfaces the
/// real error rather than silently embedding a stale blob.
#[must_use]
fn is_fresh(artifact: &Path, src_cargo: &Path, src_manifest: &Path, src_dir: &Path) -> bool {
    let Ok(meta) = fs::metadata(artifact) else {
        return false;
    };
    let Ok(artifact_mtime) = meta.modified() else {
        return false;
    };
    if is_newer_than(src_cargo, artifact_mtime) || is_newer_than(src_manifest, artifact_mtime) {
        return false;
    }
    !dir_contains_newer(src_dir, artifact_mtime)
}

/// `true` if `path` was modified strictly after `threshold`.
#[must_use]
fn is_newer_than(path: &Path, threshold: SystemTime) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t > threshold)
}

/// Recursively checks whether any file under `dir` is newer than `threshold`.
#[must_use]
fn dir_contains_newer(dir: &Path, threshold: SystemTime) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return true; // unreadable src tree → force a rebuild attempt
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if dir_contains_newer(&path, threshold) {
                return true;
            }
        } else if let Ok(t) = meta.modified() {
            if t > threshold {
                return true;
            }
        }
    }
    false
}

/// Runs `cargo build --release --target wasm32-unknown-unknown` in the module's
/// source directory, exiting with a clear message on failure.
fn compile_module(id: &str, src_dir: &Path) {
    let output = Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir(src_dir)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => die(&format!(
            "failed to invoke `cargo` to build module `{id}`: {e} — is `cargo` on PATH?"
        )),
    };

    if output.status.success() {
        return;
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_missing_target_error(&stderr) {
        die(&format!(
            "wasm32-unknown-unknown target not installed — required to compile \
             embedded module `{id}`.\n\
             Fix: rustup target add wasm32-unknown-unknown"
        ));
    }
    die(&format!("failed to compile module `{id}`:\n{stderr}"));
}

/// Detects the rustc/cargo error emitted when the wasm sysroot is absent (the
/// overwhelmingly common cause of a module build failure here).
#[must_use]
fn is_missing_target_error(stderr: &str) -> bool {
    stderr.contains("can't find crate for `core`")
        || stderr.contains("can't find crate for `std`")
        || stderr.contains("does not have a standard library preinstalled")
        || stderr.contains("rust-std")
}

fn create_dir_all_or_die(path: &Path) {
    if let Err(e) = fs::create_dir_all(path) {
        die(&format!("failed to create {}: {e}", path.display()));
    }
}

fn copy_or_die(src: &Path, dest: &Path, label: &str) {
    if let Err(e) = fs::copy(src, dest) {
        die(&format!(
            "failed to copy {label}: {} → {}: {e}",
            src.display(),
            dest.display()
        ));
    }
}

fn die(msg: &str) -> ! {
    eprintln!("error: nexus-wasm-host: {msg}");
    std::process::exit(1);
}
