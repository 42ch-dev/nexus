//! Family-typed execution operations on [`ExecutionHandle`] (v1.190 P3-T2).
//!
//! The four operations the execution authority owns for the transport:
//! schedule add/signal, durable workspace commit, and the bounded run-event
//! page. Each is a typed entry point over an authority the handle already
//! owns — the transport keeps its HTTP envelope, status codes and CLI
//! composition; this module only removes the transport's need to reach into
//! scheduler or commit internals directly.
//!
//! The schedule operations gate on STORED Actor admission: every transition
//! re-reads the durable row through the attached supervisor, never a
//! process-global "active" flag, so a restart cannot leave a live-but-
//! unadmitted owner.

use std::sync::Arc;

use nexus_contracts::generated::core::{
    CoreRunEventsResponse, CoreRunEventsResponseEventsItem, CoreRunEventsResponseEventsItemKind,
    CoreRunEventsResponseNextSequence, CoreRunEventsResponseRunId,
};
use nexus_contracts::{
    AddScheduleRequest, AddScheduleResponse, CoreRunEventsRequest, SignalScheduleRequest,
    SignalScheduleResponse,
};

use crate::error::{CoreError, CoreResult};
use crate::execution::lifecycle::ExecutionHandle;
use crate::execution::run_events::PageError;
use crate::execution::workflow::RunEventPort;
use crate::principal::Principal;

/// First sequence for a cursorless read.
const DEFAULT_RUN_EVENT_LIMIT: usize = 64;

impl ExecutionHandle {
    /// Insert a new schedule for the admitted principal's creator.
    ///
    /// The row is inserted through the attached supervisor with its frozen
    /// admission payload (source identity, structured input, role bindings), so
    /// a drive-enabled row is never observable without the payload its
    /// admission will read. Eligibility is still decided later by the tick,
    /// from the STORED row — this call does not pre-admit.
    ///
    /// # Errors
    /// `Forbidden` when the request names a foreign creator, `NotFound` when
    /// no supervisor is attached, `Busy` for a duplicate row, `Internal` when
    /// the preset cannot be frozen, and the mapped storage error on a failed
    /// insert.
    pub async fn add_schedule(
        &self,
        principal: &Principal,
        request: AddScheduleRequest,
    ) -> CoreResult<AddScheduleResponse> {
        if request.creator_id != principal.creator_id() {
            return Err(CoreError::Forbidden {
                resource: format!(
                    "schedule for creator {} (principal owns {})",
                    request.creator_id,
                    principal.creator_id()
                ),
            });
        }
        let supervisor = self.schedule_supervisor()?;
        let schedule_id = new_schedule_id();
        let schedule = build_schedule(&schedule_id, &request);
        // C-2/I-5: a `driven_v1` row must never be published without its
        // frozen admission payload. Freeze the preset's content-addressed
        // source identity, structured input and role bindings HERE, in the
        // same transaction as the row, so a concurrent tick can never observe
        // a drive-enabled row whose payload is missing.
        let descriptor = self.freeze_descriptor(&request)?;
        let seed = request.seed.clone().unwrap_or_default();
        supervisor
            .insert_pending_with_descriptor_and_seed(schedule, descriptor.as_deref(), &seed)
            .await
            .map_err(map_supervisor_error)?;
        Ok(AddScheduleResponse {
            schedule_id,
            status: "pending".to_string(),
            core_context_version: 0,
        })
    }

    /// Commit a validated change manifest through the durable workspace
    /// authority this owner was bound to.
    ///
    /// The manifest crosses the generated contract; the durable commit keeps
    /// its retained owner, digest idempotence and OCC pre-image checks. The
    /// principal is verified through the store `Principal` type — this
    /// operation takes an admitted principal and refuses an empty one.
    ///
    /// # Errors
    /// `Forbidden` for a principal with no creator, `NotFound` when no commit
    /// authority is bound, and the mapped conflict/validation/storage refusal
    /// (see [`crate::execution::workspace::commit_workspace`]).
    pub async fn commit_workspace(
        &self,
        principal: &Principal,
        request: nexus_contracts::CoreWorkspaceCommitRequest,
    ) -> CoreResult<nexus_contracts::CoreWorkspaceCommitResponse> {
        if principal.creator_id().trim().is_empty() {
            return Err(CoreError::Forbidden {
                resource: "workspace commit requires an admitted principal".into(),
            });
        }
        let authority = self.workspace_commit_authority()?;
        authority.commit(request).await
    }

