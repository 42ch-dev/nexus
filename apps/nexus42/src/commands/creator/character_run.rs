//! Character `run` observation — concurrent SSE drain and owner outcome polling (v1.185 P3).

use crate::api::DaemonClient;
use crate::errors::{CliError, Result};
use nexus_contracts::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResult, CharacterOperationResultRunStatus,
    NexusCharacterRunCaptureOutcomeStatus,
};
use nexus_contracts::daemon_api::agent_host::{
    execute_operation_request::ExecuteOperationRequest, operation_response::OperationResponse,
    session_response::SessionResponse,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;

const OUTCOME_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DEFAULT_OBSERVATION_GRACE: Duration = Duration::from_secs(30);

fn observation_grace() -> Duration {
    std::env::var("NEXUS42_RUN_OBSERVATION_GRACE_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_OBSERVATION_GRACE)
}

#[derive(Debug, Clone)]
pub struct StreamObservation {
    pub result: String,
    pub events: Vec<serde_json::Value>,
    pub op_failed: bool,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ObservationNotes {
    pub output_incomplete: bool,
    pub outcome_unavailable: bool,
}

#[derive(Debug)]
pub struct RunDelivery {
    pub session: SessionResponse,
    pub operation: OperationResponse,
    pub stream: StreamObservation,
    pub outcome: Option<CharacterOperationResult>,
    pub notes: ObservationNotes,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_character_with_observation(
    client: &DaemonClient,
    character_id: String,
    world_id: String,
    binding_id: String,
    prompt: String,
    provider_id: String,
    cwd: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    branch_id: Option<String>,
    event_id: Option<String>,
    remember: bool,
    json: bool,
) -> Result<()> {
    let mut body = serde_json::json!({
        "provider_id": provider_id,
        "actor_ref": {
            "actor_kind": "character",
            "character_id": character_id,
        },
        "viewpoint": {
            "world_id": world_id,
            "binding_id": binding_id,
        },
    });
    if let Some(cwd) = cwd {
        body["cwd"] = serde_json::Value::String(cwd);
    }
    if let Some(model) = model {
        body["model"] = serde_json::Value::String(model);
    }
    if let Some(mode) = mode {
        body["mode"] = serde_json::Value::String(mode);
    }
    if let Some(branch_id) = branch_id {
        body["viewpoint"]["branch_id"] = serde_json::Value::String(branch_id);
    }
    if let Some(event_id) = event_id {
        body["viewpoint"]["event_id"] = serde_json::Value::String(event_id);
    }
    let req: nexus_contracts::daemon_api::agent_host::create_session_request::CreateSessionRequest =
        serde_json::from_value(body)?;
    let session: SessionResponse = client.post("/v1/daemon/agent-host/sessions", &req).await?;

    let mut events = client
        .stream_get(&format!(
            "/v1/daemon/agent-host/sessions/{}/events",
            session.session_id
        ))
        .await?;

    let remember_opt = if remember { Some(true) } else { None };
    let op_req = ExecuteOperationRequest::Prompt {
        content: prompt,
        remember: remember_opt,
    };
    let operation: OperationResponse = client
        .post(
            &format!(
                "/v1/daemon/agent-host/sessions/{}/operations",
                session.session_id
            ),
            &op_req,
        )
        .await?;

    let delivery =
        observe_run_concurrently(client, &mut events, &session, &operation).await?;

    if json {
        let mut payload = serde_json::json!({
            "session": &delivery.session,
            "operation": &delivery.operation,
            "result": delivery.stream.result,
            "events": delivery.stream.events,
        });
        if let Some(outcome) = &delivery.outcome {
            payload["outcome"] = serde_json::to_value(outcome)?;
        }
        if delivery.notes.output_incomplete {
            payload["output_observation"] = serde_json::json!("incomplete");
        }
        if delivery.notes.outcome_unavailable {
            payload["capture_outcome_observation"] = serde_json::json!("unavailable");
        }
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_run_human(&delivery);
    }

    let (code, message) = classify_run_exit(remember, &delivery);
    if code == 0 {
        Ok(())
    } else {
        Err(CliError::CharacterRunExit { code, message })
    }
}

fn print_run_human(delivery: &RunDelivery) {
    let session = &delivery.session;
    println!("session_id:   {}", session.session_id);
    println!("provider_id:  {}", session.provider_id);
    if let Some(actor) = session.actor_ref.as_ref() {
        println!(
            "actor_ref:    {}",
            serde_json::to_string(actor).unwrap_or_default()
        );
    }
    if let Some(viewpoint) = session.viewpoint.as_ref() {
        println!(
            "viewpoint:    {}",
            serde_json::to_string(viewpoint).unwrap_or_default()
        );
    }
    println!("operation_id: {}", delivery.operation.operation_id);
    if let Some(outcome) = &delivery.outcome {
        println!("run_status:   {}", outcome.run_status);
        println!("capture_status: {}", outcome.capture.status);
        if let Some(pid) = &outcome.capture.pending_id {
            println!("capture_pending_id: {}", pid.as_str());
        }
        if let Some(code) = &outcome.capture.code {
            println!("capture_code: {}", code);
        }
    } else if delivery.notes.outcome_unavailable {
        println!("capture_outcome: unavailable");
    }
    if delivery.notes.output_incomplete {
        println!("output_observation: incomplete");
    }
    println!("result:");
    println!("{}", delivery.stream.result);
}

fn classify_run_exit(remember: bool, delivery: &RunDelivery) -> (i32, String) {
    if delivery.stream.op_failed {
        return (1, "agent-host operation failed".to_string());
    }

    let run_status = delivery
        .outcome
        .as_ref()
        .map(|o| o.run_status)
        .or_else(|| infer_run_status_from_stream(delivery));

    let capture = delivery.outcome.as_ref().map(|o| &o.capture);

    if delivery.notes.output_incomplete && delivery.outcome.is_none() {
        return (
            1,
            "output observation incomplete; capture outcome unavailable".to_string(),
        );
    }
    if delivery.notes.outcome_unavailable && remember {
        let msg = if delivery.notes.output_incomplete {
            "output observation incomplete; capture outcome unavailable".to_string()
        } else {
            "capture outcome unavailable".to_string()
        };
        if run_status == Some(CharacterOperationResultRunStatus::Succeeded) {
            return (1, msg);
        }
        if run_status.is_none()
            && !delivery.stream.incomplete
            && !delivery.stream.result.is_empty()
        {
            return (1, msg);
        }
    }

    match run_status {
        Some(CharacterOperationResultRunStatus::Succeeded) => {
            if !remember {
                return (0, String::new());
            }
            match capture.map(|c| c.status) {
                Some(NexusCharacterRunCaptureOutcomeStatus::Captured) => (0, String::new()),
                Some(NexusCharacterRunCaptureOutcomeStatus::Disabled) => (0, String::new()),
                Some(
                    NexusCharacterRunCaptureOutcomeStatus::Skipped
                    | NexusCharacterRunCaptureOutcomeStatus::Failed,
                ) => (
                    1,
                    format!(
                        "run succeeded but capture {}",
                        capture
                            .map(|c| c.status.to_string())
                            .unwrap_or_else(|| "failed".into())
                    ),
                ),
                Some(NexusCharacterRunCaptureOutcomeStatus::Pending) => (
                    1,
                    "capture still pending after terminal observation".to_string(),
                ),
                None => (1, "capture outcome unavailable".to_string()),
            }
        }
        Some(
            CharacterOperationResultRunStatus::Incomplete
            | CharacterOperationResultRunStatus::Failed
            | CharacterOperationResultRunStatus::Cancelled,
        ) => (1, format!("run {}", run_status.unwrap())),
        Some(CharacterOperationResultRunStatus::Running) => (
            1,
            "operation still running after observation window".to_string(),
        ),
        None => {
            if delivery.stream.incomplete {
                (1, "output observation incomplete".to_string())
            } else if remember {
                (1, "capture outcome unavailable".to_string())
            } else {
                (0, String::new())
            }
        }
    }
}

fn infer_run_status_from_stream(
    delivery: &RunDelivery,
) -> Option<CharacterOperationResultRunStatus> {
    if delivery.stream.incomplete {
        return Some(CharacterOperationResultRunStatus::Incomplete);
    }
    if delivery.stream.op_failed {
        return Some(CharacterOperationResultRunStatus::Failed);
    }
    if delivery
        .stream
        .events
        .iter()
        .any(|e| event_matches_terminal(e, true))
    {
        return Some(CharacterOperationResultRunStatus::Succeeded);
    }
    None
}

fn sse_event_matches(
    value: &serde_json::Value,
    expected_session_id: &str,
    expected_operation_id: &str,
) -> bool {
    const VARIANTS: [&str; 8] = [
        "OpStarted",
        "OpFinished",
        "OpFailed",
        "MessageDelta",
        "ThoughtDelta",
        "ToolCall",
        "ToolCallUpdate",
        "PlanUpdate",
    ];
    for key in VARIANTS {
        if let Some(inner) = value.get(key) {
            let sid = inner.get("session_id").and_then(serde_json::Value::as_str);
            let oid = inner.get("op_id").and_then(serde_json::Value::as_str);
            return sid == Some(expected_session_id) && oid == Some(expected_operation_id);
        }
    }
    false
}

fn event_matches_terminal(value: &serde_json::Value, finished: bool) -> bool {
    if finished {
        value.get("OpFinished").is_some()
    } else {
        value.get("OpFailed").is_some()
    }
}

async fn observe_run_concurrently(
    client: &DaemonClient,
    events: &mut reqwest::Response,
    session: &SessionResponse,
    operation: &OperationResponse,
) -> Result<RunDelivery> {
    let session_id = session.session_id.clone();
    let operation_id = operation.operation_id.clone();
    let outcome_path = format!("/v1/daemon/agent-host/operations/{}", operation.operation_id);
    let client = client.clone();
    let grace_deadline = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));

    let stream_fut = consume_terminal_events(events, &session_id, &operation_id, grace_deadline.clone());
    let outcome_fut = async move {
        let mut outcome_poll_failed = false;
        loop {
            if let Ok(slot) = grace_deadline.lock() {
                if slot.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Ok::<(Option<CharacterOperationResult>, bool), CliError>((None, outcome_poll_failed));
                }
            }
            match client.get_character_operation_result(&outcome_path).await {
                Ok(Some(result)) => {
                    if result.run_status != CharacterOperationResultRunStatus::Running {
                        if let Ok(mut slot) = grace_deadline.lock() {
                            if slot.is_none() {
                                *slot = Some(Instant::now() + observation_grace());
                            }
                        }
                        return Ok((Some(result), outcome_poll_failed));
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    outcome_poll_failed = true;
                }
            }
            sleep(OUTCOME_POLL_INTERVAL).await;
        }
    };

    let (stream, outcome) = tokio::join!(stream_fut, outcome_fut);
    let stream = stream?;
    let (outcome, outcome_poll_failed) = outcome?;

    let mut notes = ObservationNotes::default();
    if stream.incomplete {
        notes.output_incomplete = true;
    }
    if outcome.is_none() || outcome_poll_failed {
        notes.outcome_unavailable = true;
    }

    Ok(RunDelivery {
        session: session.clone(),
        operation: operation.clone(),
        stream,
        outcome,
        notes,
    })
}

pub(crate) async fn consume_terminal_events(
    resp: &mut reqwest::Response,
    expected_session_id: &str,
    expected_operation_id: &str,
    grace_deadline: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
) -> Result<StreamObservation> {
    let mut buf = String::new();
    let mut result = String::new();
    let mut events = Vec::new();


    let start_grace = || {
        if let Ok(mut slot) = grace_deadline.lock() {
            if slot.is_none() {
                *slot = Some(Instant::now() + observation_grace());
            }
        }
    };

    let grace_expired = || {
        grace_deadline
            .lock()
            .ok()
            .and_then(|slot| *slot)
            .is_some_and(|deadline| Instant::now() >= deadline)
    };

    loop {
        if grace_expired() {
            return Ok(StreamObservation {
                result,
                events,
                op_failed: false,
                incomplete: true,
            });
        }

        let chunk = match tokio::time::timeout(
            OUTCOME_POLL_INTERVAL,
            resp.chunk(),
        )
        .await
        {
            Ok(Ok(Some(chunk))) => chunk,
            Ok(Ok(None)) => {
                start_grace();
                return Ok(StreamObservation {
                    result,
                    events,
                    op_failed: false,
                    incomplete: true,
                });
            }
            Ok(Err(_)) => {
                start_grace();
                return Ok(StreamObservation {
                    result,
                    events,
                    op_failed: false,
                    incomplete: true,
                });
            }
            Err(_) => {
                if grace_expired() {
                    return Ok(StreamObservation {
                        result,
                        events,
                        op_failed: false,
                        incomplete: true,
                    });
                }
                continue;
            }
        };
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(idx) = buf.find("\n\n") {
            let frame = buf[..idx].to_string();
            buf = buf[idx + 2..].to_string();
            let mut data = String::new();
            for line in frame.lines() {
                if let Some(rest) = line.strip_prefix("data:") {
                    data.push_str(rest.trim_start());
                }
            }
            if data.is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&data)?;
            if sse_event_matches(&value, expected_session_id, expected_operation_id) {
                if let Some(text) = value
                    .get("MessageDelta")
                    .and_then(|v| v.get("text"))
                    .and_then(serde_json::Value::as_str)
                {
                    result.push_str(text);
                }
                if value.get("OpFailed").is_some() {
                    events.push(value);
                    start_grace();
                    return Ok(StreamObservation {
                        result,
                        events,
                        op_failed: true,
                        incomplete: false,
                    });
                }
                if value.get("OpFinished").is_some() {
                    events.push(value);
                    start_grace();
                    return Ok(StreamObservation {
                        result,
                        events,
                        op_failed: false,
                        incomplete: false,
                    });
                }
            }
            events.push(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::daemon_api::agent_host::character_operation_result::{
        NexusCharacterRunCaptureOutcome, NexusCharacterRunCaptureOutcomeStatus,
    };

    fn sample_delivery(outcome: Option<CharacterOperationResult>) -> RunDelivery {
        RunDelivery {
            session: SessionResponse {
                session_id: "sess".into(),
                provider_id: "mock".into(),
                state: "ready".into(),
                active_op_id: None,
                model: None,
                actor_ref: None,
                viewpoint: None,
            },
            operation: OperationResponse {
                operation_id: "op".into(),
                session_id: "sess".into(),
                status: "started".into(),
                capture: None,
            },
            stream: StreamObservation {
                result: "hello".into(),
                events: vec![],
                op_failed: false,
                incomplete: false,
            },
            outcome,
            notes: ObservationNotes::default(),
        }
    }

    fn succeeded_captured_outcome() -> CharacterOperationResult {
        CharacterOperationResult {
            session_id: "sess".into(),
            operation_id: "op".into(),
            run_status: CharacterOperationResultRunStatus::Succeeded,
            finish_reason: None,
            capture: NexusCharacterRunCaptureOutcome {
                status: NexusCharacterRunCaptureOutcomeStatus::Captured,
                pending_id: None,
                code: None,
            },
        }
    }

    #[test]
    fn classify_opt_out_succeeded_is_zero() {
        let delivery = sample_delivery(None);
        assert_eq!(classify_run_exit(false, &delivery), (0, String::new()));
    }

    #[test]
    fn classify_remember_captured_is_zero() {
        let delivery = sample_delivery(Some(succeeded_captured_outcome()));
        assert_eq!(classify_run_exit(true, &delivery), (0, String::new()));
    }

    #[test]
    fn classify_remember_skipped_is_nonzero() {
        let outcome = CharacterOperationResult {
            session_id: "sess".into(),
            operation_id: "op".into(),
            run_status: CharacterOperationResultRunStatus::Succeeded,
            finish_reason: None,
            capture: NexusCharacterRunCaptureOutcome {
                status: NexusCharacterRunCaptureOutcomeStatus::Skipped,
                pending_id: None,
                code: None,
            },
        };
        let delivery = sample_delivery(Some(outcome));
        let (code, _) = classify_run_exit(true, &delivery);
        assert_ne!(code, 0);
    }

    fn sample_session_operation() -> (SessionResponse, OperationResponse) {
        (
            SessionResponse {
                session_id: "sess-1".into(),
                provider_id: "mock".into(),
                state: "ready".into(),
                active_op_id: None,
                model: None,
                actor_ref: None,
                viewpoint: None,
            },
            OperationResponse {
                operation_id: "op-1".into(),
                session_id: "sess-1".into(),
                status: "started".into(),
                capture: None,
            },
        )
    }

    fn sse_message_delta_frame(session_id: &str, operation_id: &str, text: &str) -> String {
        let payload = serde_json::json!({
            "MessageDelta": {
                "session_id": session_id,
                "op_id": operation_id,
                "text": text,
            }
        });
        format!("data: {payload}\n\n")
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn consume_terminal_events_eof_arms_grace() {
        std::env::set_var("NEXUS42_RUN_OBSERVATION_GRACE_SECS", "1");
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string(sse_message_delta_frame("sess-1", "op-1", "partial")),
            )
            .mount(&server)
            .await;
        let mut resp = reqwest::Client::new()
            .get(format!("{}/events", server.uri()))
            .send()
            .await
            .expect("sse request");
        let grace = std::sync::Arc::new(std::sync::Mutex::new(None::<Instant>));
        let obs = consume_terminal_events(&mut resp, "sess-1", "op-1", grace.clone())
            .await
            .expect("consume");
        assert_eq!(obs.result, "partial");
        assert!(obs.incomplete);
        assert!(grace.lock().expect("grace").is_some());
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn observe_run_terminates_outcome_poll_after_sse_eof_within_grace() {
        std::env::set_var("NEXUS42_RUN_OBSERVATION_GRACE_SECS", "1");
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/v1/daemon/agent-host/sessions/sess-1/events",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_message_delta_frame("sess-1", "op-1", "hello")),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/v1/daemon/agent-host/operations/op-1",
            ))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = DaemonClient::new(&server.uri()).expect("client");
        let mut events = client
            .stream_get("/v1/daemon/agent-host/sessions/sess-1/events")
            .await
            .expect("events");
        let (session, operation) = sample_session_operation();
        let started = Instant::now();
        let delivery = observe_run_concurrently(&client, &mut events, &session, &operation)
            .await
            .expect("observe");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "outcome poll must terminate within grace, took {:?}",
            started.elapsed()
        );
        assert_eq!(delivery.stream.result, "hello");
        assert!(delivery.stream.incomplete);
        assert!(delivery.outcome.is_none());
        assert!(delivery.notes.outcome_unavailable);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn observe_run_preserves_output_when_outcome_poll_fails() {
        std::env::set_var("NEXUS42_RUN_OBSERVATION_GRACE_SECS", "1");
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/v1/daemon/agent-host/sessions/sess-1/events",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_message_delta_frame("sess-1", "op-1", "hello")),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/v1/daemon/agent-host/operations/op-1",
            ))
            .respond_with(wiremock::ResponseTemplate::new(500).set_body_string("{}"))
            .mount(&server)
            .await;

        let client = DaemonClient::new(&server.uri()).expect("client");
        let mut events = client
            .stream_get("/v1/daemon/agent-host/sessions/sess-1/events")
            .await
            .expect("events");
        let (session, operation) = sample_session_operation();
        let delivery = observe_run_concurrently(&client, &mut events, &session, &operation)
            .await
            .expect("observe must not abort on outcome poll failure");
        assert_eq!(delivery.stream.result, "hello");
        assert!(delivery.notes.outcome_unavailable);
    }

}
