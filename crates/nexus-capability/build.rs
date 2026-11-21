//! Build script for `nexus-capability`.
//!
//! Probes whether the `wasm32-unknown-unknown` target is installed so the
//! embedded-module tests can be cfg-gated. Mirrors `nexus-wasm-host/build.rs`:
//! the cfg is per-crate and does not propagate to dependent crates, so this
//! crate needs its own probe.
//!
//! The probe is fail-closed: a compiler that cannot answer means the build
//! stops, never that the target is assumed absent.

use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "wasm32-unknown-unknown";

fn main() {
    // Declare the custom cfg flag early so rustc does not warn about
    // `unexpected_cfgs` in the library source.
    println!("cargo::rustc-check-cfg=cfg(nexus_no_wasm_target)");

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let sysroot = sysroot_of(&rustc);

    if wasm_target_installed(&sysroot.join("lib/rustlib").join(TARGET)) {
        return;
    }

    println!(
        "cargo:warning=nexus-capability: {TARGET} target not found; \
         skipping embedded-module tests. Install the target with: \
         rustup target add {TARGET}"
    );
    println!("cargo:rustc-cfg=nexus_no_wasm_target");
}

/// True when the wasm target's std directory is present.
///
/// Only `NotFound` means "not installed". Any other stat failure, or an entry
/// that is not a directory, is an unanswerable state and panics — `is_dir()`
/// would turn those into a silent "target absent".
fn wasm_target_installed(target_dir: &Path) -> bool {
    match std::fs::metadata(target_dir) {
        Ok(meta) if meta.is_dir() => true,
        Ok(_) => panic!(
            "nexus-capability build script: cannot determine whether {TARGET} is \
             installed: {target_dir:?} exists but is not a directory. Fix the \
             toolchain; refusing to guess the target state."
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => panic!(
            "nexus-capability build script: cannot determine whether {TARGET} is \
             installed: cannot stat {target_dir:?}: {} ({err}). Fix the toolchain or \
             its permissions; refusing to guess the target state.",
            err.kind()
        ),
    }
}

/// The sysroot of the compiler Cargo is actually driving (RUSTC, not PATH).
///
/// `rustc --print sysroot --target <t>` exits 0 even for a target that is not
/// installed, so presence is decided from the sysroot's `lib/rustlib/<target>`
/// directory. An unanswerable query panics rather than reporting absence.
fn sysroot_of(rustc: &str) -> PathBuf {
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "nexus-capability build script: cannot determine whether {TARGET} is \
                 installed: `{rustc} --print sysroot` could not be run ({err}). Fix the \
                 rustc in RUSTC; refusing to guess the target state."
            )
        });

    if !output.status.success() {
        panic!(
            "nexus-capability build script: cannot determine whether {TARGET} is \
             installed: `{rustc} --print sysroot` failed with {} ({}). Fix the rustc in \
             RUSTC; refusing to guess the target state.",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = PathBuf::from(stdout.trim());
    if !path.is_dir() {
        panic!(
            "nexus-capability build script: cannot determine whether {TARGET} is \
             installed: `{rustc} --print sysroot` printed an unusable path ({path:?}). \
             Fix the rustc in RUSTC; refusing to guess the target state."
        );
    }
    path
}
