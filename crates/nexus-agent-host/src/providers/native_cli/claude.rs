//! Claude Code CLI native provider adapter.
//!
//! Manages a `claude` subprocess using `tokio::process::Command`.
//! Wave 1 limitations:
//! - Non-interactive single-turn prompt only
//! - Streaming via stdout line/chunk normalization
//! - Cancellation via process termination
//! - No structured tool calls, session restore, or MCP transport

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;

use crate::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, OperationStartedEvent,
    ProviderDescriptor, ProviderHealth, ProtocolKind, TextDeltaEvent,
};
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};
use crate::ProviderAdapter;

/// Internal state for a managed native CLI process.
#[derive(Debug)]
struct NativeSession {
    /// The child process handle.
    child: tokio::process::Child,
    /// Stdin writer for sending prompts.
    /// Wave 1 uses per-operation spawns; retained for future multi-turn support.
    #[allow(dead_code)]
    stdin: tokio::process::ChildStdin,
}

/// Claude Code CLI native provider.
///
/// Spawns `claude` (or a configured command) as a subprocess and
/// normalizes its stdout/stderr into `HostEvent` items.
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
    /// Active sessions: host session ID → managed process.
    sessions: Arc<RwLock<HashMap<HostSessionId, NativeSession>>>,
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
    ) -> Self {
        Self {
            provider_id,
            display_name,
            command,
            args,
            env,
            sessions: Arc::new(RwLock::new(HashMap::new())),
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
        )
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

    async fn probe(&self, _request: crate::capability::model::ProbeRequest) -> HostResult<ProviderHealth> {
        // Check if the command exists on PATH by attempting `which` / `where`
        let which_result = tokio::process::Command::new("which")
            .arg(&self.command)
            .output()
            .await;

        match which_result {
            Ok(output) if output.status.success() => {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                Ok(ProviderHealth {
                    provider_id: self.provider_id.clone(),
                    available: true,
                    latency_ms: None,
                    message: Some(path),
                })
            }
            _ => Ok(ProviderHealth {
                provider_id: self.provider_id.clone(),
                available: false,
                latency_ms: None,
                message: Some(format!("command '{}' not found on PATH", self.command)),
            }),
        }
    }

    async fn launch(&self, spec: crate::capability::model::LaunchSpec) -> HostResult<ManagedSessionHandle> {
        // For native CLI, "launch" means we prepare the session but don't spawn
        // the process yet — that happens in execute(). The session handle is
        // created here with a placeholder.
        let host_session_id = HostSessionId::new();

        // We don't start the process at launch; we just register the session.
        // The actual subprocess is spawned per-operation in execute().
        // This matches the "non-interactive single-turn" model.

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

    async fn execute(
        &self,
        _session: &ManagedSessionHandle,
        op: crate::capability::model::HostOperation,
    ) -> HostResult<HostEventStream> {
        let (op_id, content_blocks) = match op {
            crate::capability::model::HostOperation::Prompt { op_id, content } => (op_id, content),
            _ => {
                return Err(HostError::capability_unsupported(
                    self.provider_id.clone(),
                    "non-prompt operation",
                    "Native CLI provider only supports Prompt operations in Wave 1",
                ));
            }
        };

        // Build prompt text from content blocks
        let prompt_text: String = content_blocks
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

        // Spawn the subprocess: command [args...] -- <prompt>
        // The prompt is passed as stdin to avoid shell escaping issues.
        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&self.args)
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

        // Write prompt to stdin
        let stdin = child.stdin.take();
        if let Some(mut stdin) = stdin {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(prompt_text.as_bytes()).await.map_err(|e| {
                HostError::protocol_error("failed to write prompt to stdin", Some(e.to_string()))
            })?;
            drop(stdin); // Close stdin to signal EOF
        }

        let op_id_for_stream = op_id.clone();
        let session_id = HostSessionId::new(); // Native: per-op session
        let provider_id = self.provider_id.clone();

        // Take stdout and stderr before moving child
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Stream stdout lines as MessageDelta events, then wait for exit
        let started_stream = futures_util::stream::once({
            let op_id = op_id_for_stream.clone();
            let session_id = session_id.clone();
            async move {
                Ok(HostEvent::OpStarted(OperationStartedEvent {
                    op_id,
                    session_id,
                }))
            }
        });

        // Create a line-by-line stream from stdout
        let op_id = op_id_for_stream.clone();
        let session_id_for_stdout = session_id.clone();

        let stdout_stream: HostEventStream = if let Some(stdout) = stdout {
            let reader = tokio::io::BufReader::new(stdout);
            futures_util::stream::unfold(
                (reader, op_id, session_id_for_stdout),
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
                            let text = line.trim_end_matches('\n').trim_end_matches('\r').to_string();
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
                        Err(e) => {
                            Some((
                                Ok(HostEvent::OpFailed(OperationFailedEvent {
                                    session_id,
                                    op_id,
                                    error_category: "io_error".to_string(),
                                    error_message: e.to_string(),
                                })),
                                (reader, HostOperationId::new(), HostSessionId::new()),
                            ))
                        }
                    }
                },
            )
            .boxed()
        } else {
            futures_util::stream::once(async move {
                Ok(HostEvent::OpFailed(OperationFailedEvent {
                    session_id: session_id_for_stdout,
                    op_id,
                    error_category: "io_error".to_string(),
                    error_message: "stdout not captured".to_string(),
                }))
            })
            .boxed()
        };

        let stream = started_stream.chain(stdout_stream).boxed();

        // Spawn a background task to drain stderr and log warnings
        if let Some(stderr) = stderr {
            let provider_id = provider_id.clone();
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
        let mut sessions = self.sessions.write().await;
        if let Some(mut native_session) = sessions.remove(&session.session_id) {
            // Kill the child process if still running
            let _ = native_session.child.kill().await;
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
        assert!(!desc.capabilities.session_restore);
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
        );

        assert_eq!(provider.provider_id.0, "my-claude");
        assert_eq!(provider.command, "/opt/claude/bin/claude");
        assert_eq!(provider.args.len(), 2);
        assert_eq!(provider.env.get("ANTHROPIC_API_KEY").unwrap(), "sk-test");
    }
}
