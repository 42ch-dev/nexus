//! RuntimeMode — local-only enum.
//!
//! Creator runtime mode controlling platform dependency behavior.
//! See ADR-015, ADR-017.

use serde::{Deserialize, Serialize};

/// Creator runtime mode controlling platform dependency behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// No platform HTTP dependency for Creator's end-to-end loop (ADR-017).
    #[serde(rename = "local_only")]
    #[default]
    LocalOnly,
    /// Optional platform structured services allowed; platform must not call
    /// generative LLM on behalf of Creator (ADR-015).
    #[serde(rename = "local_first")]
    LocalFirst,
    /// Full platform capabilities including hosted LLM (ADR-015).
    #[serde(rename = "cloud_enhanced")]
    CloudEnhanced,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_json_all_variants() {
        for mode in [
            RuntimeMode::LocalOnly,
            RuntimeMode::LocalFirst,
            RuntimeMode::CloudEnhanced,
        ] {
            let s = serde_json::to_string(&mode).unwrap();
            let back: RuntimeMode = serde_json::from_str(&s).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn default_is_local_only() {
        assert_eq!(RuntimeMode::default(), RuntimeMode::LocalOnly);
    }
}
