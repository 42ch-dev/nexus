//! Build script for `nexus-daemon-runtime`.
//!
//! Ensures the `rust-embed` target directory `apps/web/dist` exists before the
//! `#[derive(RustEmbed)]` macro in `src/static_assets.rs` runs. The macro
//! **hard-fails at compile time** if the folder is absent, which breaks every
//! Rust CI job that does not first run `pnpm --filter web build` (fmt/clippy,
//! tests, sqlx offline-metadata) as well as local `cargo test` without a prior
//! web build.
//!
//! When `apps/web/dist` is missing, this script creates it together with a
//! minimal placeholder `index.html`. The placeholder is only a compile-time
//! fallback so the crate builds anywhere; the real Web UI bundle is produced by
//! `pnpm --filter web build` and is embedded whenever present (release
//! artifacts, the web-build CI job). The placeholder is a build artifact under
//! `apps/web/.gitignore` and is never committed.

use std::path::Path;

fn main() {
    // `CARGO_MANIFEST_DIR` = `.../crates/nexus-daemon-runtime`; dist is two up +
    // into `apps/web/dist`.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set");
    let dist = Path::new(&manifest_dir)
        .join("..")
        .join("..")
        .join("apps")
        .join("web")
        .join("dist");

    if !dist.exists() {
        // Create the directory + a placeholder index.html so rust-embed finds a
        // valid folder. `create_dir_all` is idempotent; the placeholder is only
        // written when the file does not yet exist (a real build leaves it
        // untouched).
        std::fs::create_dir_all(&dist).unwrap_or_else(|e| {
            panic!(
                "nexus-daemon-runtime build.rs: failed to create stub {}: {}",
                dist.display(),
                e
            )
        });
        let index = dist.join("index.html");
        if !index.exists() {
            std::fs::write(
                &index,
                "<!-- nexus-daemon-runtime build.rs placeholder so rust-embed compiles when the \
                 Web UI has not been built. Run `pnpm --filter web build` to produce the real \
                 bundle. -->\n",
            )
            .unwrap_or_else(|e| {
                panic!(
                    "nexus-daemon-runtime build.rs: failed to write stub {}: {}",
                    index.display(),
                    e
                )
            });
        }
    }

    // Re-run this script (and therefore re-trigger rust-embed) when the real
    // dist contents change. Telling cargo to watch the directory covers web
    // rebuilds landing in dist.
    println!("cargo:rerun-if-changed=../../apps/web/dist");
}
