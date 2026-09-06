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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestBuildError {
    EmptyResponse,
    TooLarge,
}

pub fn build_capture_digest(raw_prompt: &str, response_text: &str) -> Result<String, DigestBuildError> {
    if response_text.trim().is_empty() {
        return Err(DigestBuildError::EmptyResponse);
    }
    let digest = format!("Prompt:\n{raw_prompt}\n\nResponse:\n{response_text}");
    if digest.len() > MAX_CAPTURE_DIGEST_BYTES {
        return Err(DigestBuildError::TooLarge);
    }
    Ok(digest)
}

pub fn run_pending_id(operation_id: &HostOperationId) -> String {
    format!("run_{}", operation_id.to_string().replace('-', ""))
}

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

#[derive(Debug, Default)]
pub struct DrainAccumulator {
    pub message_text: String,
    pub digest_too_large: bool,
    pub saw_terminal: bool,
    pub finish_reason: Option<FinishReason>,
    pub run_failed: bool,
}

impl DrainAccumulator {
    pub fn apply_matching(&mut self, event: &HostEvent, raw_prompt: &str) {
        match event {
            HostEvent::MessageDelta(TextDeltaEvent { text, .. }) => {
                if self.digest_too_large {
                    return;
                }
                let candidate = format!(
                    "Prompt:\n{raw_prompt}\n\nResponse:\n{}{}",
                    self.message_text,
                    text
                );
                if candidate.len() > MAX_CAPTURE_DIGEST_BYTES {
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

fn map_finish_reason(reason: &FinishReason) -> CharacterOperationResultFinishReason {
    match reason {
        FinishReason::EndTurn => CharacterOperationResultFinishReason::EndTurn,
        FinishReason::MaxTokens => CharacterOperationResultFinishReason::MaxTokens,
        FinishReason::MaxTurnRequests => CharacterOperationResultFinishReason::MaxTurnRequests,
        FinishReason::Refusal => CharacterOperationResultFinishReason::Refusal,
    }
}

fn disabled_capture() -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Disabled,
        pending_id: None,
        code: None,
    }
}

fn skipped_capture(code: NexusCharacterRunCaptureOutcomeCode) -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Skipped,
        pending_id: None,
        code: Some(code),
    }
}

fn failed_capture(code: NexusCharacterRunCaptureOutcomeCode) -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Failed,
        pending_id: None,
        code: Some(code),
    }
}

fn pending_capture() -> NexusCharacterRunCaptureOutcome {
    NexusCharacterRunCaptureOutcome {
        status: NexusCharacterRunCaptureOutcomeStatus::Pending,
        pending_id: None,
        code: None,
    }
}

