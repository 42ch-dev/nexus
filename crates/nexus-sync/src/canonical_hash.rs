//! Canonical content hash for delta bundles (TD-5).
//!
//! **Preimage (this crate):** `serde_json::to_vec` bytes of the bundle's `deltas`
//! array (see [`canonical_hash_for_deltas`]). Cross-stack alignment (exact JSON
//! shape, key order, number formatting) must match `nexus-platform` / bundle
//! schema — see residual `ALIGN-HASH-01` / golden vectors before GA.
//!
//! Wire format: `sha256:` + 64 lowercase hex digits (see [`is_well_formed_canonical_hash`]).
//! Same delta sequence and contents produce the same hash for a fixed serialization.

use crate::errors::{SyncError, SyncResult};
use nexus_contracts::generated::Delta;
use sha2::{Digest, Sha256};

/// Returns true if `s` is `sha256:` followed by 64 lowercase hex digits.
pub fn is_well_formed_canonical_hash(s: &str) -> bool {
    let rest = s.strip_prefix("sha256:");
    let Some(hex_digits) = rest else {
        return false;
    };
    hex_digits.len() == 64
        && hex_digits
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

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
    fn well_formed_hash_accepts_generated() {
        let h = canonical_hash_for_deltas(&[sample_delta()]).unwrap();
        assert!(is_well_formed_canonical_hash(&h));
    }

    #[test]
    fn well_formed_rejects_uppercase_hex() {
        assert!(!is_well_formed_canonical_hash(
            "sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        ));
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