    /// Apply a lifecycle signal to an existing schedule.
    ///
    /// `pause`/`resume` flip the durable row through the supervisor, which
    /// re-reads that row's status; the remaining signals are the transport's
    /// own admission/cancel orchestration and are refused here rather than
    /// half-implemented as a second admission path.
    ///
    /// # Errors
    /// `NotFound` when the schedule or an attached authority is absent,
    /// `Busy` for an ineligible/conflicting transition, `InvalidInput` for a
    /// signal this seam does not serve, `Internal` for a storage fault.
    pub async fn signal_schedule(
        &self,
        _principal: &Principal,
        schedule_id: String,
        request: SignalScheduleRequest,
    ) -> CoreResult<SignalScheduleResponse> {
        let supervisor = self.schedule_supervisor()?;
        let status = match request.signal.as_str() {
            "pause" => {
                if !supervisor
                    .pause_schedule(&schedule_id)
                    .await
                    .map_err(map_supervisor_error)?
                {
                    return Err(CoreError::Busy);
                }
                "paused"
            }
            "resume" => {
                // Smart resume reports the ACTUAL persisted status (it may fall
                // back to `pending` when admission is not yet possible), so the
                // response carries the store's answer rather than an assumption.
                let outcome = supervisor
                    .resume_schedule(&schedule_id)
                    .await
                    .map_err(map_supervisor_error)?;
                return Ok(SignalScheduleResponse {
                    schedule_id,
                    status: outcome,
                    current_wait_id: None,
                });
            }
            "start" | "advance" | "continue" | "cancel" => {
                return Err(CoreError::InvalidInput {
                    field: "signal".into(),
                    reason: format!(
                        "signal '{}' is served by the transport's schedule \
                         orchestration, not the core signal seam",
                        request.signal
                    ),
                });
            }
            other => {
                return Err(CoreError::InvalidInput {
                    field: "signal".into(),
                    reason: format!("unknown signal '{other}'"),
                });
            }
        };
        Ok(SignalScheduleResponse {
            schedule_id,
            status: status.to_string(),
            current_wait_id: None,
        })
    }
}

/// Read a bounded page of a run's retained events.
///
/// The page comes from the SAME bounded ring the SSE transport replays from
/// (through the run-event port), so the item/byte caps and the explicit-gap
/// frames are identical on both surfaces. `resync_required` reports a
/// retention-trimmed cursor.
///
/// # Errors
/// `NotFound` when no ring is retained for `run_id`; `InvalidInput` for an
/// unparsable cursor.
pub fn run_event_page(
    port: &Arc<dyn RunEventPort>,
    request: CoreRunEventsRequest,
) -> CoreResult<CoreRunEventsResponse> {
    // `after_sequence` is a validated decimal-string newtype and `limit` a
    // `NonZeroU64`, so both are already shape-checked by the contract.
    let after = request
        .after_sequence
        .as_ref()
        .map(|cursor| {
            cursor
                .as_str()
                .parse::<u64>()
                .map_err(|_| CoreError::InvalidInput {
                    field: "after_sequence".into(),
                    reason: "must be a non-negative decimal sequence".into(),
                })
        })
        .transpose()?;
    let limit = usize::try_from(request.limit.get()).unwrap_or(DEFAULT_RUN_EVENT_LIMIT);
    let page = port
        .read_page(request.run_id.as_str(), after, limit)
        .map_err(|err| match err {
            PageError::UnknownRun(run_id) => CoreError::NotFound {
                resource: format!("run event ring {run_id}"),
            },
        })?;

    let events = page
        .frames
        .into_iter()
        .map(|frame| CoreRunEventsResponseEventsItem {
            sequence: frame.sequence,
            kind: CoreRunEventsResponseEventsItemKind::try_from(frame.kind)
                .unwrap_or_else(|_| CoreRunEventsResponseEventsItemKind("event".to_string())),
            payload: serde_json::from_str::<serde_json::Value>(&frame.data)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default(),
        })
        .collect();
    Ok(CoreRunEventsResponse {
        run_id: CoreRunEventsResponseRunId::try_from(request.run_id).map_err(|err| {
            CoreError::InvalidInput {
                field: "run_id".into(),
                reason: err.to_string(),
            }
        })?,
        events,
        next_sequence: CoreRunEventsResponseNextSequence::try_from(
            page.next_sequence.to_string(),
        )
        .map_err(|err| CoreError::Internal {
            category: format!("run-event watermark encode: {err}"),
        })?,
        terminal: page.terminal,
        resync_required: page.resync_required,
    })
}

impl ExecutionHandle {
    /// Read a bounded page of a run's retained events.
    ///
    /// # Errors
    /// `NotFound` when this owner has no run-event port (or no ring for the
    /// run), `InvalidInput` for an unparsable cursor.
    pub fn run_events(
        &self,
        _principal: &Principal,
        request: CoreRunEventsRequest,
    ) -> CoreResult<CoreRunEventsResponse> {
        let port = self.coordinator().run_event_port().ok_or_else(|| {
            CoreError::NotFound {
                resource: "run event ring (no execution owner attached)".into(),
            }
        })?;
        run_event_page(&port, request)
    }
}

