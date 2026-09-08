//! Authoritative Character run capture from the server-owned Host exec drain.

use std::pin::Pin;

use futures_util::StreamExt;
use nexus_agent_host::capability::model::{
    FinishReason, HostEvent, OperationFinishedEvent, TextDeltaEvent,
};
use nexus_agent_host::{HostError, HostOperationId, HostSessionId};
use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResult, CharacterOperationResultFinishReason,
    CharacterOperationResultRunStatus, NexusCharacterRunCaptureOutcome,
    NexusCharacterRunCaptureOutcomeCode, NexusCharacterRunCaptureOutcomePendingId,
    NexusCharacterRunCaptureOutcomeStatus,
};
use nexus_contracts::generated::daemon_api::agent_host::operation_response::{
    NexusCharacterRunCaptureOutcome as OperationCaptureOutcome,
    NexusCharacterRunCaptureOutcomeStatus as OperationCaptureOutcomeStatus,
};
use nexus_local_db::{capture_character_run, ActorContractConflict, LocalDbError, RunCaptureInput};
use sqlx::SqlitePool;

use crate::actor_knowledge_view::AdmittedActor;
use crate::workspace::actor_sessions::{
    ActorSessionRegistry, CharacterActivityGuard, CharacterOperationSnapshot,
};

pub const MAX_CAPTURE_DIGEST_BYTES: usize = 65_536;

const CAPTURE_DIGEST_STATIC_OVERHEAD: usize = "Prompt:\n".len() + "\n\nResponse:\n".len();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestBuildError {
    EmptyResponse,
    TooLarge,
}

fn checked_capture_digest_byte_len(raw_prompt: &str, response_text: &str) -> Option<usize> {
    CAPTURE_DIGEST_STATIC_OVERHEAD
        .checked_add(raw_prompt.len())?
        .checked_add(response_text.len())
}

#[must_use]
pub fn capture_digest_byte_len(raw_prompt: &str, response_text: &str) -> usize {
    checked_capture_digest_byte_len(raw_prompt, response_text).unwrap_or(usize::MAX)
}

#[must_use]
pub const fn prompt_digest_prefix_bytes(raw_prompt: &str) -> usize {
    CAPTURE_DIGEST_STATIC_OVERHEAD.saturating_add(raw_prompt.len())
}

/// # Errors
///
/// Returns `DigestBuildError` when the response is empty or the digest exceeds the byte cap.
pub fn build_capture_digest(
    raw_prompt: &str,
    response_text: &str,
) -> Result<String, DigestBuildError> {
    if response_text.trim().is_empty() {
        return Err(DigestBuildError::EmptyResponse);
    }
    match checked_capture_digest_byte_len(raw_prompt, response_text) {
        None => Err(DigestBuildError::TooLarge),
        Some(len) if len > MAX_CAPTURE_DIGEST_BYTES => Err(DigestBuildError::TooLarge),
        Some(_) => Ok(format!(
            "Prompt:\n{raw_prompt}\n\nResponse:\n{response_text}"
        )),
    }
}

#[must_use]
pub fn run_pending_id(operation_id: &HostOperationId) -> String {
    format!("run_{}", operation_id.to_string().replace('-', ""))
}

