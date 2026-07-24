//! Compute module registry.
//!
//! Exposes the embedded module loader as a read-only registry for daemon API
//! discovery endpoints. No compute invocation or state mutation happens here.

use nexus_contracts::generated::daemon_api::compute::{
    module_detail::ModuleDetail,
    module_summary::{ModuleSummary, ModuleSummaryStatus},
};
use tracing::warn;

use crate::embedded::{embedded_module_ids, embedded_module_manifest};
use crate::manifest::ModuleManifest;

/// List all embedded compute modules as summaries.
///
/// Modules whose manifest cannot be parsed are surfaced with `status: "broken"`
/// rather than silently dropped, so the daemon API and UI can distinguish
/// corrupt embedded modules from modules that are not installed.
#[must_use]
pub fn list_modules() -> Vec<ModuleSummary> {
    embedded_module_ids()
        .into_iter()
        .filter_map(|id| {
            let manifest_json = embedded_module_manifest(id)?;
            match serde_json::from_str::<ModuleManifest>(manifest_json) {
                Ok(manifest) => Some(manifest_to_summary(&manifest)),
                Err(err) => {
                    warn!(
                        %id,
                        error = %err,
                        "embedded compute module manifest failed to parse; reporting as broken"
                    );
                    Some(broken_summary(id))
                }
            }
        })
        .collect()
}

/// Get the full manifest detail for a single embedded module by id.
///
/// Returns `Ok(None)` when the module is not installed, and `Err(...)` when
/// the module is present but its manifest cannot be parsed. This lets callers
/// distinguish a missing module (404) from a broken module (500) instead of
/// conflating the two as a 404.
///
/// # Errors
///
/// Returns a [`serde_json::Error`] if the embedded manifest file exists but
/// cannot be deserialized into [`ModuleManifest`].
pub fn get_module(id: &str) -> Result<Option<ModuleDetail>, serde_json::Error> {
    let Some(manifest_json) = embedded_module_manifest(id) else {
        return Ok(None);
    };
    let manifest: ModuleManifest = serde_json::from_str(manifest_json)?;
    Ok(Some(ModuleDetail::from(&manifest)))
}

fn manifest_to_summary(manifest: &ModuleManifest) -> ModuleSummary {
    // Keep field mapping aligned with `From<&ModuleManifest> for ModuleDetail` in manifest.rs.
    ModuleSummary {
        module_id: manifest.module_id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        required_key_block_types: manifest.required_key_block_types.clone(),
        battle_report_kind: manifest.battle_report_kind.clone(),
        status: ModuleSummaryStatus::Ok,
    }
}

fn broken_summary(id: &str) -> ModuleSummary {
    ModuleSummary {
        module_id: id.to_string(),
        name: id.to_string(),
        version: "unknown".to_string(),
        description: None,
        required_key_block_types: vec![],
        battle_report_kind: None,
        status: ModuleSummaryStatus::Broken,
    }
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
    fn list_modules_marks_valid_modules_ok() {
        let modules = list_modules();
        let basic_combat = modules
            .iter()
            .find(|m| m.module_id == "basic-combat")
            .expect("basic-combat should be present");
        assert_eq!(basic_combat.status, "ok");
    }

    #[test]
    fn broken_summary_uses_id_as_name_and_unknown_version() {
        let summary = broken_summary("broken-module");
        assert_eq!(summary.module_id, "broken-module");
        assert_eq!(summary.name, "broken-module");
        assert_eq!(summary.version, "unknown");
        assert!(summary.required_key_block_types.is_empty());
        assert_eq!(summary.status, "broken");
    }

    #[test]
    fn manifest_to_summary_reports_ok() {
        let manifest = ModuleManifest {
            module_id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            nexus_abi_version: 1,
            required_key_block_types: vec!["character".to_string()],
            compute_export: "compute".to_string(),
            init_export: "init".to_string(),
            description: None,
            author: None,
            host_functions: vec![],
            schemas: None,
            battle_report_kind: None,
            max_fuel: None,
            max_memory_mib: None,
            max_wall_time_ms: None,
        };
        let summary = manifest_to_summary(&manifest);
        assert_eq!(summary.status, "ok");
    }

    #[test]
    fn get_module_returns_basic_combat_detail() {
        let detail = get_module("basic-combat")
            .expect("basic-combat manifest should parse")
            .expect("basic-combat should be present");
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
    fn get_module_returns_ok_none_for_unknown() {
        assert!(get_module("no-such-module").unwrap().is_none());
    }

    #[test]
    fn list_modules_and_get_module_agree_on_basic_combat() {
        let summary = list_modules()
            .into_iter()
            .find(|m| m.module_id == "basic-combat")
            .expect("basic-combat should be in list_modules()");
        let detail = get_module("basic-combat")
            .expect("basic-combat manifest should parse")
            .expect("basic-combat should be in get_module()");

        assert_eq!(summary.module_id, detail.module_id);
        assert_eq!(summary.name, detail.name);
        assert_eq!(summary.version, detail.version);
        assert_eq!(summary.description, detail.description);
        assert_eq!(
            summary.required_key_block_types,
            detail.required_key_block_types
        );
        assert_eq!(summary.battle_report_kind, detail.battle_report_kind);
    }

    #[test]
    fn get_module_errs_for_broken_manifest() {
        let result = get_module_detail_from_manifest("not valid json");
        assert!(
            result.is_err(),
            "broken manifest JSON should produce a parse error, got {result:?}"
        );
    }

    /// Test helper that parses a manifest JSON string as a detail lookup would.
    fn get_module_detail_from_manifest(
        manifest_json: &str,
    ) -> Result<Option<ModuleDetail>, serde_json::Error> {
        let manifest: ModuleManifest = serde_json::from_str(manifest_json)?;
        Ok(Some(ModuleDetail::from(&manifest)))
    }
}
