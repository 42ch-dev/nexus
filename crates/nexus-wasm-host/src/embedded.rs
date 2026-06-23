//! Embedded compute modules, compiled into the binary at build time.
//!
//! Mirrors the `embedded-presets/` pattern in `nexus-orchestration`: pre-built
//! `.wasm` artifacts live under `embedded-modules/<id>/<id>.wasm` and are
//! embedded via [`include_dir!`]. The committed `.wasm` blobs are rebuilt from
//! `modules/<id>/` per the procedure in `modules/README.md` (open design item
//! #6: pre-compile + commit). This keeps `cargo build -p nexus-wasm-host`
//! hermetic — no wasm toolchain required by host-crate consumers.

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
        // Populated by T7 once the .wasm is committed under embedded-modules/.
        assert!(
            embedded_module_bytes("basic-combat").is_some(),
            "basic-combat.wasm must be embedded; run the modules/ build procedure"
        );
        assert!(embedded_module_manifest("basic-combat").is_some());
    }
}
