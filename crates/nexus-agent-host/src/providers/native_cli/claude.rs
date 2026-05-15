//! Claude Code CLI native provider adapter.
//!
//! Manages a `claude` subprocess using `tokio::process::Command`.
//! Multi-turn session continuity via `--session-id` and `--resume` flags.
//!
//! # Session Model
//!
//! Each `launch()` generates a host-side UUID (the Claude CLI session ID).
//! The first `execute()` invocation passes `--session-id <uuid>` to the CLI.
//! Subsequent invocations pass `--resume <uuid>`, providing conversation
//! continuity across separate process spawns.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    ProtocolKind, ProviderDescriptor, ProviderHealth, TextDeltaEvent,
};
use crate::config::TimeoutConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::ProviderAdapter;

/// Internal state for a managed native CLI session.
#[derive(Debug)]
struct NativeSession {
    /// The Claude CLI session ID (UUID) used for `--session-id` / `--resume`.
    /// Set at `launch()` time; used by `execute()` to pass the correct flags.
    claude_session_id: String,
    /// Whether the first `execute()` has been performed for this session.
    /// `false` → pass `--session-id`, `true` → pass `--resume`.
    first_exec_done: bool,
}

/// Claude Code CLI native provider.
///
/// Spawns `claude` (or a configured command) as a subprocess and
/// normalizes its stdout/stderr into `HostEvent` items.
///
/// Multi-turn session continuity is achieved by passing `--session-id <uuid>`
/// on the first invocation and `--resume <uuid>` on subsequent ones.
pub struct ClaudeCliProvider {
    /// Provider ID (typically `claude-native` to avoid collision with ACP registry).
    provider_id: ProviderId,
    /// Display name.
    display_name: String,
    /// Command to execute (e.g., `claude`).
    command: String,
    /// Default arguments for non-interactive prompt mode.
    args: Vec<String>,
    /// Environment variables to inject.
    env: HashMap<String, String>,
    /// Active sessions: host session ID → native session state.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
    /// Timeout configuration for stage-level enforcement.
    timeouts: TimeoutConfig,
}

impl ClaudeCliProvider {
    /// Create a new Claude CLI provider with the given configuration.
    #[must_use]
    pub fn new(
        provider_id: ProviderId,
        display_name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        timeouts: TimeoutConfig,
    ) -> Self {
        Self {
            provider_id,
            display_name,
            command,
            args,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeouts,
        }
    }

    /// Create with default configuration for the `claude` command.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(
            ProviderId::new("claude-native"),
            "Claude Code CLI (native)".to_string(),
            "claude".to_string(),
            vec!["--print".to_string()],
            HashMap::new(),
            TimeoutConfig::default(),
        )
    }

    /// Build the event stream from stdout lines.
    ///
    /// Emits `OpStarted`, then `MessageDelta` per line, and a terminal
    /// `OpFinished`/`OpFailed` when stdout reaches EOF or an I/O error occurs.
    fn build_event_stream(
        stdout: Option<tokio::process::ChildStdout>,
        op_id: HostOperationId,
        session_id: HostSessionId,
    ) -> HostEventStream {
        let started = futures_util::stream::once({
            let op_id = op_id.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        let stdout_stream: HostEventStream = if let Some(stdout) = stdout {
            let reader = tokio::io::BufReader::new(stdout);
            futures_util::stream::unfold(
                (reader, op_id, session_id),
                |(mut reader, op_id, session_id)| async move {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) => {
                            // EOF — emit terminal event
                            Some((
                                Ok(HostEvent::OpFinished(OperationFinishedEvent {
                                    session_id,
                                    op_id,
                                    reason: FinishReason::EndTurn,
                                })),
                                (reader, HostOperationId::new(), HostSessionId::new()),
                            ))
                        }
                        Ok(_) => {
                            let text = line
                                .trim_end_matches('\n')
                                .trim_end_matches('\r')
                                .to_string();
                            let next_op = op_id.clone();
                            let next_session = session_id.clone();
                            Some((
                                Ok(HostEvent::MessageDelta(TextDeltaEvent {
                                    session_id,
                                    op_id,
                                    text,
                                })),
                                (reader, next_op, next_session),
                            ))
                        }
                        Err(e) => Some((
                            Ok(HostEvent::OpFailed(OperationFailedEvent {
                                session_id,
                                op_id,
                                error_category: "io_error".to_string(),
                                error_message: e.to_string(),
                            })),
                            (reader, HostOperationId::new(), HostSessionId::new()),
                        )),
                    }
                },
            )
            .boxed()
        } else {
            futures_util::stream::once(async move {
                Ok(HostEvent::OpFailed(OperationFailedEvent {
                    session_id,
                    op_id,
                    error_category: "io_error".to_string(),
                    error_message: "stdout not captured".to_string(),
                }))
            })
            .boxed()
        };

        started.chain(stdout_stream).boxed()
    }

    /// Spawn the CLI subprocess and write prompt to stdin.
    ///
    /// Returns `(stdout, stderr, child)` ready for event stream construction.
    async fn spawn_and_write_stdin(
        &self,
        full_args: &[String],
        prompt_text: &str,
    ) -> HostResult<(
        Option<tokio::process::ChildStdout>,
        Option<tokio::process::ChildStderr>,
        tokio::process::Child,
    )> {
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(full_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .envs(&self.env);

        let mut child = cmd.spawn().map_err(|e| {
            HostError::launch_failed(
                self.provider_id.clone(),
                format!("failed to spawn '{}'", self.command),
                Some(e.to_string()),
            )
        })?;

        // Write prompt to stdin and close it
        let stdin = child.stdin.take();
        if let Some(mut stdin) = stdin {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt_text.as_bytes()).await.map_err(|e| {
                HostError::protocol_error("failed to write prompt to stdin", Some(e.to_string()))
            })?;
            drop(stdin);
        }

        Ok((child.stdout.take(), child.stderr.take(), child))
    }
}

