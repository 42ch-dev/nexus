//! Embedded compute modules, compiled into the binary at build time.
//!
//! Mirrors the `embedded-presets/` pattern in `nexus-orchestration`: the
//! `.wasm` artifacts under `embedded-modules/<id>/<id>.wasm` are embedded via
//! [`include_dir!`]. The `embedded-modules/` tree is **generated and
//! gitignored** — those `.wasm` blobs are **compiled by `build.rs`** from the
//! source crates under `modules/<id>/` (see `modules/README.md` and
//! `build.rs`). This keeps `cargo build -p nexus-wasm-host` reproducible while
//! avoiding committed binary artifacts; the `wasm32-unknown-unknown` target is
//! the only extra requirement (installed automatically in CI).

use include_dir::{include_dir, Dir};

/// The compiled-in module tree.
static EMBEDDED_MODULES: Dir = include_dir!("$CARGO_MANIFEST_DIR/embedded-modules");

/// Fetch a compiled-in module's `.wasm` bytes by id (e.g. `"basic-combat"`).
#[must_use]
pub fn embedded_module_bytes(id: &str) -> Option<&'static [u8]> {
    EMBEDDED_MODULES
        .get_file(format!("{id}/{id}.wasm"))
        .map(include_dir::File::contents)
}

/// Fetch a compiled-in module's `manifest.json` text by id.
#[must_use]
pub fn embedded_module_manifest(id: &str) -> Option<&'static str> {
    EMBEDDED_MODULES
        .get_file(format!("{id}/manifest.json"))
        .and_then(|f| f.contents_utf8())
}

/// Enumerate the ids of all compiled-in modules.
#[must_use]
pub fn embedded_module_ids() -> Vec<&'static str> {
    EMBEDDED_MODULES
        .dirs()
        .filter_map(|d| {
            let name = d.path().file_name()?.to_str()?;
            // A module dir is one that ships a `<name>.wasm`.
            d.get_file(format!("{name}/{name}.wasm"))
                .is_some()
                .then_some(name)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_combat_is_embedded() {
        // Populated by build.rs, which compiles modules/basic-combat/ into
        // embedded-modules/basic-combat/basic-combat.wasm at build time.
        assert!(
            embedded_module_bytes("basic-combat").is_some(),
            "basic-combat.wasm must be embedded; build.rs compiles it from modules/basic-combat/"
        );
        assert!(embedded_module_manifest("basic-combat").is_some());
    }
}
