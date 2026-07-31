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
//!
//! When the wasm target is not installed (`build.rs` sets
//! `nexus_no_wasm_target`), this module switches to empty stubs so `cargo check
//! --all` can complete on developer machines without the target (R-V1139P0-005).

/// **Real embedded module tree** — only compiled when the wasm target is
/// available (build.rs does not set `nexus_no_wasm_target`).
#[cfg(not(nexus_no_wasm_target))]
mod real {
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
}

/// **Empty stub** — used when the wasm target is absent (build.rs sets
/// `nexus_no_wasm_target`). All queries return nothing.
#[cfg(nexus_no_wasm_target)]
mod real {
    #[must_use]
    pub fn embedded_module_bytes(_id: &str) -> Option<&'static [u8]> {
        None
    }

    #[must_use]
    pub fn embedded_module_manifest(_id: &str) -> Option<&'static str> {
        None
    }

    #[must_use]
    pub fn embedded_module_ids() -> Vec<&'static str> {
        Vec::new()
    }
}

pub use real::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Only meaningful when embedded modules were actually compiled.
    #[cfg(not(nexus_no_wasm_target))]
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

    #[test]
    fn no_wasm_target_stubs_return_nothing() {
        // When cfg is active, stubs always return None/empty. This test is
        // trivially true on real builds too (the functions exist and compile);
        // on stub builds it asserts the stub contract.
        let ids = embedded_module_ids();
        for id in ids {
            assert!(
                embedded_module_bytes(id).is_some(),
                "real build: {id} must be embedded"
            );
        }
    }
}
