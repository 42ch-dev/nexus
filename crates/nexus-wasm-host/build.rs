//! Build script for `nexus-wasm-host`.
//!
//! Implements open design item #6 resolution (**pre-compile + commit**): the
//! `.wasm` artifacts under `embedded-modules/` are built from `modules/` and
//! committed to the repo (see `modules/README.md`). This script is a **guard** —
//! it asserts the committed artifacts exist so `include_dir!` never fails and a
//! forgotten rebuild produces a clear, actionable error.
//!
//! It deliberately does **not** compile WASM. That would force every consumer
//! of `nexus-wasm-host` (CI, downstream crates) to install the wasm target and
//! own the `modules/` sources — the opposite of hermetic. The host crate builds
//! with stable Rust and no extra toolchains.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let embedded_root = manifest_dir.join("embedded-modules");

    // Guard: every embedded module dir must ship both `<id>.wasm` and
    // `manifest.json`. Scanning the tree avoids hard-coding the module list.
    let mut missing: Vec<String> = Vec::new();
    if embedded_root.is_dir() {
        for entry in std::fs::read_dir(&embedded_root)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            let Some(id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !path.is_dir() {
                continue;
            }
            let wasm = path.join(format!("{id}.wasm"));
            let manifest = path.join("manifest.json");
            if !wasm.exists() {
                missing.push(format!("{} (missing {})", id, wasm.display()));
            }
            if !manifest.exists() {
                missing.push(format!("{} (missing {})", id, manifest.display()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "nexus-wasm-host: embedded module artifacts are missing:\n  - {}\n\
         Rebuild from modules/ per modules/README.md and commit the .wasm + manifest.json.",
        missing.join("\n  - ")
    );

    // Re-run if the embedded tree changes.
    println!("cargo:rerun-if-changed=embedded-modules");
}
