//! Adapter-owned production extraction wrapper (v1.191 P1 T12, durable §8).
//!
//! `spoke_operations::adapter::orchestrate_extract` is the sole gate/assembly
//! path for a `ke-extraction` run, and it is generic over a product
//! [`ExtractionPort`] plus one injected callback. This module supplies nexus's
//! half of that boundary and nothing else:
//!
//! - [`ResolvedExtractionPort`] answers `load_extraction_input` from an
//!   already admitted, bounded native source bundle. It performs no source
//!   I/O: the orchestration layer resolved the bundle (its resolver, its
//!   admission) and injects it, so the adapter never touches a filesystem,
//!   job queue, session or cancellation token.
//! - [`extract_candidates`] awaits that port and the caller's native callback
//!   through [`orchestrate_extract`] and returns the orchestrator's validated
//!   response together with the native relationship sidecar.
//!
//! # Layering (durable §8)
//!
//! The adapter does not import `nexus-orchestration` or `nexus-core`: run and
//! session identity, cancellation, source resolution and governance resolution
//! all stay with the caller. `ExtractionPort` is not a member of
//! `BaselinePorts`/`FullPorts` upstream, so this module adds no baseline/full
//! port obligation, and the adapter's port impls stay unchanged.
//!
//! # What the wrapper does not do
//!
//! It persists nothing. Candidates are prepared (id allocated once, trusted
//! governance attached) by [`nexus_knowledge::world_kb::prepare_extract`] and
//! persisted by the caller through
//! [`nexus_knowledge::world_kb::persist_prepared_extract`] inside its own
//! atomic job transaction. A rejected protocol run, a source-load failure or a
//! dropped (cancelled) future therefore leaves no write behind: the returned
//! [`ExtractCandidatesOutcome`] is the only carrier of prepared candidates and
//! of the relationship sidecar, and it exists only on success.
//!
//! Relationship candidates ride in that sidecar because SPOKE's success arm
//! carries `KnowledgeEntry`s only; they are neither dropped nor reconstructed
//! here.

use std::future::Future;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nexus_knowledge::world_kb::PreparedExtractCandidate;
use serde_json::{json, Map, Value};
use spoke_operations::{orchestrate_extract, spoke_ok, spoke_ok_unit, spoke_reject};

use crate::conversion::knowledge_record_to_spoke;
use crate::{
    ExtractRequest, ExtractResponse, ExtractRunInput, ExtractionPort, ExtractionResult,
    SpokeRejectCode, SpokeResult,
};

/// An already admitted, bounded native source bundle resolved for exactly one
/// extraction run.
///
/// `run_id` is the real run identity the orchestration layer resolved the
/// bundle for; the wrapper never mints one.
#[derive(Debug, Clone)]
pub struct ResolvedExtractionInput {
    /// The run identity the bundle belongs to. A request that does not echo it
    /// is refused before any callback runs.
    pub run_id: String,
    /// Opaque in-process native bundle (admitted source text and anchors).
    /// Never a wire object and never serialized onto `ExtractRequest` /
    /// `ExtractResponse`.
    pub bundle: Value,
}

/// The adapter-owned [`ExtractionPort`] over one admitted bundle.
///
/// The bundle is injected, not loaded: `load_extraction_input` only verifies
/// that the orchestrator is asking for the run the bundle was resolved for and
/// hands the bundle back. There is no source I/O here, and no default is
/// synthesized when the run identity disagrees.
pub struct ResolvedExtractionPort {
    input: ResolvedExtractionInput,
}

impl ResolvedExtractionPort {
    /// Wrap one admitted bundle as an [`ExtractionPort`].
    #[must_use]
    pub const fn new(input: ResolvedExtractionInput) -> Self {
        Self { input }
    }
}