pub fn initial_capture_outcome(remember: bool) -> OperationCaptureOutcome {
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

fn run_status_from_terminal(
    acc: &DrainAccumulator,
    cancel_requested: bool,
) -> (CharacterOperationResultRunStatus, Option<CharacterOperationResultFinishReason>) {
    if cancel_requested && !acc.saw_terminal {
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
        FinishReason::MaxTokens | FinishReason::MaxTurnRequests | FinishReason::Refusal => {
            CharacterOperationResultRunStatus::Incomplete
        }
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
    let binding_id = snapshot
        .ctx
        .binding_id
        .as_deref()
        .expect("character capture requires binding");
    if guard.epoch() != snapshot.ctx.character_epoch.unwrap_or(guard.epoch()) {
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

pub async fn drain_and_finalize_character_operation(
    pool: SqlitePool,
    registry: ActorSessionRegistry,
    mut stream: Pin<Box<dyn futures_util::Stream<Item = Result<HostEvent, HostError>> + Send>>,
    activity_guard: Option<CharacterActivityGuard>,
    snapshot: CharacterOperationSnapshot,
) {
    let op_id = snapshot.operation_id.clone();
    let raw_prompt = snapshot.raw_prompt.clone();
    let mut acc = DrainAccumulator::default();

    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                if !event_matches_operation(&event, &snapshot.session_id, &op_id) {
                    continue;
                }
                acc.apply_matching(&event, &raw_prompt);
                if acc.saw_terminal {
                    break;
                }
            }
            Err(_) => {
                acc.run_failed = true;
                break;
            }
        }
    }

    let cancel_requested = registry.operation_cancel_requested(&op_id);
    let (run_status, finish_reason) = run_status_from_terminal(&acc, cancel_requested);

    let _ = registry.begin_operation_finalizing(&op_id);

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

    let capture = finalize_capture_outcome(
        &snapshot,
        &acc,
        cancel_requested,
        run_status,
        persisted,
    );

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
    use nexus_agent_host::capability::model::{
        FinishReason, HostEvent, OperationFailedEvent, OperationFinishedEvent, TextDeltaEvent,
    };
    use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
        NexusCharacterRunCaptureOutcomeCode, NexusCharacterRunCaptureOutcomeStatus,
    };
    use crate::actor_knowledge_view::AdmittedActor;
    use crate::workspace::actor_sessions::{ActorSessionRegistry, CharacterOperationSnapshot};

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

    fn stream_of(events: Vec<HostEvent>) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<HostEvent, HostError>> + Send>> {
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
        assert!(!event_matches_operation(&foreign, &snapshot.session_id, &snapshot.operation_id));
        let local = message_delta(&snapshot, "yes");
        assert!(event_matches_operation(&local, &snapshot.session_id, &snapshot.operation_id));
    }

    #[test]
    fn accumulator_ignores_thought_and_tools() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator::default();
        acc.apply_matching(&thought_delta(&snapshot, "secret"), "p");
        acc.apply_matching(&message_delta(&snapshot, "visible"), "p");
        assert_eq!(acc.message_text, "visible");
    }

    #[test]
    fn accumulator_flags_oversize_without_truncating() {
        let snapshot = sample_snapshot(true, "p");
        let mut acc = DrainAccumulator::default();
        let overhead = format!("Prompt:\n{}\n\nResponse:\n", "p").len();
        let chunk = "a".repeat(MAX_CAPTURE_DIGEST_BYTES - overhead);
        acc.apply_matching(&message_delta(&snapshot, &chunk), "p");
        acc.apply_matching(&message_delta(&snapshot, "x"), "p");
        assert!(acc.digest_too_large);
        assert_eq!(acc.message_text.len(), MAX_CAPTURE_DIGEST_BYTES - overhead);
    }

    #[tokio::test]
    async fn opt_out_yields_disabled_capture() {
        let snapshot = sample_snapshot(false, "hello");
        let registry = ActorSessionRegistry::new();
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
            .character_operation_result(
                "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &op_id,
            )
            .expect("outcome");
        assert_eq!(result.capture.status, NexusCharacterRunCaptureOutcomeStatus::Disabled);
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Succeeded);
    }

    #[tokio::test]
    async fn end_turn_without_remember_is_disabled() {
        let snapshot = sample_snapshot(false, "q");
        let registry = ActorSessionRegistry::new();
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
        assert_eq!(result.capture.status, NexusCharacterRunCaptureOutcomeStatus::Disabled);
    }

    #[tokio::test]
    async fn max_tokens_marks_incomplete_and_skips_capture() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Incomplete);
        assert_eq!(result.capture.status, NexusCharacterRunCaptureOutcomeStatus::Skipped);
        assert_eq!(
            result.capture.code,
            Some(NexusCharacterRunCaptureOutcomeCode::RunIncomplete)
        );
    }

    #[tokio::test]
    async fn refusal_and_failure_skip_capture() {
        for (reason, status) in [
            (FinishReason::Refusal, CharacterOperationResultRunStatus::Incomplete),
        ] {
            let snapshot = sample_snapshot(true, "q");
            let registry = ActorSessionRegistry::new();
            registry.reserve_character_operation(snapshot.clone()).unwrap();
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
            assert_eq!(result.capture.status, NexusCharacterRunCaptureOutcomeStatus::Skipped);
        }

        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
        assert_eq!(result.capture.status, NexusCharacterRunCaptureOutcomeStatus::Skipped);
    }

    #[tokio::test]
    async fn cancel_suppresses_capture_even_on_end_turn() {
        let snapshot = sample_snapshot(true, "q");
        let registry = ActorSessionRegistry::new();
        registry.reserve_character_operation(snapshot.clone()).unwrap();
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
        assert_eq!(result.run_status, CharacterOperationResultRunStatus::Cancelled);
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
