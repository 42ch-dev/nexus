//! Provider policy the fixture publishes, and the explicit authenticated
//! peer→principal table admission is built from (§4.1, §4.2, §7.0.6, §7.1).
//!
//! Everything here is the fixture's own declaration: it serves `spoke-memory`
//! and — depending on the construction flag — `spoke-memory-reflection`. The
//! optional transfer family is neither advertised nor implemented. The limits
//! are the bounds the private operation slices actually enforce, and
//! `engine_policy_version` is the fixture's own stamp rather than Nexus's, so
//! a client can tell the two providers apart.

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::Arc;

use serde_json::{json, Value};
use spoke_connect::remote::MemoryAdmission;
use spoke_operations::spoke_schemas::Principal;
use spoke_operations::{SpokeReject, SpokeRejectCode, SpokeResult};

use crate::candidates::{
    DEDUP_RETENTION_SECONDS, IDEMPOTENCY_RETENTION_SECONDS, MAX_CONTENT_BYTES,
    RECEIPT_RETENTION_SECONDS,
};
use crate::spaces::{ENGINE_POLICY_VERSION, MAX_PAGE_SIZE};

/// §3: the memory protocol revision this provider speaks.
pub const MEMORY_PROTOCOL_VERSION: u64 = 1;

/// §4.1: the families this fixture serves. All three reflection operations
/// are genuinely served, so `spoke-memory-reflection` is advertised exactly
/// when the provider is constructed advertised (C-003 honesty: no capability
/// claim without serving, and no silent omission of a served family).
pub const BASE_CAPABILITY: &str = "spoke-memory";
pub const REFLECTION_CAPABILITY: &str = "spoke-memory-reflection";

/// The capability list for one provider shape.
#[must_use]
pub fn capabilities(advertises_reflection: bool) -> Vec<&'static str> {
    if advertises_reflection {
        vec![BASE_CAPABILITY, REFLECTION_CAPABILITY]
    } else {
        vec![BASE_CAPABILITY]
    }
}

/// §4.2: authored content kinds this provider requires before generation.
///
/// It generates reflections without an authored-kind prerequisite, so it
/// requires no authored kind; the insufficient-data outcome is
/// `INSUFFICIENT_DATA`-driven, not a profile-kind demand.
const REQUIRED_PROFILE_KINDS: [&str; 0] = [];

/// Largest candidate count one `memory.capture` can produce: the fixture's
/// advisory classifier emits at most one candidate per capture.
const MAX_CANDIDATES_PER_CAPTURE: u64 = 1;

// The transfer family is not served, so these three schema-required bounds back
// no operation of this provider. They are positive only because
// `memory.describe.limits` requires every member; nothing else reads them.
const EXPORT_CHUNK_BYTES: u64 = 262_144;
const EXPORT_RETENTION_SECONDS: u64 = 86_400;
const STAGED_IMPORT_TTL_SECONDS: u64 = 86_400;

fn non_zero(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap_or(NonZeroU64::MIN)
}

/// §7.1 `limits` payload, built from the bounds the operation slices enforce.
fn limits() -> Value {
    json!({
        "max_content_bytes": non_zero(u64::try_from(MAX_CONTENT_BYTES).unwrap_or(u64::MAX)),
        "max_candidates_per_capture": non_zero(MAX_CANDIDATES_PER_CAPTURE),
        "max_page_size": MAX_PAGE_SIZE,
        "export_chunk_bytes": non_zero(EXPORT_CHUNK_BYTES),
        "idempotency_retention_seconds": non_zero(IDEMPOTENCY_RETENTION_SECONDS),
        "receipt_retention_seconds": non_zero(RECEIPT_RETENTION_SECONDS),
        "dedup_retention_seconds": non_zero(DEDUP_RETENTION_SECONDS),
        "staged_import_ttl_seconds": non_zero(STAGED_IMPORT_TTL_SECONDS),
        "export_retention_seconds": non_zero(EXPORT_RETENTION_SECONDS),
    })
}

/// §7.1 `memory.describe` success body, for one provider shape.
#[must_use]
pub fn describe_body(advertises_reflection: bool) -> Value {
    json!({
        "memory_protocol_version": non_zero(MEMORY_PROTOCOL_VERSION),
        "capabilities": capabilities(advertises_reflection),
        "limits": limits(),
        "required_profile_kinds": REQUIRED_PROFILE_KINDS,
        "engine_policy_version": ENGINE_POLICY_VERSION,
    })
}

/// The explicit, operator-supplied `authenticated peer_id → principal_id`
/// table the memory admission reads (MEM-R003).
///
/// It is the only source of requester identity: the principal is looked up from
/// the connect-authenticated peer id, never from a payload field, a creator
/// field or a session id. A peer that is not in the table is refused — the
/// table is the authority, not a fallback to a derived or default principal.
#[derive(Debug, Clone, Default)]
pub struct PeerDirectory {
    principals: BTreeMap<String, String>,
}

impl PeerDirectory {
    /// An empty table: every peer is currently refused.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind one authenticated peer id to one principal id.
    ///
    /// A peer maps to one stable principal, so reconnecting or restarting the
    /// transport keeps the same memory identity and therefore the same grants,
    /// receipts and dedup namespace.
    #[must_use]
    pub fn with_peer(
        mut self,
        peer_id: impl Into<String>,
        principal_id: impl Into<String>,
    ) -> Self {
        self.principals.insert(peer_id.into(), principal_id.into());
        self
    }

    /// The principal admitted for one authenticated peer, or a terminal refusal.
    ///
    /// # Errors
    ///
    /// Returns `NOT_AUTHORIZED` for a peer the operator never mapped, and
    /// `INTERNAL_ERROR` when the stored principal id is not protocol-shaped.
    /// That refusal carries static text only: the operator-supplied value and
    /// the decoder's own message never reach the wire.
    pub fn principal_for(&self, peer_id: &str) -> Result<Principal, SpokeReject> {
        let Some(principal_id) = self.principals.get(peer_id) else {
            return Err(SpokeReject {
                code: SpokeRejectCode::NotAuthorized,
                message: format!("peer {peer_id} has no authenticated memory principal"),
                details: None,
            });
        };
        serde_json::from_value(json!({
            "principal_id": principal_id,
            "extensions": {},
        }))
        .map_err(|_| SpokeReject {
            code: SpokeRejectCode::InternalError,
            message: "the stored memory principal is not protocol-shaped; refusing".to_owned(),
            details: None,
        })
    }

    /// The admission closure the SDK's memory service face takes.
    #[must_use]
    pub fn admission(self: &Arc<Self>) -> MemoryAdmission {
        let directory = Arc::clone(self);
        Arc::new(
            move |peer_id: &str| match directory.principal_for(peer_id) {
                Ok(principal) => SpokeResult::Ok(principal),
                Err(reject) => SpokeResult::Reject(reject),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use spoke_operations::{SpokeRejectCode, SpokeResult};

    use super::PeerDirectory;

    /// `with_peer` stores operator input verbatim, and the protocol requires a
    /// nonempty `principal_id`, so an empty stored principal is a malformed row
    /// an operator can really produce. It must refuse through the SDK's
    /// admission face — never admit a principal, never widen into a default.
    #[test]
    fn an_empty_stored_principal_is_refused_through_the_admission_face() {
        let directory = Arc::new(PeerDirectory::new().with_peer("peer-a", ""));
        let reject = match directory.admission()("peer-a") {
            SpokeResult::Reject(reject) => reject,
            SpokeResult::Ok(_) => panic!("a malformed stored principal must not be admitted"),
        };
        assert_eq!(reject.code, SpokeRejectCode::InternalError);
    }
}