#[async_trait]
impl ExtractionPort for ResolvedExtractionPort {
    async fn load_extraction_input(&self, request: &ExtractRequest) -> SpokeResult<Value> {
        // The bundle belongs to exactly one run. A request for another run is a
        // caller wiring error, not something to paper over with the bundle at
        // hand.
        if request.run_id.as_str() != self.input.run_id {
            let mut details = Map::new();
            details.insert("field".into(), Value::String("run_id".into()));
            details.insert(
                "resolved_run_id".into(),
                Value::String(self.input.run_id.clone()),
            );
            details.insert(
                "request_run_id".into(),
                Value::String(request.run_id.as_str().to_owned()),
            );
            return spoke_reject(
                SpokeRejectCode::InvalidInput,
                "resolved extraction bundle belongs to a different run",
                Some(details),
            );
        }
        spoke_ok(self.input.bundle.clone())
    }
}

/// Native callback output: prepared candidates plus advisory run metadata and
/// the relationship sidecar.
///
/// The candidates come from
/// [`nexus_knowledge::world_kb::prepare_extract`], so their ids are already
/// allocated and their governance already attached; the adapter converts them
/// to the wire type without touching either.
#[derive(Debug, Clone)]
pub struct NativeExtractionOutput {
    /// Prepared candidates (`entry_id` and governance final).
    pub candidates: Vec<PreparedExtractCandidate>,
    /// Optional open string naming the extraction method.
    pub method: Option<String>,
    /// Optional advisory coverage hint, retained verbatim.
    pub coverage_hint: Option<Value>,
    /// Relationship candidates. SPOKE's success arm carries `KnowledgeEntry`s
    /// only, so these ride beside the response and are never dropped.
    pub relationships: Vec<Value>,
}

/// One accepted extraction: the upstream response plus the native candidates
/// that produced it and the relationship sidecar.
///
/// `candidates` are the exact prepared records the caller persists (through
/// `persist_prepared_extract`) after the job's own atomic guarantees; their ids
/// equal the ids echoed on `response`.
#[derive(Debug, Clone)]
pub struct ExtractCandidatesOutcome {
    /// Upstream-validated response (candidates + run metadata echo).
    pub response: ExtractResponse,
    /// The prepared candidates behind `response`'s entries, in order.
    pub candidates: Vec<PreparedExtractCandidate>,
    /// Native relationship sidecar, verbatim.
    pub relationships: Vec<Value>,
}

/// Run one extraction through the upstream orchestrator over an admitted
/// bundle and the caller's native callback.
///
/// Ordering and gating are upstream's: request boundaries → port load →
/// callback (exactly once) → candidate status gate → response assembly. This
/// wrapper adds only the native→wire conversion of the prepared candidates and
/// the protocol validation below.
///
/// Cancellation is the caller's future being dropped — the wrapper spawns
/// nothing and swallows nothing, so dropping it after the source load or
/// mid-callback stops the run before any candidate is persisted.
///
/// # `Send` bridge (v1.191 P1 T13)
///
/// `spoke_operations::orchestrate_extract` is the one upstream orchestrator
/// whose injected source loader is a trait object (`ports: &dyn ExtractionPort`)
/// — every other family takes `&impl BaselinePorts`. A `dyn` trait object
/// carries no `Sync` bound, so the orchestrator's future is `!Send`, while both
/// nexus production extraction callers are `Send`-bound `#[async_trait]`
/// capabilities (`Capability: Send + Sync`) driven from spawned tasks. The
/// adapter therefore drives the orchestrator on its own short-lived driver
/// thread (see [`drive_orchestrator`]) instead of awaiting it inline, and both
/// the callback and the request gain `'static` so they can cross to it.
pub async fn extract_candidates<F, Fut>(
    request: ExtractRequest,
    resolved_input: ResolvedExtractionInput,
    extractor: F,
) -> SpokeResult<ExtractCandidatesOutcome>
where
    F: FnOnce(ExtractRunInput) -> Fut + Send + 'static,
    Fut: Future<Output = SpokeResult<NativeExtractionOutput>> + Send + 'static,
{
    let request_run_id = request.run_id.as_str().to_owned();

    // The native output is produced inside the callback and must survive
    // upstream's fixed `SpokeResult<ExtractionResult>` callback shape, so the
    // callback hands it back through this slot.
    let native_output: Arc<Mutex<Option<NativeExtractionOutput>>> = Arc::new(Mutex::new(None));
    let response = {
        let slot = Arc::clone(&native_output);
        drive_orchestrator(request, resolved_input, move |run_input| {
            let slot = Arc::clone(&slot);
            async move {
                let mut native = match extractor(run_input).await {
                    SpokeResult::Ok(native) => native,
                    SpokeResult::Reject(reject) => return SpokeResult::Reject(reject),
                };
                let method = native.method.take();
                let coverage_hint = native.coverage_hint.take();
                let candidates = native
                    .candidates
                    .iter()
                    .map(|prepared| knowledge_record_to_spoke(&prepared.record))
                    .collect::<Vec<_>>();
                *slot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(native);
                SpokeResult::Ok(ExtractionResult {
                    candidates,
                    method,
                    coverage_hint,
                })
            }
        })
        .await
    };

    let response = match response {
        SpokeResult::Ok(response) => response,
        SpokeResult::Reject(reject) => return SpokeResult::Reject(reject),
    };

    // Success upstream means the callback ran and stored its native output.
    let native = native_output
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    let Some(native) = native else {
        return spoke_reject(
            SpokeRejectCode::InternalError,
            "extract callback reported success without a native result",
            None,
        );
    };

    match assert_response_echo(&response, &native.candidates, &request_run_id) {
        SpokeResult::Ok(()) => SpokeResult::Ok(ExtractCandidatesOutcome {
            response,
            candidates: native.candidates,
            relationships: native.relationships,
        }),
        SpokeResult::Reject(reject) => SpokeResult::Reject(reject),
    }
}

