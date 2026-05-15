//! Tool risk classification.
//!
//! Wave 1: static declaration only with extension point for future
//! auto-classification (R-006).

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
}
