use std::path::PathBuf;

fn main() {
    // The bundled sidecar binary must exist before Tauri's build script runs;
    // `bundle.externalBin` resolves it at compile time using the target-triple
    // suffix (e.g. `nexus42-aarch64-apple-darwin`). On a fresh clone the
    // `binaries/` directory only contains the README, so fail fast with a clear
    // remediation instead of Tauri's opaque "resource path doesn't exist" error.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let target = current_target_triple();
    let binary = PathBuf::from(&manifest_dir)
        .join("binaries")
        .join(format!("nexus42-{target}"));

    if !binary.exists() {
        panic!(
            "Missing sidecar binary: {}.\n\
             Run `pnpm -w run sidecar` from the repo root (or \
             `SIDECAR_TARGETS='{}' bash scripts/fetch-sidecar.sh`) to build the \
             nexus42 sidecar binary before compiling the desktop crate.",
            binary.display(),
            target,
        );
    }

    // Tauri v2 build script — generates the app context from tauri.conf.json.
    tauri_build::build();
}

/// Reconstruct the target triple Cargo is building for so the build script only
/// requires the sidecar binary that will actually be bundled. This keeps the
/// default local/CI flow single-arch (aarch64-apple-darwin in V1.66) while still
/// allowing `SIDECAR_TARGETS=...` local multi-arch builds.
fn current_target_triple() -> String {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH");
    // V1.66 is macOS-only; the vendor/os segment is always "apple-darwin".
    format!("{arch}-apple-darwin")
}
