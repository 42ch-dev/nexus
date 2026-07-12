//! Compute module registry.
//!
//! Exposes the embedded module loader as a read-only registry for daemon API
//! discovery endpoints. No compute invocation or state mutation happens here.

use nexus_contracts::generated::daemon_api::compute::{
    module_detail::ModuleDetail, module_summary::ModuleSummary,
};

use crate::embedded::{embedded_module_ids, embedded_module_manifest};
use crate::manifest::ModuleManifest;

/// List all embedded compute modules as summaries.
#[must_use]
pub fn list_modules() -> Vec<ModuleSummary> {
    embedded_module_ids()
        .into_iter()
        .filter_map(|id| {
            let manifest_json = embedded_module_manifest(id)?;
            let manifest: ModuleManifest = serde_json::from_str(manifest_json).ok()?;
            Some(manifest_to_summary(&manifest))
        })
        .collect()
}

/// Get the full manifest detail for a single embedded module by id.
#[must_use]
pub fn get_module(id: &str) -> Option<ModuleDetail> {
    let manifest_json = embedded_module_manifest(id)?;
    let manifest: ModuleManifest = serde_json::from_str(manifest_json).ok()?;
    manifest_to_detail(&manifest).ok()
}

fn manifest_to_summary(manifest: &ModuleManifest) -> ModuleSummary {
    ModuleSummary {
        module_id: manifest.module_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        required_key_block_types: manifest.required_key_block_types.clone(),
        battle_report_kind: manifest.battle_report_kind.clone(),
    }
}

fn manifest_to_detail(manifest: &ModuleManifest) -> Result<ModuleDetail, serde_json::Error> {
    // The generated ModuleDetail mirrors the manifest.json shape, so a
    // JSON round-trip is the smallest durable bridge between the hand-written
    // runtime struct and the generated wire contract.
    let value = serde_json::to_value(manifest)?;
    serde_json::from_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_modules_includes_basic_combat() {
        let modules = list_modules();
        assert!(
            modules.iter().any(|m| m.module_id == "basic-combat"),
            "basic-combat should be in the registry list: {modules:?}"
        );
    }

    #[test]
    fn get_module_returns_basic_combat_detail() {
        let detail = get_module("basic-combat").expect("basic-combat should be present");
        assert_eq!(detail.module_id, "basic-combat");
        assert_eq!(detail.name, "Basic Combat");
        assert_eq!(detail.nexus_abi_version, 1);
        assert!(
            detail
                .required_key_block_types
                .contains(&"character".to_string()),
            "basic-combat should require character key blocks: {detail:?}"
        );
    }

    #[test]
    fn get_module_returns_none_for_unknown() {
        assert!(get_module("no-such-module").is_none());
    }
}