/// Drive `orchestrate_extract` on a dedicated driver thread.
///
/// The orchestrator's future is `!Send` (see [`extract_candidates`]): it holds a
/// `&dyn ExtractionPort`. Running it here keeps the adapter's public future
/// `Send` for its `Send`-bound callers without touching spoke's lifecycle: the
/// orchestrator still gates the request, loads through the injected port, awaits
/// the callback exactly once, gates candidate status and assembles the response.
///
/// The driver owns a current-thread runtime and hands the `Send` response back
/// over a channel. **Cancellation is preserved**: dropping this future drops
/// `cancel_tx`, the driver's `select!` resolves, and the orchestrator future
/// (with the native callback) is dropped mid-flight — so a cancelled run can
/// never reach the caller's persistence. The driver thread exits with its
/// runtime as soon as the run settles or is cancelled; it is never leaked.
async fn drive_orchestrator<F, Fut>(
    request: ExtractRequest,
    resolved_input: ResolvedExtractionInput,
    extractor: F,
) -> SpokeResult<ExtractResponse>
where
    F: FnOnce(ExtractRunInput) -> Fut + Send + 'static,
    Fut: Future<Output = SpokeResult<ExtractionResult>> + Send + 'static,
{
    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<SpokeResult<ExtractResponse>>();
    // Held for the whole call, never read: dropping it is the cancellation
    // signal the driver's `select!` observes, so the orchestrator future (and
    // the native callback) is dropped when this future is.
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let spawned = std::thread::Builder::new()
        .name("spoke-extract-driver".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    let _ = result_tx.send(spoke_reject(
                        SpokeRejectCode::InternalError,
                        format!("extraction driver runtime: {e}"),
                        None,
                    ));
                    return;
                }
            };
            let port = ResolvedExtractionPort::new(resolved_input);
            let driven = runtime.block_on(async move {
                tokio::select! {
                    response = orchestrate_extract(&port, request, extractor) => Some(response),
                    // Resolves only when the caller's future was dropped.
                    _ = cancel_rx => None,
                }
            });
            if let Some(response) = driven {
                let _ = result_tx.send(response);
            }
        });
    if let Err(e) = spawned {
        return spoke_reject(
            SpokeRejectCode::InternalError,
            format!("extraction driver thread: {e}"),
            None,
        );
    }

    result_rx.await.unwrap_or_else(|_| {
        spoke_reject(
            SpokeRejectCode::InternalError,
            "extraction driver stopped without a response",
            None,
        )
    })
}

