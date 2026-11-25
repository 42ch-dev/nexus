//! Shared `wasm32-unknown-unknown` presence probe.
//!
//! `build.rs` is compiled on its own, so `#[cfg(test)]` code inside it never
//! runs under `cargo test`. This file is therefore the single copy used by
//! `nexus-wasm-host/build.rs`, `nexus-spoke-adapter/build.rs` and
//! `tests/wasm_target_probe.rs`, all via `#[path]`.
//!
//! Fail-closed: only a missing target directory means "not installed". Any
//! other stat failure, or an entry that is not a directory, panics —
//! `is_dir()` would turn those into a silent "target absent".

use std::path::PathBuf;
use std::process::Command;

const TARGET: &str = "wasm32-unknown-unknown";

/// True when the wasm target's std directory is present in `rustc`'s sysroot.
///
/// `rustc --print sysroot --target <t>` exits 0 even when the target is not
/// installed, so presence is decided from `sysroot/lib/rustlib/<target>`.
#[must_use]
pub fn wasm_target_installed(rustc: &str) -> bool {
    let target_dir = sysroot_of(rustc).join("lib/rustlib").join(TARGET);
    match std::fs::metadata(&target_dir) {
        Ok(meta) if meta.is_dir() => true,
        Ok(_) => panic!(
            "wasm target probe: cannot determine whether {TARGET} is installed: \
             `{}` exists but is not a directory. Fix the toolchain; refusing to \
             guess the target state.",
            target_dir.display()
        ),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => panic!(
            "wasm target probe: cannot determine whether {TARGET} is installed: \
             cannot stat `{}`: {} ({err}). Fix the toolchain or its permissions; \
             refusing to guess the target state.",
            target_dir.display(),
            err.kind()
        ),
    }
}

/// The sysroot of the compiler Cargo is actually driving (`RUSTC`, not `PATH`).
///
/// Build scripts use it to emit `cargo:rerun-if-changed` for
/// `lib/rustlib` so installing the target later (`rustup target add`) does not
/// leave a cached "target absent" decision in place.
///
/// An unanswerable query panics rather than reporting absence.
pub fn sysroot_of(rustc: &str) -> PathBuf {
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .unwrap_or_else(|err| {
            panic!(
                "wasm target probe: cannot determine whether {TARGET} is installed: \
                 `{rustc} --print sysroot` could not be run ({err}). Fix the rustc in \
                 RUSTC; refusing to guess the target state."
            )
        });

    assert!(
        output.status.success(),
        "wasm target probe: cannot determine whether {TARGET} is installed: \
         `{rustc} --print sysroot` failed with {} ({}). Fix the rustc in RUSTC; \
         refusing to guess the target state.",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = PathBuf::from(stdout.trim());
    assert!(
        path.is_dir(),
        "wasm target probe: cannot determine whether {TARGET} is installed: \
         `{rustc} --print sysroot` printed an unusable path (`{}`). Fix the rustc \
         in RUSTC; refusing to guess the target state.",
        path.display()
    );
    path
}