#[must_use]
pub fn event_matches_operation(
    event: &HostEvent,
    session_id: &HostSessionId,
    op_id: &HostOperationId,
) -> bool {
    match event {
        HostEvent::OpStarted(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::OpFinished(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::OpFailed(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::ThoughtDelta(e) | HostEvent::MessageDelta(e) => {
            &e.session_id == session_id && &e.op_id == op_id
        }
        HostEvent::ToolCall(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::ToolCallUpdate(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::PlanUpdate(e) => &e.session_id == session_id && &e.op_id == op_id,
        HostEvent::SessionCreated(_) | HostEvent::SessionStopped(_) | HostEvent::Status(_) => false,
    }
}

#[derive(Debug)]
pub struct DrainAccumulator {
    pub message_text: String,
    pub digest_too_large: bool,
    prompt_prefix_bytes: usize,
    pub saw_terminal: bool,
    pub finish_reason: Option<FinishReason>,
    pub run_failed: bool,
}

impl DrainAccumulator {
    #[must_use]
    pub const fn for_prompt(raw_prompt: &str) -> Self {
        let prompt_prefix_bytes = prompt_digest_prefix_bytes(raw_prompt);
        Self {
            message_text: String::new(),
            digest_too_large: prompt_prefix_bytes >= MAX_CAPTURE_DIGEST_BYTES,
            prompt_prefix_bytes,
            saw_terminal: false,
            finish_reason: None,
            run_failed: false,
        }
    }

    pub fn apply_matching(&mut self, event: &HostEvent) {
        match event {
            HostEvent::MessageDelta(TextDeltaEvent { text, .. }) => {
                if self.digest_too_large {
                    return;
                }
                let Some(next_total) = self
                    .prompt_prefix_bytes
                    .checked_add(self.message_text.len())
                    .and_then(|n| n.checked_add(text.len()))
                else {
                    self.digest_too_large = true;
                    return;
                };
                if next_total > MAX_CAPTURE_DIGEST_BYTES {
                    self.digest_too_large = true;
                    return;
                }
                self.message_text.push_str(text);
            }
            HostEvent::OpFinished(OperationFinishedEvent { reason, .. }) => {
                self.saw_terminal = true;
                self.finish_reason = Some(reason.clone());
            }
            HostEvent::OpFailed(_) => {
                self.saw_terminal = true;
                self.run_failed = true;
            }
            _ => {}
        }
    }
}

impl Default for DrainAccumulator {
    fn default() -> Self {
        Self::for_prompt("")
    }
}

const fn map_finish_reason(reason: &FinishReason) -> CharacterOperationResultFinishReason {
    match reason {
        FinishReason::EndTurn => CharacterOperationResultFinishReason::EndTurn,
        FinishReason::MaxTokens => CharacterOperationResultFinishReason::MaxTokens,
        FinishReason::MaxTurnRequests => CharacterOperationResultFinishReason::MaxTurnRequests,
        FinishReason::Refusal => CharacterOperationResultFinishReason::Refusal,
        FinishReason::Cancelled => CharacterOperationResultFinishReason::Cancelled,
    }
}

const fn disabled_capture() -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
        pending_id: None,
        code: None,
    }
}

const fn skipped_capture(
    code: NexusCharacterRunCaptureOutcomeCode,
) -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Skipped,
        pending_id: None,
        code: Some(code),
    }
}

const fn failed_capture(
    code: NexusCharacterRunCaptureOutcomeCode,
) -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Failed,
        pending_id: None,
        code: Some(code),
    }
}

const fn pending_capture() -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Pending,
        pending_id: None,
        code: None,
    }
}

#[must_use]
pub const fn initial_capture_outcome(remember: bool) -> OperationCaptureOutcome {
    if remember {
        OperationCaptureOutcome {
            status: OperationCaptureOutcomeStatus::Pending,
            pending_id: None,
            code: None,
        }
    } else {
        OperationCaptureOutcome {
            status: OperationCaptureOutcomeStatus::Disabled,
            pending_id: None,
            code: None,
        }
    }
}

