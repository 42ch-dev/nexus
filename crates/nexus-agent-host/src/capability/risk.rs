//! Tool risk classification.
//!
//! Wave 1: static declaration only with extension point for future
//! auto-classification (R-006).
//! V1.19: added `AutoToolRiskClassifier` with compiled regex patterns.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Risk level for a tool operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    /// Read-only operation (safe to auto-approve).
    Read,
    /// Write operation (may modify state).
    Write,
    /// Destructive operation (irreversible changes).
    Destructive,
}

/// Classifier for tool risk levels.
///
/// Wave 1 supports static declaration only. Future waves will add
/// name/pattern-based auto-classification (R-006).
pub trait ToolRiskClassifier: Send + Sync {
    /// Classify the risk level of a tool by name.
    ///
    /// Returns `None` if the tool is unknown (treated as `Write` by default).
    fn classify(&self, tool_name: &str) -> Option<ToolRisk>;

    /// Classify with fallback: unknown tools default to `Write`.
    fn classify_or_default(&self, tool_name: &str) -> ToolRisk {
        self.classify(tool_name).unwrap_or(ToolRisk::Write)
    }
}

/// Static tool risk classifier based on a predefined map.
#[derive(Debug, Clone, Default)]
pub struct StaticToolRiskClassifier {
    /// Known tool risk mappings.
    tools: std::collections::HashMap<String, ToolRisk>,
}

impl StaticToolRiskClassifier {
    /// Create a new empty classifier.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool with a specific risk level.
    pub fn register(&mut self, tool_name: impl Into<String>, risk: ToolRisk) {
        self.tools.insert(tool_name.into(), risk);
    }
}

impl ToolRiskClassifier for StaticToolRiskClassifier {
    fn classify(&self, tool_name: &str) -> Option<ToolRisk> {
        self.tools.get(tool_name).copied()
    }
}

// ---------------------------------------------------------------------------
// Compiled regex patterns for auto-classification
// ---------------------------------------------------------------------------

/// Destructive patterns — irreversible / highly dangerous operations.
static DESTRUCTIVE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[_\-\s])(?:drop|truncate|purge|erase|wipe|rm|force|kill|destroy)(?:$|[_\-\s])")
        .expect("destructive regex is valid")
});

/// Write patterns — state-mutating operations.
static WRITE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[_\-\s])(?:create|update|delete|send|manage|write|insert|modify|edit|remove)(?:$|[_\-\s])")
        .expect("write regex is valid")
});

/// Read patterns — read-only / safe operations.
static READ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[_\-\s])(?:get|list|search|fetch|view|read|show|find|query|check|describe)(?:$|[_\-\s])")
        .expect("read regex is valid")
});

/// Auto tool risk classifier using compiled regex patterns.
///
/// Classification priority (first match wins):
/// 1. Destructive patterns
/// 2. Write patterns
/// 3. Read patterns
/// 4. Default: `Write`
///
/// A `StaticToolRiskClassifier` can be layered on top for explicit overrides.
#[derive(Debug, Clone, Default)]
pub struct AutoToolRiskClassifier {
    /// Static overrides take precedence over pattern matching.
    overrides: StaticToolRiskClassifier,
}

impl AutoToolRiskClassifier {
    /// Create a new auto classifier with no static overrides.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an auto classifier with static overrides.
    ///
    /// Override entries take precedence over regex classification.
    #[must_use]
    pub fn with_overrides(overrides: StaticToolRiskClassifier) -> Self {
        Self { overrides }
    }

    /// Register a static override for a specific tool name.
    pub fn register_override(&mut self, tool_name: impl Into<String>, risk: ToolRisk) {
        self.overrides.register(tool_name, risk);
    }
}

