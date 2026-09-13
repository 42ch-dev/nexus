//! Rust-side admission of [`ValidatedProviderRecipe`] for JS provider callbacks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nexus_contracts::ValidatedProviderRecipe;

use crate::config::validate_workspace_path;
use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;

fn canonicalize_executable(path: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    if !canonical.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if canonical.metadata().ok()?.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(canonical)
}

fn resolve_executable(command: &str) -> HostResult<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(HostError::policy_denied("provider launch command is empty"));
    }
    let path = Path::new(trimmed);
    let resolved = if path.is_absolute() {
        canonicalize_executable(path)
    } else if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(HostError::policy_denied(
            "relative executable paths with separators are rejected",
        ));
    } else {
        which::which(trimmed).ok().and_then(|found| canonicalize_executable(&found))
    };
    resolved
        .map(|p| p.display().to_string())
        .ok_or_else(|| HostError::policy_denied("executable is not an absolute canonical path"))
}

fn sanitize_env(env: &HashMap<String, String>) -> HashMap<String, String> {
    env.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Build the wire recipe from catalog-owned launch fields and workspace boundary.
pub fn build_validated_recipe(
    provider_id: &ProviderId,
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
    recipe_generation: String,
    workspace_root: &Path,
) -> HostResult<ValidatedProviderRecipe> {
    let canonical_cwd = validate_workspace_path(workspace_root)?;
    let executable = resolve_executable(command)?;
    Ok(ValidatedProviderRecipe {
        provider_id: provider_id.0.clone(),
        recipe_generation,
        executable,
        args: args.to_vec(),
        env: sanitize_env(env),
        cwd: canonical_cwd.display().to_string(),
        permissions_ref: None,
        config_ref: None,
        process_identity: None,
    })
}

/// Reject caller-supplied recipe keys before admission injection.
pub fn reject_caller_recipe_payload(
    payload: &serde_json::Map<String, serde_json::Value>,
) -> HostResult<()> {
    if payload.contains_key("recipe") {
        return Err(HostError::policy_denied(
            "caller-supplied recipe rejected; Rust admission is required",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_caller_supplied_recipe_key() {
        let mut payload = serde_json::Map::new();
        payload.insert(
            "recipe".into(),
            serde_json::json!({ "provider_id": "evil" }),
        );
        let err = reject_caller_recipe_payload(&payload).unwrap_err();
        assert!(err.to_string().contains("caller-supplied recipe rejected"));
    }
}