const fn run_status_from_terminal(
    acc: &DrainAccumulator,
    cancel_requested: bool,
) -> (
    CharacterOperationResultRunStatus,
    Option<CharacterOperationResultFinishReason>,
) {
    if cancel_requested && !acc.saw_terminal {
        return (CharacterOperationResultRunStatus::Cancelled, None);
    }
    if cancel_requested && acc.run_failed {
        return (CharacterOperationResultRunStatus::Cancelled, None);
    }
    if acc.run_failed {
        return (CharacterOperationResultRunStatus::Failed, None);
    }
    if !acc.saw_terminal {
        return (
            if cancel_requested {
                CharacterOperationResultRunStatus::Cancelled
            } else {
                CharacterOperationResultRunStatus::Failed
            },
            None,
        );
    }
    let reason = acc.finish_reason.as_ref().expect("terminal without reason");
    let finish = map_finish_reason(reason);
    let status = match reason {
        FinishReason::EndTurn => CharacterOperationResultRunStatus::Succeeded,
        FinishReason::MaxTokens
        | FinishReason::MaxTurnRequests
        | FinishReason::Refusal
        | FinishReason::Cancelled => CharacterOperationResultRunStatus::Incomplete,
    };
    if cancel_requested {
        return (CharacterOperationResultRunStatus::Cancelled, Some(finish));
    }
    (status, Some(finish))
}

async fn try_persist_capture(
    pool: &SqlitePool,
    snapshot: &CharacterOperationSnapshot,
    guard: &CharacterActivityGuard,
    raw_prompt: &str,
    response_text: &str,
) -> NexusCharacterRunCaptureOutcome {
    let AdmittedActor::Character { character_id } = &snapshot.ctx.actor else {
        return disabled_capture();
    };
    let Some(binding_id) = snapshot.ctx.binding_id.as_deref() else {
        return failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureScopeChanged);
    };
    let expected_epoch = snapshot
        .ctx
        .character_epoch
        .unwrap_or_else(|| guard.epoch());
    if guard.epoch() != expected_epoch {
        return failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureScopeChanged);
    }
    let digest = match build_capture_digest(raw_prompt, response_text) {
        Ok(d) => d,
        Err(DigestBuildError::EmptyResponse) => {
            return failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureEmptyOutput);
        }
        Err(DigestBuildError::TooLarge) => {
            return failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureTooLarge);
        }
    };
    let pending_id = run_pending_id(&snapshot.operation_id);
    let captured_at = chrono::Utc::now().to_rfc3339();
    let input = RunCaptureInput {
        operation_id: &snapshot.operation_id.to_string(),
        session_id: &snapshot.session_id.to_string(),
        character_id: character_id.as_str(),
        binding_id,
        pending_id: &pending_id,
        raw_digest: &digest,
        captured_at: &captured_at,
        lifecycle_epoch: guard.epoch(),
    };
    match capture_character_run(pool, &snapshot.owner_creator_id, input).await {
        Ok(_receipt) => NexusCharacterRunCaptureOutcome {
            status: NexusCharacterRunCaptureOutcomeStatus::Captured,
            pending_id: Some(
                NexusCharacterRunCaptureOutcomePendingId::try_from(pending_id)
                    .expect("generated pending id fits schema"),
            ),
            code: None,
        },
        Err(LocalDbError::ActorContractConflict {
            code: ActorContractConflict::RunCaptureScopeChanged,
        }) => failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureScopeChanged),
        Err(_) => failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureStoreFailed),
    }
}

fn finalize_capture_outcome(
    snapshot: &CharacterOperationSnapshot,
    acc: &DrainAccumulator,
    cancel_requested: bool,
    run_status: CharacterOperationResultRunStatus,
    persisted: Option<NexusCharacterRunCaptureOutcome>,
) -> NexusCharacterRunCaptureOutcome {
    if !snapshot.remember {
        return disabled_capture();
    }
    if let Some(outcome) = persisted {
        return outcome;
    }
    if cancel_requested {
        return skipped_capture(NexusCharacterRunCaptureOutcomeCode::RunCancelled);
    }
    match run_status {
        CharacterOperationResultRunStatus::Succeeded => {
            if acc.digest_too_large {
                return failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureTooLarge);
            }
            match build_capture_digest(&snapshot.raw_prompt, &acc.message_text) {
                Ok(_) => failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureStoreFailed),
                Err(DigestBuildError::EmptyResponse) => {
                    failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureEmptyOutput)
                }
                Err(DigestBuildError::TooLarge) => {
                    failed_capture(NexusCharacterRunCaptureOutcomeCode::CaptureTooLarge)
                }
            }
        }
        CharacterOperationResultRunStatus::Incomplete => {
            skipped_capture(NexusCharacterRunCaptureOutcomeCode::RunIncomplete)
        }
        CharacterOperationResultRunStatus::Failed => {
            skipped_capture(NexusCharacterRunCaptureOutcomeCode::RunFailed)
        }
        CharacterOperationResultRunStatus::Cancelled => {
            skipped_capture(NexusCharacterRunCaptureOutcomeCode::RunCancelled)
        }
        CharacterOperationResultRunStatus::Running => pending_capture(),
    }
}

