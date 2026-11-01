//! Nexus-owned deterministic provider policy (spoke-memory §4.1, §7.1).
//!
//! Round A declares the surface the provider serves and the bounds it enforces.
//! Nothing here calls out to a model or the network: the quota/retention values
//! are operator policy, `engine_policy_version` is the opaque stamp clients pin
//! recall/reflection bases to, and the §8.2 freeze allowlist is the protocol's
//! own predicate (re-used from `spoke_operations`, never re-implemented here).
//!
//! Round B extends this module with the candidate classifier, recall scoring +
//! budget, and staleness computation the content families need.

use std::num::NonZeroU64;

use serde_json::{json, Value};

/// §3: the memory protocol revision this provider speaks.
pub(super) const MEMORY_PROTOCOL_VERSION: u64 = 1;

/// §4.1: the family flag served. The sub-capabilities
/// (`spoke-memory-reflection`, `spoke-memory-transfer`) are not served, so they
/// are never advertised and their ops refuse `op_unsupported`.
pub(super) const CAPABILITIES: [&str; 1] = ["spoke-memory"];

/// §4.2: the authored content kinds this provider requires before generation.
pub(super) const REQUIRED_PROFILE_KINDS: [&str; 1] = ["personality"];

/// Opaque provider policy version (§6.1). Bumped whenever a policy function's
/// output would change for unchanged input.
pub(super) const ENGINE_POLICY_VERSION: &str = "nexus-local-memory-v1";

/// §7.1 `limits`. Every value is a positive integer bound the provider enforces
/// or guarantees; `dedup_retention_seconds` covers
/// `idempotency_retention_seconds` as MEM-R021 requires.
const MAX_CONTENT_BYTES: u64 = 65_536;
const MAX_CANDIDATES_PER_CAPTURE: u64 = 32;
const MAX_PAGE_SIZE: u64 = 100;
const EXPORT_CHUNK_BYTES: u64 = 262_144;
const IDEMPOTENCY_RETENTION_SECONDS: u64 = 2_592_000;
const RECEIPT_RETENTION_SECONDS: u64 = 2_592_000;
const DEDUP_RETENTION_SECONDS: u64 = 5_184_000;
const STAGED_IMPORT_TTL_SECONDS: u64 = 86_400;
const EXPORT_RETENTION_SECONDS: u64 = 86_400;

/// §7.1 `limits` payload.
pub(super) fn limits() -> Value {
    json!({
        "max_content_bytes": MAX_CONTENT_BYTES,
        "max_candidates_per_capture": MAX_CANDIDATES_PER_CAPTURE,
        "max_page_size": MAX_PAGE_SIZE,
        "export_chunk_bytes": EXPORT_CHUNK_BYTES,
        "idempotency_retention_seconds": IDEMPOTENCY_RETENTION_SECONDS,
        "receipt_retention_seconds": RECEIPT_RETENTION_SECONDS,
        "dedup_retention_seconds": DEDUP_RETENTION_SECONDS,
        "staged_import_ttl_seconds": STAGED_IMPORT_TTL_SECONDS,
        "export_retention_seconds": EXPORT_RETENTION_SECONDS,
    })
}

/// §7.0.7 page bound: the caller's `limit` defaults to and may never exceed the
/// advertised `max_page_size`.
pub(super) const MAX_PAGE_SIZE_BOUND: NonZeroU64 = match NonZeroU64::new(MAX_PAGE_SIZE) {
    Some(bound) => bound,
    None => panic!("max_page_size must be positive"),
};