impl ToolRiskClassifier for AutoToolRiskClassifier {
    fn classify(&self, tool_name: &str) -> Option<ToolRisk> {
        // 1. Check static overrides first
        if let Some(risk) = self.overrides.classify(tool_name) {
            return Some(risk);
        }

        // 2. Match against compiled patterns (destructive first, then write, then read)
        if DESTRUCTIVE_RE.is_match(tool_name) {
            return Some(ToolRisk::Destructive);
        }
        if WRITE_RE.is_match(tool_name) {
            return Some(ToolRisk::Write);
        }
        if READ_RE.is_match(tool_name) {
            return Some(ToolRisk::Read);
        }

        // 3. Default: Write (not Read)
        Some(ToolRisk::Write)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_classifier_known_tool() {
        let mut classifier = StaticToolRiskClassifier::new();
        classifier.register("file_read", ToolRisk::Read);
        classifier.register("file_write", ToolRisk::Write);
        classifier.register("file_delete", ToolRisk::Destructive);

        assert_eq!(classifier.classify("file_read"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("file_write"), Some(ToolRisk::Write));
        assert_eq!(
            classifier.classify("file_delete"),
            Some(ToolRisk::Destructive)
        );
    }

    #[test]
    fn static_classifier_unknown_tool() {
        let classifier = StaticToolRiskClassifier::new();
        assert_eq!(classifier.classify("unknown"), None);
        assert_eq!(classifier.classify_or_default("unknown"), ToolRisk::Write);
    }

    #[test]
    fn tool_risk_serialization() {
        let risk = ToolRisk::Destructive;
        let json = serde_json::to_string(&risk).unwrap();
        assert_eq!(json, "\"destructive\"");
    }

    // --- AutoToolRiskClassifier tests ---

    #[test]
    fn auto_classifier_destructive_patterns() {
        let classifier = AutoToolRiskClassifier::new();
        // Exact keywords
        assert_eq!(classifier.classify("drop_table"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("truncate_log"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("purge_cache"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("erase_disk"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("wipe_data"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("rm_file"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("force_delete"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("kill_process"), Some(ToolRisk::Destructive));
        assert_eq!(classifier.classify("destroy_resource"), Some(ToolRisk::Destructive));
    }

    #[test]
    fn auto_classifier_write_patterns() {
        let classifier = AutoToolRiskClassifier::new();
        assert_eq!(classifier.classify("create_user"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("update_record"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("delete_item"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("send_email"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("manage_settings"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("write_file"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("insert_row"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("modify_config"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("edit_page"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("remove_entry"), Some(ToolRisk::Write));
    }

    #[test]
    fn auto_classifier_read_patterns() {
        let classifier = AutoToolRiskClassifier::new();
        assert_eq!(classifier.classify("get_user"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("list_items"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("search_docs"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("fetch_data"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("view_report"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("read_config"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("show_status"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("find_record"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("query_index"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("check_health"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("describe_instance"), Some(ToolRisk::Read));
    }

    #[test]
    fn auto_classifier_default_is_write() {
        let classifier = AutoToolRiskClassifier::new();
        // Names that don't match any pattern should default to Write
        assert_eq!(classifier.classify("calc"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("transform"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("process"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify_or_default("unknown"), ToolRisk::Write);
    }

    #[test]
    fn auto_classifier_destructive_takes_precedence_over_write() {
        let classifier = AutoToolRiskClassifier::new();
        // "delete" matches both destructive and write keywords — destructive wins
        // because destructive is checked first
        // "force" is destructive; "update" is write
        assert_eq!(classifier.classify("force_update"), Some(ToolRisk::Destructive));
    }

    #[test]
    fn auto_classifier_static_overrides_take_precedence() {
        let mut overrides = StaticToolRiskClassifier::new();
        overrides.register("drop_table", ToolRisk::Read); // Override destructive to read
        overrides.register("custom_tool", ToolRisk::Destructive);

        let classifier = AutoToolRiskClassifier::with_overrides(overrides);

        // Override beats pattern
        assert_eq!(classifier.classify("drop_table"), Some(ToolRisk::Read));
        // Custom tool not matching any pattern but registered
        assert_eq!(classifier.classify("custom_tool"), Some(ToolRisk::Destructive));
        // Non-override still falls through to pattern matching
        assert_eq!(classifier.classify("get_user"), Some(ToolRisk::Read));
    }

    #[test]
    fn auto_classifier_register_override_method() {
        let mut classifier = AutoToolRiskClassifier::new();
        // Before override: "purge" matches destructive
        assert_eq!(classifier.classify("purge_all"), Some(ToolRisk::Destructive));

        // Override to Read
        classifier.register_override("purge_all", ToolRisk::Read);
        assert_eq!(classifier.classify("purge_all"), Some(ToolRisk::Read));
    }

    #[test]
    fn auto_classifier_case_insensitive() {
        let classifier = AutoToolRiskClassifier::new();
        assert_eq!(classifier.classify("GET_User"), Some(ToolRisk::Read));
        assert_eq!(classifier.classify("CREATE_Item"), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("DROP_Table"), Some(ToolRisk::Destructive));
    }

    #[test]
    fn auto_classifier_always_returns_some() {
        let classifier = AutoToolRiskClassifier::new();
        // Auto classifier never returns None — always falls back to Write
        assert_eq!(classifier.classify(""), Some(ToolRisk::Write));
        assert_eq!(classifier.classify("xyz_no_match"), Some(ToolRisk::Write));
    }
}