#[cfg(test)]
type BeforeFinalizeHook =
    std::sync::Arc<dyn Fn(&ActorSessionRegistry, &HostOperationId) + Send + Sync>;

#[cfg(test)]
static BEFORE_OPERATION_FINALIZE_HOOK: std::sync::Mutex<Option<BeforeFinalizeHook>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn set_before_operation_finalize_hook(
    hook: impl Fn(&ActorSessionRegistry, &HostOperationId) + Send + Sync + 'static,
) {
    *BEFORE_OPERATION_FINALIZE_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(std::sync::Arc::new(hook));
}

#[cfg(test)]
pub(crate) fn clear_before_operation_finalize_hook() {
    *BEFORE_OPERATION_FINALIZE_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[allow(
    clippy::missing_const_for_fn,
    unused_variables,
    clippy::used_underscore_binding
)]
fn fire_before_operation_finalize_hook(
    _registry: &ActorSessionRegistry,
    _operation_id: &HostOperationId,
) {
    #[cfg(test)]
    {
        let hook = BEFORE_OPERATION_FINALIZE_HOOK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(hook) = hook {
            hook(_registry, _operation_id);
        }
    }
}

pub async fn drain_and_finalize_character_operation(
    pool: SqlitePool,
    registry: ActorSessionRegistry,
    mut stream: Pin<Box<dyn futures_util::Stream<Item = Result<HostEvent, HostError>> + Send>>,
    activity_guard: Option<CharacterActivityGuard>,
    snapshot: CharacterOperationSnapshot,
) {
    let op_id = snapshot.operation_id.clone();
    let mut acc = DrainAccumulator::for_prompt(&snapshot.raw_prompt);

    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                if !event_matches_operation(&event, &snapshot.session_id, &op_id) {
                    continue;
                }
                acc.apply_matching(&event);
                if acc.saw_terminal {
                    break;
                }
            }
            Err(_) => {
                acc.run_failed = true;
            }
        }
    }

    fire_before_operation_finalize_hook(&registry, &op_id);

    let cancel_requested = registry.begin_operation_finalizing(&op_id);
    let (run_status, finish_reason) = run_status_from_terminal(&acc, cancel_requested);

    let persisted = if snapshot.remember
        && !cancel_requested
        && run_status == CharacterOperationResultRunStatus::Succeeded
        && !acc.digest_too_large
    {
        if let Some(guard) = activity_guard.as_ref() {
            Some(
                try_persist_capture(
                    &pool,
                    &snapshot,
                    guard,
                    &snapshot.raw_prompt,
                    &acc.message_text,
                )
                .await,
            )
        } else {
            None
        }
    } else {
        None
    };

    let capture =
        finalize_capture_outcome(&snapshot, &acc, cancel_requested, run_status, persisted);

    let outcome = CharacterOperationResult {
        operation_id: op_id.to_string(),
        session_id: snapshot.session_id.to_string(),
        run_status,
        finish_reason,
        capture,
    };

    registry.commit_operation_terminal(&op_id, outcome);
    registry.clear_indexed_operation(&op_id);
    drop(activity_guard);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_knowledge_view::AdmittedActor;
    use crate::workspace::actor_sessions::{ActorSessionRegistry, CharacterOperationSnapshot};
    use nexus_agent_host::capability::model::{
        FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, TextDeltaEvent,
    };
    use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
        NexusCharacterRunCaptureOutcomeCode, NexusCharacterRunCaptureOutcomeStatus,
    };

    fn sample_snapshot(remember: bool, raw_prompt: &str) -> CharacterOperationSnapshot {
        let session_id = HostSessionId::new();
        let operation_id = HostOperationId::new();
        CharacterOperationSnapshot {
            owner_creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ctx: crate::actor_admission::AdmittedActorContext {
                actor: AdmittedActor::Character {
                    character_id: "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                },
                owner_creator_id: "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                world_id: "wld_worldA".into(),
                binding_id: Some("awb_cccccccccccccccccccccccccccccccc".into()),
                branch_id: None,
                event_id: None,
                character_epoch: Some(0),
                view: crate::actor_knowledge_view::ActorKnowledgePage {
                    items: Vec::new(),
                    limit: 50,
                    has_more: false,
                    next_cursor: None,
                },
            },
            session_id,
            operation_id,
            remember,
            raw_prompt: raw_prompt.into(),
        }
    }

    fn message_delta(snapshot: &CharacterOperationSnapshot, text: &str) -> HostEvent {
        HostEvent::MessageDelta(TextDeltaEvent {
            session_id: snapshot.session_id.clone(),
            op_id: snapshot.operation_id.clone(),
            text: text.into(),
        })
    }

    fn thought_delta(snapshot: &CharacterOperationSnapshot, text: &str) -> HostEvent {
        HostEvent::ThoughtDelta(TextDeltaEvent {
            session_id: snapshot.session_id.clone(),
            op_id: snapshot.operation_id.clone(),
            text: text.into(),
        })
    }

    fn op_finished(snapshot: &CharacterOperationSnapshot, reason: FinishReason) -> HostEvent {
        HostEvent::OpFinished(OperationFinishedEvent {
            session_id: snapshot.session_id.clone(),
            op_id: snapshot.operation_id.clone(),
            reason,
        })
    }

    fn foreign_message(snapshot: &CharacterOperationSnapshot, text: &str) -> HostEvent {
        HostEvent::MessageDelta(TextDeltaEvent {
            session_id: HostSessionId::new(),
            op_id: snapshot.operation_id.clone(),
            text: text.into(),
        })
    }

    fn stream_of(
        events: Vec<HostEvent>,
    ) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<HostEvent, HostError>> + Send>>
    {
        Box::pin(futures_util::stream::iter(events.into_iter().map(Ok)))
    }

    #[test]
    fn digest_rejects_empty_and_oversize() {
        assert_eq!(
            build_capture_digest("hi", "   "),
            Err(DigestBuildError::EmptyResponse)
        );
        let big = "x".repeat(MAX_CAPTURE_DIGEST_BYTES);
        assert!(build_capture_digest("p", &big).is_err());
        let ok = build_capture_digest("prompt", "visible");
        assert!(ok.is_ok());
        assert!(ok.unwrap().starts_with("Prompt:\n"));
    }

    #[test]
    fn digest_counts_utf8_bytes_not_chars() {
        let prompt = "é";
        let response = "🙂".repeat(20_000);
        let digest = format!("Prompt:\n{prompt}\n\nResponse:\n{response}");
        assert!(digest.len() > MAX_CAPTURE_DIGEST_BYTES);
        assert_eq!(
            build_capture_digest(prompt, &response),
            Err(DigestBuildError::TooLarge)
        );
    }

    #[test]
    fn event_matching_ignores_foreign_session() {
        let snapshot = sample_snapshot(true, "p");
        let foreign = foreign_message(&snapshot, "nope");
        assert!(!event_matches_operation(
            &foreign,
            &snapshot.session_id,
            &snapshot.operation_id
        ));
        let local = message_delta(&snapshot, "yes");
        assert!(event_matches_operation(
            &local,
            &snapshot.session_id,
            &snapshot.operation_id
        ));
    }

    #[test]
    fn accumulator_ignores_thought_and_tools() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator::for_prompt("p");
        acc.apply_matching(&thought_delta(&snapshot, "secret"));
        acc.apply_matching(&message_delta(&snapshot, "visible"));
        assert_eq!(acc.message_text, "visible");
    }

    #[test]
    fn accumulator_flags_oversize_without_truncating() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator::for_prompt("p");
        let overhead = prompt_digest_prefix_bytes("p");
        let chunk = "a".repeat(MAX_CAPTURE_DIGEST_BYTES - overhead);
        acc.apply_matching(&message_delta(&snapshot, &chunk));
        acc.apply_matching(&message_delta(&snapshot, "x"));
        assert!(acc.digest_too_large);
        assert_eq!(acc.message_text.len(), MAX_CAPTURE_DIGEST_BYTES - overhead);
    }

    #[test]
    fn oversized_single_delta_rejects_before_append() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator::for_prompt("p");
        let huge = "x".repeat(MAX_CAPTURE_DIGEST_BYTES * 2);
        acc.apply_matching(&message_delta(&snapshot, &huge));
        assert!(acc.digest_too_large);
        assert!(
            acc.message_text.is_empty(),
            "must reject before copying delta"
        );
    }

    #[test]
    fn oversized_prompt_bounded_at_capture_entry() {
        let huge_prompt = "p".repeat(MAX_CAPTURE_DIGEST_BYTES);
        let acc = DrainAccumulator::for_prompt(&huge_prompt);
        assert!(acc.digest_too_large);
        assert!(acc.message_text.is_empty());
    }

    #[tokio::test]
    async fn oversized_prompt_production_drain_classifies_at_entry() {
        let huge_prompt = "p".repeat(MAX_CAPTURE_DIGEST_BYTES);
        let snapshot = sample_snapshot(true, &huge_prompt);
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "visible"),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Succeeded
        );
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Failed
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::CaptureTooLarge)
        );
    }

    #[tokio::test]
    async fn oversized_single_delta_preserves_run_success() {
        let snapshot = sample_snapshot(true, "p");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        let huge = "y".repeat(MAX_CAPTURE_DIGEST_BYTES * 2);
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, &huge),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Succeeded
        );
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Failed
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::CaptureTooLarge)
        );
    }

    #[tokio::test]
    async fn opt_out_yields_disabled_capture() {
        let snapshot = sample_snapshot(false, "hello");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        let events = vec![
            message_delta(&snapshot, "answer"),
            op_finished(&snapshot, FinishReason::EndTurn),
        ];
        let op_id = snapshot.operation_id.clone();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(events),
            None,
            snapshot,
        )
        .await;
        let result = registry
            .character_operation_result("ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &op_id)
            .expect("outcome");
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Disabled
        );
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn end_turn_without_remember_is_disabled() {
        let snapshot = sample_snapshot(false, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "ok"),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Disabled
        );
    }

    #[tokio::test]
    async fn max_tokens_marks_incomplete_and_skips_capture() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "partial"),
                op_finished(&snapshot, FinishReason::MaxTokens),
            ]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Incomplete
        );
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Skipped
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunIncomplete)
        );
    }

    #[tokio::test]
    async fn refusal_and_failure_skip_capture() {
        for (reason, status) in [(
            FinishReason::Refusal,
            CharacterOperationResultRunStatus::Incomplete,
        )] {
            let snapshot = sample_snapshot(true, "q");
            let registry = ActorSessionRegistry::new();
            registry
                .reserve_character_operation(snapshot.clone())
                .unwrap();
            drain_and_finalize_character_operation(
                sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
                registry.clone(),
                stream_of(vec![
                    message_delta(&snapshot, "no"),
                    op_finished(&snapshot, reason),
                ]),
                None,
                snapshot.clone(),
            )
            .await;
            let result = registry
                .character_operation_result(
                    "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    &snapshot.operation_id,
                )
                .expect("outcome");
            assert_eq!(result.run_status, status);
            assert_eq!(
                result.capture.status,
                NexusCharacterRunCaptureOutcomeStatus::Skipped
            );
        }

        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![HostEvent::OpFailed(OperationFailedEvent {
                session_id: snapshot.session_id.clone(),
                op_id: snapshot.operation_id.clone(),
                error_category: "internal".into(),
                error_message: "boom".into(),
            })]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Failed);
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunFailed)
        );
    }

    #[tokio::test]
    async fn eof_without_terminal_fails_run_and_skips_capture() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![message_delta(&snapshot, "orphan")]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Failed);
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Skipped
        );
    }

    #[tokio::test]
    async fn cancel_suppresses_capture_even_on_end_turn() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        registry
            .request_operation_cancel(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "late"),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Cancelled
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunCancelled)
        );
    }

    #[test]
    fn run_pending_id_format() {
        let op = HostOperationId::new();
        let pending = run_pending_id(&op);
        assert!(pending.starts_with("run_"));
        assert!(!pending.contains('-'));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_latched_at_finalize_boundary_suppresses_capture() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        clear_before_operation_finalize_hook();

        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        let owner = snapshot.owner_creator_id.clone();
        let op_id = snapshot.operation_id.clone();

        let at_boundary = Arc::new(Barrier::new(2));
        let cancel_done = Arc::new(Barrier::new(2));
        let reg_for_cancel = registry.clone();
        let owner_for_cancel = owner.clone();
        let op_for_cancel = op_id.clone();
        let at_boundary_cancel = Arc::clone(&at_boundary);
        let cancel_done_cancel = Arc::clone(&cancel_done);

        let cancel_thread = thread::spawn(move || {
            at_boundary_cancel.wait();
            reg_for_cancel
                .request_operation_cancel(&owner_for_cancel, &op_for_cancel)
                .expect("cancel latch");
            cancel_done_cancel.wait();
        });

        set_before_operation_finalize_hook({
            let at_boundary_drain = Arc::clone(&at_boundary);
            let cancel_done_drain = Arc::clone(&cancel_done);
            let guard_op = op_id.clone();
            move |_reg, op| {
                // The finalize hook is global; without this guard a concurrently
                // running plain drain test (which also fires the hook during its own
                // drain_and_finalize_character_operation call) would deadlock on these
                // barriers. Only block for this test's own operation — every test uses a
                // fresh HostOperationId, so this is deterministic and isolated.
                if op != &guard_op {
                    return;
                }
                at_boundary_drain.wait();
                cancel_done_drain.wait();
            }
        });

        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "late"),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            None,
            snapshot,
        )
        .await;
        cancel_thread.join().expect("cancel thread");

        clear_before_operation_finalize_hook();

        let result = registry
            .character_operation_result(&owner, &op_id)
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Cancelled
        );
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Skipped
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunCancelled)
        );
    }

    #[tokio::test]
    async fn op_failed_with_accepted_cancel_yields_cancelled_not_failed() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        registry
            .request_operation_cancel(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .unwrap();
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream_of(vec![HostEvent::OpFailed(OperationFailedEvent {
                session_id: snapshot.session_id.clone(),
                op_id: snapshot.operation_id.clone(),
                error_category: "internal".into(),
                error_message: "boom".into(),
            })]),
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Cancelled
        );
        assert_ne!(result.run_status, CharacterOperationResultRunStatus::Failed);
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunCancelled)
        );
    }

    #[test]
    fn digest_exact_boundary_accepted() {
        let prompt = "boundary";
        let overhead = prompt_digest_prefix_bytes(prompt);
        let response = "z".repeat(MAX_CAPTURE_DIGEST_BYTES - overhead);
        assert_eq!(
            capture_digest_byte_len(prompt, &response),
            MAX_CAPTURE_DIGEST_BYTES
        );
        assert!(build_capture_digest(prompt, &response).is_ok());
    }

    #[test]
    fn accumulator_checked_add_overflow_flags_too_large() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator {
            message_text: String::new(),
            digest_too_large: false,
            prompt_prefix_bytes: usize::MAX - 1,
            saw_terminal: false,
            finish_reason: None,
            run_failed: false,
        };
        acc.apply_matching(&message_delta(&snapshot, "xx"));
        assert!(acc.digest_too_large);
        assert!(acc.message_text.is_empty());
    }

    #[tokio::test]
    async fn host_error_stream_drains_to_terminal_finalization() {
        use nexus_agent_host::ProviderId;

        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        registry
            .register_indexed_operation(snapshot.operation_id.clone(), snapshot.session_id.clone());
        let items = vec![
            Err(nexus_agent_host::HostError::ProviderUnavailable {
                provider_id: ProviderId::new("mock"),
                message: "fault".into(),
            }),
            Ok(message_delta(&snapshot, "late")),
            Ok(op_finished(&snapshot, FinishReason::EndTurn)),
        ];
        let stream: std::pin::Pin<
            Box<dyn futures_util::Stream<Item = Result<HostEvent, HostError>> + Send>,
        > = Box::pin(futures_util::stream::iter(items));
        drain_and_finalize_character_operation(
            sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap(),
            registry.clone(),
            stream,
            None,
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &snapshot.operation_id,
            )
            .expect("terminal outcome committed once");
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Failed);
        assert!(registry
            .resolve_indexed_operation_session(&snapshot.operation_id)
            .is_none());
    }

    #[tokio::test]
    async fn missing_binding_fails_capture_without_panic() {
        let (_tmp, home, db_path) = crate::test_utils::create_test_workspace().await;
        let state = crate::workspace::WorkspaceState::new_for_testing(home, db_path, None).await;
        let pool = state.pool().unwrap().clone();
        let owner = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        nexus_local_db::ensure_creator_row(&pool, owner, "Owner")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds              (world_id, workspace_id, owner_creator_id, title, slug, status, visibility,               time_policy, metadata_json, created_at)              VALUES ('wld_worldA', 'ws', ?, 'WorldA', 'worldA', 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            &pool,
            nexus_local_db::CreateCharacterParams {
                owner_creator_id: owner,
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: "wld_worldA",
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        let registry = state.actor_sessions();
        let guard = registry
            .admit_character_activity(&pool, owner, &created.character.character_id)
            .await
            .expect("admit");
        let guard_epoch = guard.epoch();
        drop(guard);
        let mut snapshot = sample_snapshot(true, "q");
        snapshot.owner_creator_id = owner.to_string();
        snapshot.ctx.owner_creator_id = owner.to_string();
        snapshot.ctx.actor = AdmittedActor::Character {
            character_id: created.character.character_id.clone(),
        };
        snapshot.ctx.world_id = "wld_worldA".into();
        snapshot.ctx.binding_id = None;
        snapshot.ctx.character_epoch = Some(guard_epoch);
        registry
            .reserve_character_operation(snapshot.clone())
            .unwrap();
        let guard = registry
            .admit_character_activity(&pool, owner, &created.character.character_id)
            .await
            .expect("readmit");
        drain_and_finalize_character_operation(
            pool,
            registry.clone(),
            stream_of(vec![
                message_delta(&snapshot, "visible"),
                op_finished(&snapshot, FinishReason::EndTurn),
            ]),
            Some(guard),
            snapshot.clone(),
        )
        .await;
        let result = registry
            .character_operation_result(owner, &snapshot.operation_id)
            .expect("outcome");
        assert_eq!(
            result.run_status,
            CharacterOperationResultRunStatus::Succeeded
        );
        assert_eq!(
            result.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Failed
        );
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::CaptureScopeChanged)
        );
    }

    #[test]
    fn initial_capture_outcome_states() {
        assert_eq!(
            initial_capture_outcome(false).status,
            OperationCaptureOutcomeStatus::Disabled
        );
        assert_eq!(
            initial_capture_outcome(true).status,
            OperationCaptureOutcomeStatus::Pending
        );
    }
}
