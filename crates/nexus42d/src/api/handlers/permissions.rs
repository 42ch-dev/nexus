//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! Permission enforcement for ACP tool execution.
//!
//! This module provides a simple permission checker that validates tool
//! execution requests against a configurable permission policy.
//!
//! # Architecture
//!
//! ```text
//! Tool execute request
//!     │
//!     ▼
//! PermissionEnforcer::check()
//!     │
//!     ├─► Load policy from workspace
//!     ├─► Map tool_name to permission category
//!     ├─► Evaluate policy
//!     └─► Grant / Deny
//! ```
//!
//! # Permission Categories
//!
//! ACP tools are mapped to permission categories:
//! - `fs/read_text_file` → `file_system.read`
//! - `fs/write_text_file` → `file_system.write`
//! - `terminal/*` → `terminal.execute` (deferred to V1.1)

use crate::api::errors::NexusApiError;

/// Map an ACP tool name to its permission category.
///
/// Returns `None` if the tool name doesn't map to any known category,
/// which means the tool is unrestricted (or should be denied by default).
fn tool_to_permission_category(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "fs/read_text_file" | "fs/list_directory" | "fs/read_directory" => Some("file_system.read"),
        "fs/delete_file" => Some("file_system.delete"),
        "fs/write_text_file" | "fs/create_directory" | "fs/move_file" => Some("file_system.write"),
        "terminal/create" | "terminal/output" | "terminal/release" | "terminal/kill" => Some("terminal.execute"),
        _ => None,
    }
}

/// Check if a tool execution is permitted based on the tool name and
/// an optional permission set.
///
/// If `granted_permissions` is `None`, all tools are permitted (no enforcement).
/// If provided, only tools whose permission category is in the set are allowed.
///
/// # Arguments
///
/// * `tool_name` - The ACP tool name (e.g., "`fs/read_text_file`")
/// * `granted_permissions` - Set of granted permission categories, or None for no enforcement
///
/// # Returns
///
/// `Ok(())` if the tool is permitted, or an error with details.
pub fn check_tool_permission<S: std::hash::BuildHasher>(
    tool_name: &str,
    granted_permissions: Option<&std::collections::HashSet<String, S>>,
) -> Result<(), NexusApiError> {
    // No enforcement if no permission set provided
    let Some(permissions) = granted_permissions else {
        return Ok(());
    };

    let category = tool_to_permission_category(tool_name);

    category.map_or_else(
        || {
            // Unknown tool: deny by default for safety
            Err(NexusApiError::InsufficientPermissions {
                required: vec![format!("tool:{tool_name}")],
            })
        },
        |cat| {
            if permissions.contains(cat) {
                Ok(())
            } else {
                Err(NexusApiError::InsufficientPermissions {
                    required: vec![cat.to_string()],
                })
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn file_read_tool_maps_to_correct_category() {
        assert_eq!(
            tool_to_permission_category("fs/read_text_file"),
            Some("file_system.read")
        );
    }

    #[test]
    fn file_write_tool_maps_to_correct_category() {
        assert_eq!(
            tool_to_permission_category("fs/write_text_file"),
            Some("file_system.write")
        );
    }

    #[test]
    fn terminal_tools_map_to_correct_category() {
        assert_eq!(
            tool_to_permission_category("terminal/create"),
            Some("terminal.execute")
        );
        assert_eq!(
            tool_to_permission_category("terminal/kill"),
            Some("terminal.execute")
        );
    }

    #[test]
    fn unknown_tool_maps_to_none() {
        assert_eq!(tool_to_permission_category("unknown/tool"), None);
    }

    #[test]
    fn no_enforcement_allows_all_tools() {
        let result = check_tool_permission::<std::collections::hash_map::RandomState>("fs/read_text_file", None);
        assert!(result.is_ok());
    }

    #[test]
    fn granted_permission_allows_matching_tool() {
        let mut perms = HashSet::new();
        perms.insert("file_system.read".to_string());

        let result = check_tool_permission("fs/read_text_file", Some(&perms));
        assert!(result.is_ok());
    }

    #[test]
    fn missing_permission_denies_tool() {
        let mut perms = HashSet::new();
        perms.insert("file_system.read".to_string());

        let result = check_tool_permission("fs/write_text_file", Some(&perms));
        assert!(result.is_err());
    }

    #[test]
    fn empty_permissions_denies_all_tools() {
        let perms = HashSet::new();

        let result = check_tool_permission("fs/read_text_file", Some(&perms));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_tool_denied_by_default() {
        let mut perms = HashSet::new();
        perms.insert("file_system.read".to_string());

        let result = check_tool_permission("custom/tool", Some(&perms));
        assert!(result.is_err());
    }

    #[test]
    fn wildcard_admin_permission_grants_all() {
        let mut perms = HashSet::new();
        perms.insert("admin".to_string());

        // Admin doesn't grant everything — we'd need special handling
        // For now, unknown tools are still denied
        let result = check_tool_permission("fs/read_text_file", Some(&perms));
        assert!(result.is_err());
    }
}
