//! Canonical content hash for delta bundles (TD-5).
//!
//! **Preimage (this crate):** `serde_json::to_vec` bytes of the bundle's **`deltas` JSON array
//! only** (not the full `Bundle` object). Normative spec: v1-spec **ADR-006**; OSS companion doc:
//! `.agents/plans/knowledge/canonical-hash-v1.md`. Unit test `golden_alignment_vector_matches_documented_digest`
//! locks the ADR §3.3 golden vector. Other stacks must match these bytes for interop.
//!
//! Wire format: `sha256:` + 64 lowercase hex digits (see [`is_well_formed_canonical_hash`]).
//! Same delta sequence and contents produce the same hash for a fixed serialization.

use crate::errors::{SyncError, SyncResult};
use nexus_contracts::Delta;
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
    use nexus_contracts::{Delta, DeltaOperation, DeltaType};
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
        let a = sample_delta();
        let b = {
            let mut b = sample_delta();
            b.payload = json!({"display_name": "Other"});
            b
        };
        let ha = canonical_hash_for_deltas(&[a]).unwrap();
        let hb = canonical_hash_for_deltas(&[b]).unwrap();
        assert_ne!(ha, hb);
    }

    /// Frozen cross-stack fixture: changing field values requires updating
    /// `.agents/plans/knowledge/canonical-hash-v1.md` and any platform golden vectors.
    fn golden_alignment_fixture_delta() -> Delta {
        Delta {
            delta_type: DeltaType::KeyBlock,
            operation: DeltaOperation::Create,
            target_entity_type: Some("key_block".to_string()),
            target_entity_id: None,
            payload: json!({"display_name": "Golden"}),
            source_anchor: None,
            local_timestamp: "2026-04-09T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn golden_alignment_vector_matches_documented_digest() {
        // Frozen digest for `golden_alignment_fixture_delta` (serde_json::to_vec on `[delta]`).
        const EXPECTED: &str =
            "sha256:b9c07221605405f763956471055fed2ecdfdce7858f423a371aa387eec8befab";
        let got = canonical_hash_for_deltas(&[golden_alignment_fixture_delta()]).unwrap();
        assert_eq!(got, EXPECTED, "keep in sync with .agents/plans/knowledge/canonical-hash-v1.md");
    }
}