/// Protocol validation before the wrapper returns.
///
/// The response must be the success variant, must echo the request run id, and
/// must carry exactly the converted prepared candidates — same ids, same
/// order, none dropped or reconstructed. A violation is a boundary defect and
/// is rejected instead of returning a partly-trusted success.
fn assert_response_echo(
    response: &ExtractResponse,
    prepared: &[PreparedExtractCandidate],
    expected_run_id: &str,
) -> SpokeResult<()> {
    let ExtractResponse::Variant0 {
        candidates, run, ..
    } = response
    else {
        return spoke_reject(
            SpokeRejectCode::InternalError,
            "extract orchestrator returned a failure variant with success status",
            None,
        );
    };

    if run.run_id.as_str() != expected_run_id {
        let mut details = Map::new();
        details.insert("field".into(), Value::String("run_id".into()));
        details.insert("expected".into(), Value::String(expected_run_id.to_owned()));
        details.insert(
            "actual".into(),
            Value::String(run.run_id.as_str().to_owned()),
        );
        return spoke_reject(
            SpokeRejectCode::InternalError,
            "extract response does not echo the request run id",
            Some(details),
        );
    }

    let echoed: Vec<&str> = candidates.iter().map(|c| c.entry_id.as_str()).collect();
    let expected: Vec<&str> = prepared
        .iter()
        .map(|p| p.record.entry_id.as_str())
        .collect();
    if echoed != expected {
        let mut details = Map::new();
        details.insert("expected".into(), json!(expected));
        details.insert("echoed".into(), json!(echoed));
        return spoke_reject(
            SpokeRejectCode::InternalError,
            "extract response candidates differ from the prepared candidates",
            Some(details),
        );
    }

    spoke_ok_unit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody;
    use nexus_knowledge::world_kb::source_anchor::SourceAnchor;
    use nexus_knowledge::world_kb::store::{InMemoryKbStore, KbStore};
    use nexus_knowledge::world_kb::{
        persist_prepared_extract, prepare_extract, ExtractPrepareInput, KnowledgeGovernance,
        ValidationMode, DISCLOSURE_OWNER_PRIVATE,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    const RUN_ID: &str = "xrun_t12";
    const WORLD_ID: &str = "wld_t12";

    fn request(run_id: &str) -> ExtractRequest {
        serde_json::from_value(json!({
            "run_id": run_id,
            "sources": [{
                "schema_version": 1,
                "source_id": "Works/novel/Chapters/03.md",
                "extensions": {},
            }],
        }))
        .expect("valid ExtractRequest")
    }

    fn admitted_bundle(run_id: &str) -> ResolvedExtractionInput {
        ResolvedExtractionInput {
            run_id: run_id.to_string(),
            bundle: json!({ "chapters": [{ "path": "03.md", "text": "Lin Xia drew her blade." }] }),
        }
    }

    fn prepare_input(canonical_name: &str, governance: KnowledgeGovernance) -> ExtractPrepareInput {
        ExtractPrepareInput {
            world_id: WORLD_ID.to_string(),
            block_type: BlockType::Character,
            canonical_name: canonical_name.to_string(),
            body: KnowledgeEntryBody {
                summary: Some("A brave warrior".to_string()),
                attributes: Some(json!({ "novel_category": "character" })),
                tags: Some(vec!["novel".to_string()]),
                ..Default::default()
            },
            source_anchor: SourceAnchor::from_excerpt("Chapter 03: Lin Xia appeared..."),
            validation_mode: ValidationMode::Novel,
            governance,
        }
    }

    /// The production caller shape: candidates are prepared inside the callback
    /// and persisted only after the orchestrator accepted the run.
    async fn persist_outcome(store: &InMemoryKbStore, outcome: ExtractCandidatesOutcome) {
        for candidate in outcome.candidates {
            persist_prepared_extract(store, candidate)
                .await
                .expect("persist prepared candidate");
        }
    }

    #[tokio::test]
    async fn v1191_extract_converts_prepared_candidates_and_keeps_the_sidecar() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let relationships = vec![json!({
            "source_canonical_name": "char_lin_xia",
            "target_canonical_name": "char_mo",
            "relation_type": "allied_with",
        })];
        let expected_relationships = relationships.clone();

        let outcome = extract_candidates(
            request(RUN_ID),
            admitted_bundle(RUN_ID),
            |run_input| async move {
                // The admitted bundle (and only it) reaches the callback.
                assert_eq!(run_input.input["chapters"][0]["path"], "03.md");
                let prepared =
                    prepare_extract(prepare_input("char_lin_xia", KnowledgeGovernance::shared()))
                        .expect("prepare");
                spoke_ok(NativeExtractionOutput {
                    candidates: vec![prepared],
                    method: Some("novel-chapter".to_string()),
                    coverage_hint: Some(json!({ "chapters": 1 })),
                    relationships,
                })
            },
        )
        .await;
        let outcome = match outcome {
            SpokeResult::Ok(outcome) => outcome,
            SpokeResult::Reject(reject) => panic!("expected success, got reject: {reject:?}"),
        };

        let ExtractResponse::Variant0 {
            candidates, run, ..
        } = &outcome.response
        else {
            panic!("expected the success variant");
        };
        assert_eq!(run.run_id.as_str(), RUN_ID, "run echo");
        assert_eq!(
            run.method.as_ref().map(|method| method.as_str()),
            Some("novel-chapter")
        );
        assert_eq!(run.coverage_hint, Some(json!({ "chapters": 1 })));
        // Identical candidates: same id, same order, none reconstructed.
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].entry_id,
            outcome.candidates[0].record.entry_id
        );
        // Relationship sidecar retained beside SPOKE's KE-only success arm.
        assert_eq!(outcome.relationships, expected_relationships);

        persist_outcome(&store, outcome).await;
        assert_eq!(store.list_by_world(WORLD_ID).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn v1191_extract_persists_the_same_prepared_ids_and_governance() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let policy = KnowledgeGovernance {
            holder_entry_id: Some("hld_t12".to_string()),
            disclosure: Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
        };

        let outcome = extract_candidates(request(RUN_ID), admitted_bundle(RUN_ID), {
            let policy = policy.clone();
            move |_| async move {
                spoke_ok(NativeExtractionOutput {
                    candidates: vec![prepare_extract(prepare_input(
                        "char_lin_xia",
                        policy.clone(),
                    ))
                    .expect("prepare")],
                    method: None,
                    coverage_hint: None,
                    relationships: Vec::new(),
                })
            }
        })
        .await;
        let outcome = match outcome {
            SpokeResult::Ok(outcome) => outcome,
            SpokeResult::Reject(reject) => panic!("expected success, got reject: {reject:?}"),
        };

        // The trusted policy reaches the wire unchanged (holder → owner).
        let ExtractResponse::Variant0 { candidates, .. } = &outcome.response else {
            panic!("expected the success variant");
        };
        assert_eq!(
            candidates[0].owner.as_ref().map(|owner| owner.as_str()),
            Some("hld_t12")
        );
        assert_eq!(
            candidates[0]
                .disclosure
                .as_ref()
                .map(|value| value.as_str()),
            Some(DISCLOSURE_OWNER_PRIVATE)
        );

        let prepared_id = outcome.candidates[0].record.entry_id.clone();
        persist_outcome(&store, outcome).await;

        let stored = store.get_knowledge_entry(&prepared_id).await.unwrap();
        assert_eq!(stored.entry_id, prepared_id, "no second id minted");
        assert_eq!(stored.holder_entry_id.as_deref(), Some("hld_t12"));
        assert_eq!(stored.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    }

    #[tokio::test]
    async fn v1191_extract_source_failure_invokes_no_callback_and_writes_nothing() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let calls = Arc::new(AtomicUsize::new(0));

        let outcome = extract_candidates(request(RUN_ID), admitted_bundle("xrun_other"), {
            let calls = Arc::clone(&calls);
            move |_| async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let prepared =
                    prepare_extract(prepare_input("char_lin_xia", KnowledgeGovernance::shared()))
                        .expect("prepare");
                spoke_ok(NativeExtractionOutput {
                    candidates: vec![prepared],
                    method: None,
                    coverage_hint: None,
                    relationships: Vec::new(),
                })
            }
        })
        .await;

        let reject = match outcome {
            SpokeResult::Reject(reject) => reject,
            SpokeResult::Ok(_) => panic!("a mismatched bundle must not produce a success"),
        };
        assert_eq!(reject.code, SpokeRejectCode::InvalidInput);
        assert_eq!(reject.details.as_ref().unwrap()["field"], "run_id");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "no callback on source failure"
        );
        assert!(
            store.list_by_world(WORLD_ID).await.unwrap().is_empty(),
            "source failure writes nothing"
        );
    }

    #[tokio::test]
    async fn v1191_extract_invalid_candidate_result_is_rejected_without_write() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let outcome =
            extract_candidates(request(RUN_ID), admitted_bundle(RUN_ID), |_| async move {
                let mut prepared =
                    prepare_extract(prepare_input("char_lin_xia", KnowledgeGovernance::shared()))
                        .expect("prepare");
                // A callback that rewrites the candidate's status must not be able
                // to hand back a success carrying a non-provisional entry.
                prepared.record.status = "confirmed".to_string();
                spoke_ok(NativeExtractionOutput {
                    candidates: vec![prepared],
                    method: None,
                    coverage_hint: None,
                    relationships: Vec::new(),
                })
            })
            .await;

        let reject = match outcome {
            SpokeResult::Reject(reject) => reject,
            SpokeResult::Ok(_) => panic!("a non-provisional candidate must be rejected"),
        };
        assert_eq!(reject.code, SpokeRejectCode::CandidateNotProvisional);
        assert!(store.list_by_world(WORLD_ID).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn v1191_extract_callback_failure_rejects_without_write() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let outcome =
            extract_candidates(request(RUN_ID), admitted_bundle(RUN_ID), |_| async move {
                spoke_reject::<NativeExtractionOutput>(
                    SpokeRejectCode::InternalError,
                    "extractor unavailable",
                    None,
                )
            })
            .await;

        let reject = match outcome {
            SpokeResult::Reject(reject) => reject,
            SpokeResult::Ok(_) => panic!("an extractor failure must not produce a success"),
        };
        assert_eq!(reject.code, SpokeRejectCode::InternalError);
        assert_eq!(reject.message, "extractor unavailable");
        assert!(store.list_by_world(WORLD_ID).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn v1191_extract_cancellation_before_commit_writes_nothing() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let started = Arc::new(AtomicUsize::new(0));

        // Cancellation is the caller dropping the run future; nothing here
        // spawns a task, so a dropped future stops before any persist.
        let cancelled = tokio::time::timeout(
            Duration::from_millis(20),
            extract_candidates(request(RUN_ID), admitted_bundle(RUN_ID), {
                let started = Arc::clone(&started);
                move |_| async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    std::future::pending::<SpokeResult<NativeExtractionOutput>>().await
                }
            }),
        )
        .await;

        assert!(cancelled.is_err(), "the pending run must be cancelled");
        assert_eq!(started.load(Ordering::SeqCst), 1, "callback had started");
        assert!(
            store.list_by_world(WORLD_ID).await.unwrap().is_empty(),
            "cancellation before commit leaves zero writes"
        );
    }

    /// v1.191 P1 T13: the wrapper's future must stay `Send`.
    ///
    /// Both production extraction callers are `Send`-bound `#[async_trait]`
    /// capabilities (`Capability: Send + Sync`) driven from spawned tasks, and
    /// upstream `orchestrate_extract` captures a `&dyn ExtractionPort` (no
    /// `Sync` bound) — so its future is `!Send`. This guard fails the moment the
    /// adapter stops driving the orchestrator on the driver thread, i.e. the
    /// moment a production caller would no longer be able to await it.
    #[test]
    fn v1191_extract_candidates_future_is_send() {
        fn assert_send<T: Send>(_: T) {}

        let request: ExtractRequest = serde_json::from_value(json!({
            "run_id": RUN_ID,
            "sources": [{ "schema_version": 1, "source_id": "c03.md", "extensions": {} }],
        }))
        .expect("valid ExtractRequest");
        assert_send(extract_candidates(
            request,
            admitted_bundle(RUN_ID),
            |_| async {
                SpokeResult::Ok(NativeExtractionOutput {
                    candidates: Vec::new(),
                    method: None,
                    coverage_hint: None,
                    relationships: Vec::new(),
                })
            },
        ));
    }
}
