//! Placeholder module until schema codegen is implemented

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceholderContract {
    pub schema_version: String,
}

impl Default for PlaceholderContract {
    fn default() -> Self {
        Self {
            schema_version: "0.1.0".to_string(),
        }
    }
}