impl ExecutionHandle {
    /// Freeze a new row's admission payload (source identity + input +
    /// bindings) at insertion.
    ///
    /// `_system.*` presets are never admitted, so they carry no descriptor.
    /// Every other preset must resolve; a preset with prompt roles but no
    /// configured default binding provider is refused rather than published
    /// drive-enabled with an empty binding map.
    fn freeze_descriptor(
        &self,
        request: &AddScheduleRequest,
    ) -> CoreResult<Option<Vec<u8>>> {
        use nexus_orchestration::run_state::{AgentBinding, RunDescriptorV1};

        if request.preset_id.starts_with("_system.") {
            return Ok(None);
        }
        let home = self.nexus_home.as_deref().ok_or_else(|| CoreError::Internal {
            category: "schedule insert requires a nexus home to resolve the preset".into(),
        })?;
        let registry = self
            .capability_holder()
            .get()
            .ok_or_else(|| CoreError::Internal {
                category: "capability registry unavailable; cannot freeze preset identity".into(),
            })?;
        let loaded = nexus_preset::resolve_preset(&request.preset_id, home, &registry).map_err(
            |e| CoreError::Internal {
                category: format!("failed to resolve preset '{}': {e}", request.preset_id),
            },
        )?;
        let source = loaded.source_identity.clone().ok_or_else(|| CoreError::Internal {
            category: format!(
                "preset '{}' has no content-addressed source identity",
                request.preset_id
            ),
        })?;
        // N-9: derive bindings ONLY from the coordinator's configured default
        // provider, never from request-supplied catalog order.
        let roles = nexus_preset::required_prompt_roles(&loaded);
        let agent_bindings = if roles.is_empty() {
            std::collections::HashMap::new()
        } else {
            let Some(provider_id) = self.coordinator().binding_provider() else {
                return Err(CoreError::Internal {
                    category: format!(
                        "preset '{}' requires {} prompt role binding(s) but no configured \
                         default binding provider exists",
                        request.preset_id,
                        roles.len()
                    ),
                });
            };
            roles
                .into_iter()
                .map(|role| {
                    (
                        role,
                        AgentBinding {
                            provider_id: provider_id.to_string(),
                            model: None,
                        },
                    )
                })
                .collect()
        };
        let input = request
            .input
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        // work_id resolves from the structured input OR a JSON seed, matching
        // the retained add path: a gated preset whose Work is only named in the
        // seed must still freeze that Work into the row/descriptor.
        let work_id = input
            .get("work_id")
            .and_then(|w| w.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| {
                request
                    .seed
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .and_then(|v| {
                        v.get("work_id")
                            .and_then(|w| w.as_str())
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                    })
            });
        let descriptor = RunDescriptorV1 {
            creator_id: request.creator_id.clone(),
            work_id,
            workspace_root: std::path::PathBuf::new(),
            preset_id: request.preset_id.clone(),
            preset_version: 1,
            source,
            input,
            agent_bindings,
            parent_session_id: None,
            graph_name: None,
        };
        serde_json::to_vec(&descriptor)
            .map(Some)
            .map_err(|e| CoreError::Internal {
                category: format!("failed to serialize execution descriptor: {e}"),
            })
    }

    /// The bound workspace commit authority, or a typed `NotFound`.
    fn workspace_commit_authority(
        &self,
    ) -> CoreResult<crate::execution::workspace::WorkspaceCommitAuthority> {
        self.workspace_commit
            .clone()
            .ok_or_else(|| CoreError::NotFound {
                resource: "workspace commit authority (no execution owner bound)".into(),
            })
    }

    /// The attached schedule supervisor, or a typed `NotFound`.
    fn schedule_supervisor(
        &self,
    ) -> CoreResult<Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>> {
        self.coordinator()
            .schedule_supervisor()
            .ok_or_else(|| CoreError::NotFound {
                resource: "schedule supervisor (no execution owner attached)".into(),
            })
    }
}

/// Build a durable pending row from the generated add request.
///
/// `creator_id`/`preset_id`/`label`/`scheduled_at`/dependencies cross
/// verbatim; the status is `Pending` with a fresh version-0 core context, so
/// the row is inserted in exactly the shape the tick expects to admit.
fn build_schedule(
    schedule_id: &str,
    request: &AddScheduleRequest,
) -> nexus_contracts::local::schedule::Schedule {
    use nexus_contracts::local::schedule::{
        CoreContextVersion, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
    };
    Schedule {
        id: ScheduleId(schedule_id.to_string()),
        creator_id: request.creator_id.clone(),
        preset_id: request.preset_id.clone(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency: ScheduleConcurrency::Serial,
        depends_on: request
            .depends_on
            .iter()
            .map(|id| ScheduleId(id.clone()))
            .collect(),
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: request.scheduled_at.clone(),
        label: request.label.clone(),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    }
}

/// Fresh schedule id in the daemon's retained `SCH<timestamp>` shape.
fn new_schedule_id() -> String {
    format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"))
}

/// Map a supervisor refusal onto the neutral taxonomy.
fn map_supervisor_error(
    err: nexus_orchestration::schedule::supervisor::SupervisorError,
) -> CoreError {
    use nexus_orchestration::schedule::supervisor::SupervisorError as E;
    match err {
        E::NotFound(_) => CoreError::NotFound {
            resource: "schedule".into(),
        },
        E::InvalidTransition(..) => CoreError::Busy,
        E::DuplicateSchedule { .. } => CoreError::Busy,
        E::Database(e) => crate::error::db_err(&e),
    }
}
