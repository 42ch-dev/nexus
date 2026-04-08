//! Canonical content hash for delta bundles (TD-5).
//!
//! Computes `sha256:<hexlower>` over the JSON serialization of the bundle's
//! `deltas` array. Same delta sequence and contents produce the same hash.

use crate::errors::{SyncError, SyncResult};
use nexus_contracts::generated::Delta;
use sha2::{Digest, Sha256};

/// SHA-256 over `serde_json` serialization of `deltas` (deterministic for a given slice).
pub fn canonical_hash_for_deltas(deltas: &[Delta]) -> SyncResult<String> {
    let payload = serde_json::to_vec(deltas).map_err(SyncError::from)?;
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let out = hasher.finalize();
    Ok(format!("sha256:{}", hex::encode(out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::{DeltaOperation, DeltaType};
    use serde_json::json;

    fn sample_delta() -> Delta {
        Delta {
            delta_type: DeltaType::KeyBlock,
            operation: DeltaOperation::Create,
            target_entity_type: Some("key_block".to_string()),
            target_entity_id: None,
            payload: json!({"display_name": "Test"}),
            source_anchor: None,
            local_timestamp: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn identical_deltas_identical_hash() {
        let d = vec![sample_delta()];
        let a = canonical_hash_for_deltas(&d).unwrap();
        let b = canonical_hash_for_deltas(&d).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
        assert_eq!(a.len(), 7 + 64);
    }

    #[test]
    fn different_payload_different_hash() {
        let mut a = sample_delta();
        let mut b = sample_delta();
        b.payload = json!({"display_name": "Other"});
        let ha = canonical_hash_for_deltas(&[a]).unwrap();
        let hb = canonical_hash_for_deltas(&[b]).unwrap();
        assert_ne!(ha, hb);
    }
}
