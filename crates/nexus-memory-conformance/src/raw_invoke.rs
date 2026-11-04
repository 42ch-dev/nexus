//! D15 raw-request bridge: the exact received-object digest, carried in a
//! per-invoke scope, plus the wire error-envelope normalization.
//!
//! MEM-R008 makes `payload_digest` the JCS digest of the request object the
//! transport received, and MEM-R021 hangs replay on that digest. A typed client
//! cannot express every received shape — serde omits an empty
//! `source_context_ids` on serialization, so an explicit `[]` and an omitted
//! member are the same typed request with different wire objects — so the
//! digest must be taken over the received [`Value`] *before* it is decoded, and
//! once.
//!
//! The bridge therefore computes the digest over the received object inside the
//! service's `invoke`, then *moves* that same object into the unchanged
//! `MemoryPortService`. Only the digest plus the op and provider it was
//! received for travel in the scope; no raw body is cloned, cached, logged or
//! kept in a request map. The scope is a task-local established inside
//! `invoke`, so the SDK responder's spawned serve task owns exactly its own
//! request: concurrent invokes stay isolated, and a detached invoke that
//! outlives the client's timeout keeps its digest until its work ends.
//!
//! A typed port method that needs the digest asks for it with
//! [`admitted`](admitted). A missing or mismatched scope is refused before any
//! effect, so a direct in-process port call cannot fabricate one.

use std::future::Future;

use serde_json::Value;
use spoke_operations::{
    jcs_sha256_hex, SpokeReject, SpokeRejectCode, SpokeResult,
};

use crate::candidates::{required_capabilities, Admitted};

tokio::task_local! {
    /// The one admitted request this task is serving right now.
    static REQUEST: RequestScope;
}

/// Digest and identity of the request object one `invoke` received.
#[derive(Clone, Debug)]
pub(crate) struct RequestScope {
    digest: String,
    op: String,
    provider: String,
}

/// Compute the request digest for `op` if the op is a mutating one.
///
/// The digest is JCS over the received object, taken once, and the object is
/// left for the caller to move into the service. Read-only ops carry no
/// receipt and therefore no digest.
pub(crate) fn admission(
    payload: &Value,
    op: &str,
    provider: &str,
) -> Result<Option<RequestScope>, SpokeReject> {
    // The published §7 capability table is the single statement of which ops
    // mutate; it is the same table the receipt target uses, so no second op
    // classification exists here.
    if required_capabilities(op).is_none() {
        return Ok(None);
    }
    match jcs_sha256_hex(payload) {
        SpokeResult::Ok(digest) => Ok(Some(RequestScope {
            digest,
            op: op.to_owned(),
            provider: provider.to_owned(),
        })),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

/// Run `future` with `scope` as the task's admitted request.
pub(crate) async fn with_scope<F: Future>(scope: RequestScope, future: F) -> F::Output {
    REQUEST.scope(scope, future).await
}

/// The digest admitted for `op` on `provider`, paired with the typed view the
/// port method parsed from that same object.
///
/// Fails closed: a call with no scope, or with the scope of another op or
/// another provider, is refused before the caller can touch storage.
pub(crate) fn admitted<Request>(
    op: &str,
    provider: &str,
    request: Request,
) -> Result<Admitted<Request>, SpokeReject> {
    REQUEST
        .try_with(|scope| {
            if scope.op == op && scope.provider == provider {
                Ok(Admitted::from_scope(scope.digest.clone(), request))
            } else {
                Err(scope_missing(op))
            }
        })
        .unwrap_or_else(|_| Err(scope_missing(op)))
}

fn scope_missing(op: &str) -> SpokeReject {
    let mut details = serde_json::Map::new();
    details.insert(
        "kind".to_owned(),
        Value::String("request_scope_missing".to_owned()),
    );
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: format!(
            "{op} was invoked outside its admitted request scope; refusing before any effect"
        ),
        details: Some(details),
    }
}

/// §14.2 retry class of one memory-boundary error envelope.
///
/// Only an explicitly unknown outcome is `uncertain`, and `INTERNAL_ERROR` is
/// `retryable` only because this provider raises it for failures asserted to
/// have committed nothing (§14.3 — post-commit loss is `OUTCOME_UNCERTAIN`).
fn retry_class(code: SpokeRejectCode) -> &'static str {
    match code {
        SpokeRejectCode::OutcomeUncertain => "uncertain",
        SpokeRejectCode::InternalError => "retryable",
        _ => "terminal",
    }
}

/// Supply the protocol-required `details.retry_class` on one served result,
/// preserving the provider's code, message and existing details.
///
/// This is the memory-boundary normalization: the C4 fault and retention
/// producers deliberately carry no envelope class, and the dispatch layer only
/// classifies its own denials, so the provider's answers reach the wire as
/// §14.2 envelopes here.
pub(crate) fn normalize<T>(result: SpokeResult<T>) -> SpokeResult<T> {
    match result {
        SpokeResult::Reject(mut reject) => {
            let details = reject.details.get_or_insert_with(serde_json::Map::new);
            details.insert(
                "retry_class".to_owned(),
                Value::String(retry_class(reject.code).to_owned()),
            );
            SpokeResult::Reject(reject)
        }
        ok => ok,
    }
}
