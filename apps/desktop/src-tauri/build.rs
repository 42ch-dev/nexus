use std::path::PathBuf;

fn main() {
    // The bundled sidecar binaries must exist before Tauri's build script runs;
    // `bundle.externalBin` resolves them at compile time. On a fresh clone the
    // `binaries/` directory only contains the README, so fail fast with a clear
    // remediation instead of Tauri's opaque "resource path doesn't exist" error.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let targets = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
    let missing: Vec<PathBuf> = targets
        .iter()
        .map(|t| {
            PathBuf::from(&manifest_dir)
                .join("binaries")
                .join(format!("nexus42-{t}"))
        })
        .filter(|p| !p.exists())
        .collect();

    if !missing.is_empty() {
        let paths: Vec<String> = missing.iter().map(|p| p.display().to_string()).collect();
        panic!(
            "Missing sidecar binary(s): {}.\n\
             Run `pnpm -w run sidecar` from the repo root to build the nexus42 \
             sidecar binaries before compiling the desktop crate.",
            paths.join(", ")
        );
    }

    // Tauri v2 build script — generates the app context from tauri.conf.json.
    tauri_build::build();
}