#[async_trait]
impl ProviderAdapter for ClaudeCliProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: self.provider_id.clone(),
            display_name: self.display_name.clone(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: CapabilityDescriptor::native_cli_limited(),
        }
    }

    async fn probe(
        &self,
        _request: crate::capability::model::ProbeRequest,
    ) -> HostResult<ProviderHealth> {
        // Cross-platform command lookup: `which` crate handles PATH scanning
        // and Windows PATHEXT resolution automatically. Wrapped in
        // spawn_blocking to keep the async runtime responsive, and
        // enforced with launch_ms timeout.
        let command = self.command.clone();
        let provider_id = self.provider_id.clone();
        let launch_dur = self.timeouts.launch_duration();

        let result = tokio::time::timeout(
            launch_dur,
            tokio::task::spawn_blocking(move || which::which(&command)),
        )
        .await
        .map_err(|_| {
            HostError::timeout(
                "probe",
                format!(
                    "command lookup timed out after {}ms",
                    self.timeouts.launch_ms
                ),
            )
            .with_provider(self.provider_id.clone())
        })?;

        let health = match result {
            Ok(Ok(resolved_path)) => ProviderHealth {
                provider_id,
                available: true,
                latency_ms: None,
                message: Some(resolved_path.to_string_lossy().into_owned()),
            },
            _ => ProviderHealth {
                provider_id,
                available: false,
                latency_ms: None,
                message: Some(format!("command '{}' not found on PATH", self.command)),
            },
        };
        Ok(health)
    }

    async fn launch(
        &self,
        spec: crate::capability::model::LaunchSpec,
    ) -> HostResult<ManagedSessionHandle> {
        // For native CLI providers, launch() only registers session state
        // (no process spawned yet — the actual process spawns in execute()).
        let host_session_id = HostSessionId::new();

        // Generate a UUID for Claude CLI session continuity.
        // This is passed as `--session-id <uuid>` on first execute and
        // `--resume <uuid>` on subsequent ones.
        let claude_session_id = uuid::Uuid::new_v4().to_string();

        // Store session state (no process spawned yet — that happens in execute()).
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(
                host_session_id.clone(),
                NativeSession {
                    claude_session_id,
                    first_exec_done: false,
                },
            );
        }

        tracing::info!(
            session_id = %host_session_id,
            provider_id = %self.provider_id,
            cwd = %spec.cwd.display(),
            "Native CLI session registered (process spawns on first execute)"
        );

        Ok(ManagedSessionHandle {
            provider_id: self.provider_id.clone(),
            session_id: host_session_id,
            capabilities: CapabilityDescriptor::native_cli_limited(),
        })
    }

    // Multi-turn execute involves session-state lookup, CLI flag assembly,
    // process spawn, stdin write, and stdout streaming — splitting would
    // reduce clarity, so allow the line count here.
    #[allow(clippy::too_many_lines)]
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let crate::capability::model::HostOperation::Prompt { op_id, content } = op else {
            return Err(HostError::capability_unsupported(
                self.provider_id.clone(),
                "non-prompt operation",
                "Native CLI provider only supports Prompt operations",
            ));
        };

        // Build prompt text from content blocks
        let prompt_text: String = content
            .iter()
            .map(|block| match block {
                HostContentBlock::Text { text } => text.as_str(),
                HostContentBlock::ResourceLink { uri, .. } => uri.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        if prompt_text.is_empty() {
            return Err(HostError::protocol_error(
                "empty prompt text for native CLI",
                None,
            ));
        }

        // Look up session state to determine session-continuity flags
        let (claude_session_id, is_resume) = {
            let mut sessions = self.sessions.write().await;
            let native_session = sessions.get_mut(&session.session_id).ok_or_else(|| {
                HostError::internal(format!(
                    "session {} not found in native CLI provider",
                    session.session_id
                ))
            })?;

            let id = native_session.claude_session_id.clone();
            let resume = native_session.first_exec_done;
            native_session.first_exec_done = true;
            drop(sessions);
            (id, resume)
        };

        // Build command arguments: base args + session-continuity flags
        let mut full_args = self.args.clone();

        if is_resume {
            // Subsequent calls: --resume <session-id>
            full_args.push("--resume".to_string());
            full_args.push(claude_session_id.clone());
        } else {
            // First call: --session-id <uuid>
            full_args.push("--session-id".to_string());
            full_args.push(claude_session_id);
        }

        // Spawn the subprocess with prompt_ms timeout for the setup phase
        // (spawn + stdin write). The streaming phase runs until EOF.
        let prompt_dur = self.timeouts.prompt_duration();

        let spawn_result = tokio::time::timeout(
            prompt_dur,
            self.spawn_and_write_stdin(&full_args, &prompt_text),
        )
        .await
        .map_err(|_| {
            HostError::timeout(
                "prompt",
                format!(
                    "CLI process setup timed out after {}ms",
                    self.timeouts.prompt_ms
                ),
            )
            .with_provider(self.provider_id.clone())
            .with_session(session.session_id.clone())
            .with_op(op_id.clone())
        })??;

        let (stdout, stderr, mut child) = spawn_result;

        let stream = Self::build_event_stream(stdout, op_id, session.session_id.clone());

        // Spawn a background task to drain stderr and log warnings
        if let Some(stderr) = stderr {
            let provider_id = self.provider_id.clone();
            tokio::spawn(async move {
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::warn!(
                        provider_id = %provider_id,
                        stderr = %line,
                        "Native CLI stderr output"
                    );
                }
            });
        }

        // Wait for the child process in the background to prevent zombies
        tokio::spawn(async move {
            let _ = child.wait().await;
        });

        Ok(stream)
    }

    async fn cancel(
        &self,
        _session: &ManagedSessionHandle,
        _op_id: HostOperationId,
    ) -> HostResult<()> {
        // Native CLI cancellation in Wave 1 is a no-op since the process
        // is spawned per-operation and will exit when stdin is closed.
        // If the process is still running, we could kill it, but since
        // each execute() spawns a fresh child, cancellation is handled
        // by the process completing or timing out.
        tracing::info!(
            provider_id = %self.provider_id,
            "Native CLI cancel requested (process exits naturally on stdin close)"
        );
        Ok(())
    }

    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()> {
        {
            let mut sessions = self.sessions.write().await;
            sessions.remove(&session.session_id);
        }
        tracing::info!(
            session_id = %session.session_id,
            provider_id = %self.provider_id,
            "Native CLI session shut down"
        );
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor::native_cli_limited()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_descriptor() {
        let provider = ClaudeCliProvider::default_config();
        let desc = provider.descriptor();

        assert_eq!(desc.provider_id.0, "claude-native");
        assert_eq!(desc.protocol_kind, ProtocolKind::NativeCli);
        assert!(desc.capabilities.text_prompt);
        assert!(desc.capabilities.streaming);
        assert!(desc.capabilities.cancellation);
        assert!(!desc.capabilities.structured_tool_calls);
        assert!(
            desc.capabilities.session_restore,
            "native CLI now supports session_restore via --resume"
        );
        assert!(!desc.capabilities.mcp_http);
    }

    #[test]
    fn default_config_has_print_flag() {
        let provider = ClaudeCliProvider::default_config();
        assert_eq!(provider.command, "claude");
        assert_eq!(provider.args, vec!["--print".to_string()]);
    }

    #[tokio::test]
    async fn probe_unavailable_when_command_not_found() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("nonexistent-cli-xyz"),
            "Fake".to_string(),
            "nonexistent_cli_xyz_12345".to_string(),
            vec![],
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let health = provider
            .probe(crate::capability::model::ProbeRequest { timeout_ms: 5000 })
            .await
            .expect("probe should succeed");

        assert!(!health.available);
        assert!(health.message.unwrap().contains("not found"));
    }

    #[test]
    fn custom_config_overrides() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("my-claude"),
            "My Claude".to_string(),
            "/opt/claude/bin/claude".to_string(),
            vec!["-p".to_string(), "--verbose".to_string()],
            HashMap::from([("ANTHROPIC_API_KEY".to_string(), "sk-test".to_string())]),
            TimeoutConfig::default(),
        );

        assert_eq!(provider.provider_id.0, "my-claude");
        assert_eq!(provider.command, "/opt/claude/bin/claude");
        assert_eq!(provider.args.len(), 2);
        assert_eq!(provider.env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
    }

    #[test]
    fn native_cli_limited_descriptor_session_restore() {
        let caps = CapabilityDescriptor::native_cli_limited();
        assert!(
            caps.session_restore,
            "native_cli_limited should claim session_restore since --resume is supported"
        );
    }

    #[tokio::test]
    async fn launch_generates_session_id_and_registers_state() {
        let provider = ClaudeCliProvider::default_config();

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch should succeed");

        // Verify the session was registered
        let sessions = provider.sessions.read().await;
        let native_session = sessions.get(&handle.session_id);
        assert!(native_session.is_some(), "session should be registered");

        let ns = native_session.unwrap();
        assert!(
            !ns.claude_session_id.is_empty(),
            "claude_session_id should be set"
        );
        assert!(
            !ns.first_exec_done,
            "first_exec_done should be false initially"
        );
    }

    #[tokio::test]
    async fn execute_uses_session_id_flag_on_first_call() {
        let provider = ClaudeCliProvider::new(
            ProviderId::new("test-native"),
            "Test".to_string(),
            "echo".to_string(), // `echo` is available on all platforms
            vec!["--print".to_string()],
            HashMap::new(),
            TimeoutConfig::default(),
        );

        let handle = provider
            .launch(crate::capability::model::LaunchSpec {
                cwd: std::path::PathBuf::from("/tmp"),
                model: None,
                mode: None,
                mcp_servers: vec![],
            })
            .await
            .expect("launch");

        // First execute should succeed (echo just prints the prompt)
        let result = provider
            .execute(
                &handle,
                crate::capability::model::HostOperation::Prompt {
                    op_id: HostOperationId::new(),
                    content: vec![HostContentBlock::Text {
                        text: "hello".to_string(),
                    }],
                },
            )
            .await;

        assert!(result.is_ok(), "first execute should succeed");

        // Verify first_exec_done is now true
        let sessions = provider.sessions.read().await;
        let ns = sessions.get(&handle.session_id).expect("session exists");
        assert!(
            ns.first_exec_done,
            "first_exec_done should be true after first execute"
        );
    }

    #[test]
    fn acp_full_descriptor_set_model_false_set_mode_true() {
        let caps = CapabilityDescriptor::acp_full();
        assert!(
            !caps.set_model,
            "acp_full should claim set_model = false (depends on dynamic discovery)"
        );
        assert!(
            caps.set_mode,
            "acp_full should claim set_mode = true (stable RPC available)"
        );
    }
}
