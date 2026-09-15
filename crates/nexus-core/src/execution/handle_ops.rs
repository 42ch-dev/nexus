//! Family-typed execution operations on [`ExecutionHandle`] (v1.190 P3-T2).
//!
//! The four operations the execution authority owns for the transport:
//! schedule add/signal, durable workspace commit, and the bounded run-event
//! page. Each is a typed entry point over an authority the handle already
//! owns — the transport keeps its HTTP envelope, status codes and CLI
//! composition; this module only removes the transport's need to reach into
//! scheduler or commit internals directly.
//!
//! Three invariants hold across every operation here:
//!
//! 1. **Owner fence.** Every entry point first checks the coordinator's
//!    draining barrier (the same T1 admission predicate `ensure_driving` and
//!    `admit_schedule` gate on), so an operation reaching a closing owner is
//!    refused with `Closing` rather than writing beside the in-flight drain.
//! 2. **Stored Actor admission.** Schedule eligibility is decided from the
//!    durable row (status/policy re-read per transition), never a
//!    process-global flag.
//! 3. **Gate policy.** A `driven_v1` row is only published after the preset's
//!    declared gates pass — or after an audited `force_gates` bypass — exactly
//!    as the transport path does. The typed seam cannot enqueue a gated preset
//!    on a missing, foreign or invalid Work.

use std::sync::Arc;

use nexus_contracts::generated::core::{
    CoreRunEventsResponse, CoreRunEventsResponseEventsItem, CoreRunEventsResponseEventsItemKind,
    CoreRunEventsResponseNextSequence, CoreRunEventsResponseRunId,
};
use nexus_contracts::local::orchestration::preset_gate::{FailedGate, PresetGatesFailed};
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AddScheduleResponse, ScheduleConcurrencyRequest, SignalScheduleRequest,
    SignalScheduleResponse,
};
use nexus_contracts::local::schedule::{
    CoreContextVersion, ParallelWithIds, Schedule, ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_contracts::CoreRunEventsRequest;
use nexus_orchestration::preset_gates::{
    GateEvalError, PreviousPresetLookup, PreviousPresetResult, PresetInput, WorkSnapshot,
    evaluate_gates,
};

use crate::error::{CoreError, CoreResult};
use crate::execution::lifecycle::ExecutionHandle;
use crate::execution::run_events::PageError;
use crate::execution::workflow::RunEventPort;
use crate::principal::Principal;
use crate::PresetError;

/// Default page size for a cursorless run-event read.
const DEFAULT_RUN_EVENT_LIMIT: usize = 64;

/// Maximum length for a `force_gates` audit reason (mirrors the transport).
const MAX_REASON_LEN: usize = 512;

impl ExecutionHandle {
    /// Insert a new schedule for the admitted principal's creator.
    ///
    /// The preset's declared gates are evaluated BEFORE any write, and a
    /// `force_gates` bypass records an audit row first. The row is then
    /// inserted through the attached supervisor with its frozen admission
    /// payload (source identity, structured input, role bindings and the
    /// request's own concurrency declaration), so a drive-enabled row is never
    /// observable without the payload its admission will read. Eligibility is
    /// still decided later by the tick, from the STORED row — this call does
    /// not pre-admit.
    ///
    /// # Errors
    /// `Forbidden` when the request names a foreign creator, `Closing` when the
    /// owner is shutting down, `Busy` for a duplicate row, `InvalidInput` for a
    /// malformed `force_gates` request, `Preset` for a failed gate evaluation,
    /// and the mapped storage error on a failed insert.
    pub async fn add_schedule(
        &self,
        principal: &Principal,
        request: AddScheduleRequest,
    ) -> CoreResult<AddScheduleResponse> {
        self.ensure_admitting()?;
        if request.creator_id != principal.creator_id() {
            return Err(CoreError::Forbidden {
                resource: format!(
                    "schedule for creator {} (principal owns {})",
                    request.creator_id,
                    principal.creator_id()
                ),
            });
        }

        // Resolve the preset ONCE: the same resolution supplies both the gate
        // set that must pass and the frozen source identity, so policy and the
        // frozen payload cannot disagree.
        let loaded = self.resolve_preset(&request.preset_id)?;
        let empty_gates: &[nexus_contracts::local::orchestration::preset_gate::Gate] = &[];
        let gates = loaded
            .as_ref()
            .map_or(empty_gates, |preset| preset.manifest.preset.gates.as_slice());
        let audit_reason = self.enforce_gate_policy(&request, gates).await?;

        // A bypass is recorded BEFORE anything else can fail, and in particular
        // before the insertion preconditions below. The daemon writes the audit
        // and schedule rows in one transaction; this seam keeps ONE insertion
        // authority (the supervisor, which owns the row+deps+seed transaction)
        // and instead guarantees the audit trail is never missing — an audit row
        // without a schedule records an ATTEMPTED bypass, which is the
        // conservative side.
        //
        // Ordering is load-bearing: an audit written after a fallible
        // precondition would be silently skipped exactly when the operation is
        // refused, so a bypass attempt that never reached a row would leave no
        // trace at all. Everything that can refuse the insert therefore runs
        // AFTER the audit write.
        if let Some(reason) = &audit_reason {
            self.write_force_gates_audit(&request, reason).await?;
        }

        let supervisor = self.schedule_supervisor()?;
        let schedule_id = new_schedule_id();
        let schedule = build_schedule(&schedule_id, &request);
        // C-2/I-5: a `driven_v1` row must never be published without its
        // frozen admission payload.
        let descriptor = self.freeze_descriptor(&request, loaded.as_ref())?;
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
    /// its retained owner, digest idempotence and OCC pre-image checks.
    ///
    /// # Errors
    /// `Forbidden` for a principal with no creator, `Closing` when the owner is
    /// shutting down, `NotFound` when no commit authority is bound, and the
    /// mapped conflict/validation/storage refusal.
    pub async fn commit_workspace(
        &self,
        principal: &Principal,
        request: nexus_contracts::CoreWorkspaceCommitRequest,
    ) -> CoreResult<nexus_contracts::CoreWorkspaceCommitResponse> {
        self.ensure_admitting()?;
        if principal.creator_id().trim().is_empty() {
            return Err(CoreError::Forbidden {
                resource: "workspace commit requires an admitted principal".into(),
            });
        }
        let authority = self
            .workspace_commit_authority()
            .ok_or_else(|| CoreError::NotFound {
                resource: "workspace commit authority (no execution owner bound)".into(),
            })?
            .clone();
        authority.commit(request).await
    }

    /// Apply a lifecycle signal to an existing schedule.
    ///
    /// Ownership is enforced BEFORE mutation: the durable row's `creator_id`
    /// must equal the admitted principal's creator, so a caller cannot pause or
    /// resume a foreign schedule by guessing its id. `pause`/`resume` then flip
    /// the durable row through the supervisor, which re-reads that row's
    /// status. The remaining signals are the transport's own admission/cancel
    /// orchestration and are refused here rather than half-implemented as a
    /// second admission path.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `NotFound` when the schedule
    /// is absent or owned by another creator, `Busy` for an ineligible
    /// transition, `InvalidInput` for a signal this seam does not serve,
    /// `Internal` for a storage fault.
    pub async fn signal_schedule(
        &self,
        principal: &Principal,
        schedule_id: String,
        request: SignalScheduleRequest,
    ) -> CoreResult<SignalScheduleResponse> {
        self.ensure_admitting()?;
        let supervisor = self.schedule_supervisor()?;
        self.require_schedule_owner(principal, &schedule_id).await?;

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

    /// Read a bounded page of a run's retained events.
    ///
    /// # Errors
    /// `Closing` when the owner is shutting down, `NotFound` when this owner
    /// has no run-event port (or no ring for the run), `InvalidInput` for an
    /// unparsable cursor.
    pub fn run_events(
        &self,
        _principal: &Principal,
        request: CoreRunEventsRequest,
    ) -> CoreResult<CoreRunEventsResponse> {
        self.ensure_admitting()?;
        let port = self.coordinator().run_event_port().ok_or_else(|| CoreError::NotFound {
            resource: "run event ring (no execution owner attached)".into(),
        })?;
        run_event_page(&port, request)
    }

    /// The owner-level fence every entry point checks.
    ///
    /// Reuses the coordinator's T1 admission barrier (the same predicate
    /// `ensure_driving`/`admit_schedule` gate on), so an operation reaching a
    /// closing owner cannot install an effect beside the in-flight drain.
    fn ensure_admitting(&self) -> CoreResult<()> {
        if self.coordinator().is_draining() {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    /// Require the durable schedule row to belong to the principal's creator.
    ///
    /// A missing row and a foreign row return the SAME `NotFound`, so a caller
    /// cannot probe for the existence of another creator's schedules.
    async fn require_schedule_owner(
        &self,
        principal: &Principal,
        schedule_id: &str,
    ) -> CoreResult<()> {
        let pool = self.coordinator().pool();
        let owner: Option<String> =
            sqlx::query_scalar("SELECT creator_id FROM creator_schedules WHERE schedule_id = ?")
                .bind(schedule_id)
                .fetch_optional(pool.as_ref())
                .await
                .map_err(|e| crate::error::db_err(&e))?;
        if owner.as_deref() == Some(principal.creator_id()) {
            return Ok(());
        }
        Err(CoreError::NotFound {
            resource: format!("schedule {schedule_id}"),
        })
    }

    /// Write the audited `force_gates` bypass row.
    async fn write_force_gates_audit(
        &self,
        request: &AddScheduleRequest,
        reason: &str,
    ) -> CoreResult<()> {
        let pool = self.coordinator().pool();
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| crate::error::db_err(&e))?;
        let params = nexus_local_db::ForceGatesAuditParams {
            audit_id: format!("fga_{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f")),
            preset_id: request.preset_id.clone(),
            work_id: request
                .input
                .as_ref()
                .and_then(|v| v.get("work_id"))
                .and_then(|w| w.as_str())
                .map(str::to_string)
                .or_else(|| work_id_from_seed(request.seed.as_deref()))
                .unwrap_or_else(|| "unknown".to_string()),
            creator_id: request.creator_id.clone(),
            reason: reason.to_string(),
            forced_at: chrono::Utc::now().to_rfc3339(),
        };
        nexus_local_db::insert_force_gates_audit(&mut *conn, &params)
            .await
            .map_err(|e| CoreError::Internal {
                category: format!("failed to write force-gates audit row: {e}"),
            })
    }

    /// Evaluate the preset's declared gates, returning the audit reason when
    /// the caller asked to bypass them.
    async fn enforce_gate_policy(
        &self,
        request: &AddScheduleRequest,
        gates: &[nexus_contracts::local::orchestration::preset_gate::Gate],
    ) -> CoreResult<Option<String>> {
        if request.force_gates {
            let raw = request.reason.as_deref().unwrap_or("");
            if raw.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "reason".into(),
                    reason: "force_gates requires a non-empty reason (audit-logged)".into(),
                });
            }
            let sanitized = sanitize_reason(raw);
            if sanitized.len() > MAX_REASON_LEN {
                return Err(CoreError::InvalidInput {
                    field: "reason".into(),
                    reason: format!(
                        "reason exceeds maximum length ({MAX_REASON_LEN} chars); got {} chars",
                        sanitized.len()
                    ),
                });
            }
            if sanitized != raw {
                return Err(CoreError::InvalidInput {
                    field: "reason".into(),
                    reason: "reason contains ANSI escape sequences or control characters"
                        .to_string(),
                });
            }
            return Ok(Some(raw.to_string()));
        }

        if gates.is_empty() {
            return Ok(None);
        }

        // A gated preset ALWAYS requires work_id for evaluation. Failing closed
        // here is what stops the typed seam enqueuing a gated preset unchecked.
        let work_id = request
            .input
            .as_ref()
            .and_then(|v| v.get("work_id"))
            .and_then(|w| w.as_str())
            .map(str::to_string)
            .or_else(|| work_id_from_seed(request.seed.as_deref()));
        let Some(work_id) = work_id else {
            return Err(gate_failure(
                &request.preset_id,
                "",
                vec![FailedGate {
                    kind: "work_field".to_string(),
                    expected: "work_id must be provided for gated preset".to_string(),
                    actual: "omitted".to_string(),
                    remediation: "Pass work_id via input.work_id or seed.work_id, \
                                  or use force_gates=true with a reason."
                        .to_string(),
                }],
            ));
        };

        // The Work must exist FOR THIS CREATOR: a foreign or unknown Work is a
        // gate failure, never an empty snapshot that silently passes.
        let pool = self.coordinator().pool();
        let row: Option<WorkSnapshotRow> = sqlx::query_as::<_, WorkSnapshotRow>(
            "SELECT work_profile, work_ref, workspace_slug, intake_status, \
                    world_id, status, current_stage, total_planned_chapters \
             FROM works WHERE work_id = ? AND creator_id = ?",
        )
        .bind(&work_id)
        .bind(&request.creator_id)
        .fetch_optional(pool.as_ref())
        .await
        .map_err(|e| crate::error::db_err(&e))?;

        let Some(row) = row else {
            return Err(gate_failure(
                &request.preset_id,
                &work_id,
                vec![FailedGate {
                    kind: "work_field".to_string(),
                    expected: "work must exist for the owning creator".to_string(),
                    actual: format!("work {work_id} not found for this creator"),
                    remediation: "Create the Work first, then schedule the preset.".to_string(),
                }],
            ));
        };

        let snapshot = WorkSnapshot {
            work_id: work_id.clone(),
            creator_id: request.creator_id.clone(),
            work_profile: row.work_profile,
            work_ref: row.work_ref,
            workspace_slug: row.workspace_slug,
            intake_status: row.intake_status,
            world_id: row.world_id,
            status: row.status,
            current_stage: row.current_stage,
            title: None,
            total_planned_chapters: row.total_planned_chapters,
        };

        let mut vars = std::collections::HashMap::new();
        if let Some(obj) = request.input.as_ref().and_then(|v| v.as_object()) {
            for (k, v) in obj {
                vars.insert(k.clone(), v.to_string());
            }
        }
        if let Some(ref wr) = snapshot.work_ref {
            vars.insert("work_ref".to_string(), wr.clone());
        }
        vars.insert("work_id".to_string(), work_id.clone());
        let preset_input = PresetInput { vars };

        // Filesystem-kind gates resolve against the SAME frozen root the engine
        // and commit authority use; without one there is nothing to resolve
        // against, so fall back to the nexus home exactly as the transport does.
        let workspace_root = self
            .workspace_root()
            .map(std::path::Path::to_path_buf)
            .or_else(|| self.nexus_home().map(std::path::Path::to_path_buf))
            .ok_or_else(|| CoreError::Internal {
                category: "gate evaluation requires a workspace root or nexus home".into(),
            })?;
        let lookup = DbPreviousPresetLookup { pool };

        match evaluate_gates(
            gates,
            &request.preset_id,
            &snapshot,
            &preset_input,
            &workspace_root,
            &lookup,
        )
        .await
        {
            Ok(Ok(())) => Ok(None),
            Ok(Err(failure)) => Err(CoreError::Preset(PresetError::Rejected {
                code: "preset_gates_failed".to_string(),
                message: render_gate_failure(&failure),
            })),
            Err(err) => Err(CoreError::Internal {
                category: format!("gate evaluation error: {err}"),
            }),
        }
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

    /// Resolve the preset (and therefore its gates) through the shared resolver.
    ///
    /// A resolution failure is reported, never swallowed: a preset that cannot
    /// be loaded cannot have its gates evaluated, so it must not be enqueued.
    fn resolve_preset(
        &self,
        preset_id: &str,
    ) -> CoreResult<Option<nexus_preset::LoadedPreset>> {
        // `_system.*` presets are never admitted and carry no gates.
        if preset_id.starts_with("_system.") {
            return Ok(None);
        }
        let home = self.nexus_home().ok_or_else(|| CoreError::Internal {
            category: "schedule insert requires a nexus home to resolve the preset".into(),
        })?;
        let registry = self
            .capability_holder()
            .get()
            .ok_or_else(|| CoreError::Internal {
                category: "capability registry unavailable; cannot resolve preset".into(),
            })?;
        nexus_preset::resolve_preset(preset_id, home, &registry)
            .map(Some)
            .map_err(|e| {
                CoreError::Internal {
                    category: format!("failed to resolve preset '{preset_id}': {e}"),
                }
            })
    }

    /// Freeze a new row's admission payload at insertion.
    ///
    /// The frozen identity is the SAME resolution that supplied the gates. Role
    /// bindings prefer the caller's explicit `agent_bindings` (validated to
    /// cover every prompt role the preset declares) and fall back to the
    /// coordinator's configured default provider — never to request-supplied
    /// catalog order.
    fn freeze_descriptor(
        &self,
        request: &AddScheduleRequest,
        loaded: Option<&nexus_preset::LoadedPreset>,
    ) -> CoreResult<Option<Vec<u8>>> {
        use nexus_orchestration::run_state::{AgentBinding, RunDescriptorV1};

        let Some(loaded) = loaded else {
            // `_system.*`: never admitted, so no descriptor is required.
            return Ok(None);
        };
        let source = loaded
            .source_identity
            .clone()
            .ok_or_else(|| CoreError::Internal {
                category: format!(
                    "preset '{}' has no content-addressed source identity",
                    request.preset_id
                ),
            })?;

        let input = request
            .input
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

        // Explicit request bindings win, but only when they cover the preset's
        // declared prompt roles; a partial map is a refusal, not a silent
        // partial binding.
        let roles = nexus_preset::required_prompt_roles(loaded);
        let agent_bindings = if let Some(explicit) = request.agent_bindings.as_ref() {
            let mut bindings = std::collections::HashMap::with_capacity(explicit.len());
            for (role, dto) in explicit {
                bindings.insert(
                    role.clone(),
                    AgentBinding {
                        provider_id: dto.provider_id.clone(),
                        model: dto.model.clone(),
                    },
                );
            }
            let missing: Vec<String> = roles
                .iter()
                .filter(|role| !bindings.contains_key(*role))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "agent_bindings".into(),
                    reason: format!(
                        "preset '{}' requires bindings for role(s) {}",
                        request.preset_id,
                        missing.join(", ")
                    ),
                });
            }
            bindings
        } else if roles.is_empty() {
            std::collections::HashMap::new()
        } else {
            // Bind the coordinator (and the provider string) before the closure
            // below captures it: `self.coordinator()` returns a temporary `Arc`
            // that would otherwise be dropped at the end of the `let-else`.
            let coordinator = self.coordinator();
            let Some(provider_id) = coordinator.binding_provider() else {
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
                            provider_id: provider_id.to_owned(),
                            model: None,
                        },
                    )
                })
                .collect()
        };

        // work_id resolves from the structured input OR a JSON seed, matching
        // the retained add path: a gated preset whose Work is only named in the
        // seed must still freeze that Work into the row/descriptor.
        let work_id = input
            .get("work_id")
            .and_then(|w| w.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| work_id_from_seed(request.seed.as_deref()));

        let descriptor = RunDescriptorV1 {
            creator_id: request.creator_id.clone(),
            work_id,
            workspace_root: self
                .workspace_root()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default(),
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
    // Unwrap the request's newtypes once, up front: the ring takes `&str` and
    // the response needs the owned id, so binding it here avoids cloning.
    let run_id = String::from(request.run_id);
    let page = port
        .read_page(&run_id, after, limit)
        .map_err(|err| match err {
            PageError::UnknownRun(run_id) => CoreError::NotFound {
                resource: format!("run event ring {run_id}"),
            },
        })?;

    // `kind` is a minLength-1 newtype. The ring only ever writes non-empty
    // event names, so a conversion failure is a ring bug worth surfacing rather
    // than papering over with a fabricated kind.
    let events = page
        .frames
        .into_iter()
        .map(|frame| {
            let kind = CoreRunEventsResponseEventsItemKind::try_from(frame.kind.as_str())
                .map_err(|err| CoreError::Internal {
                    category: format!("run-event kind encode: {err}"),
                })?;
            Ok(CoreRunEventsResponseEventsItem {
                sequence: frame.sequence,
                kind,
                payload: serde_json::from_str::<serde_json::Value>(&frame.data)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default(),
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(CoreRunEventsResponse {
        run_id: CoreRunEventsResponseRunId::try_from(run_id).map_err(|err| {
            CoreError::InvalidInput {
                field: "run_id".into(),
                reason: err.to_string(),
            }
        })?,
        events,
        next_sequence: CoreRunEventsResponseNextSequence::try_from(page.next_sequence.to_string())
            .map_err(|err| CoreError::Internal {
                category: format!("run-event watermark encode: {err}"),
            })?,
        terminal: page.terminal,
        resync_required: page.resync_required,
    })
}

/// Build a durable pending row from the generated add request.
///
/// Every payload-bearing field crosses: creator/preset/label/scheduled_at and
/// the dependency list verbatim, and the request's concurrency declaration
/// (including a `parallel_with` whitelist) maps onto the stored concurrency.
fn build_schedule(schedule_id: &str, request: &AddScheduleRequest) -> Schedule {
    let concurrency = match request.concurrency.as_ref() {
        None | Some(ScheduleConcurrencyRequest::Serial) => ScheduleConcurrency::Serial,
        Some(ScheduleConcurrencyRequest::ParallelAny) => ScheduleConcurrency::ParallelAny,
        Some(ScheduleConcurrencyRequest::ParallelWith { schedule_ids }) => {
            ScheduleConcurrency::ParallelWith(ParallelWithIds {
                schedule_ids: schedule_ids
                    .iter()
                    .map(|id| ScheduleId(id.clone()))
                    .collect(),
            })
        }
    };
    Schedule {
        id: ScheduleId(schedule_id.to_string()),
        creator_id: request.creator_id.clone(),
        preset_id: request.preset_id.clone(),
        preset_version: 1,
        status: ScheduleStatus::Pending,
        concurrency,
        depends_on: request
            .depends_on
            .clone()
            .unwrap_or_default()
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

/// Build a `preset_gates_failed` refusal from the failed-gate list.
fn gate_failure(preset_id: &str, work_id: &str, failed_gates: Vec<FailedGate>) -> CoreError {
    CoreError::Preset(PresetError::Rejected {
        code: "preset_gates_failed".to_string(),
        message: render_gate_failure(&PresetGatesFailed {
            error: "preset_gates_failed".to_string(),
            preset_id: preset_id.to_string(),
            work_id: work_id.to_string(),
            failed_gates,
        }),
    })
}

/// Render a gate failure into the retained human-readable message.
fn render_gate_failure(failure: &PresetGatesFailed) -> String {
    let mut msg = format!("preset gates failed for '{}'", failure.preset_id);
    for gate in &failure.failed_gates {
        msg.push_str(&format!(
            "; {}: expected {}, got {}",
            gate.kind, gate.expected, gate.actual
        ));
    }
    msg
}

/// Strip ANSI escape sequences and control characters from a reason string.
///
/// Mirrors the transport's sanitizer without pulling `regex` into this cohort:
/// the `ESC [ params letter` grammar is matched by hand.
fn sanitize_reason(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while matches!(chars.peek(), Some(d) if d.is_ascii_digit() || *d == ';') {
                    chars.next();
                }
                chars.next(); // the terminating letter
            }
            continue;
        }
        if c.is_control() && c != '\n' {
            continue;
        }
        out.push(c);
    }
    out
}

/// Fresh schedule id in the daemon's retained `SCH<timestamp>` shape.
fn new_schedule_id() -> String {
    format!("SCH{}", chrono::Utc::now().format("%Y%m%d%H%M%S%3f"))
}

/// `work_id` from a JSON seed, mirroring the transport's fallback.
fn work_id_from_seed(seed: Option<&str>) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(seed?)
        .ok()
        .and_then(|v| {
            v.get("work_id")
                .and_then(|w| w.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

/// One Work-snapshot row for gate evaluation.
#[derive(sqlx::FromRow)]
struct WorkSnapshotRow {
    work_profile: Option<String>,
    work_ref: Option<String>,
    workspace_slug: Option<String>,
    intake_status: Option<String>,
    world_id: Option<String>,
    status: Option<String>,
    current_stage: Option<String>,
    total_planned_chapters: Option<i64>,
}

/// `PreviousPreset` gate lookup over the Creator DB.
struct DbPreviousPresetLookup {
    pool: Arc<sqlx::SqlitePool>,
}

impl PreviousPresetLookup for DbPreviousPresetLookup {
    fn lookup(
        &self,
        preset_id: &str,
        work_id: &str,
        _creator_id: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<PreviousPresetResult, GateEvalError>>
                + Send
                + '_,
        >,
    > {
        let pool = Arc::clone(&self.pool);
        let preset_id = preset_id.to_string();
        let work_id = work_id.to_string();
        Box::pin(async move {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM creator_schedules \
                 WHERE preset_id = ? AND status = 'completed' AND work_id = ?",
            )
            .bind(&preset_id)
            .bind(&work_id)
            .fetch_one(pool.as_ref())
            .await
            .map_err(|e| GateEvalError::Database(e.to_string()))?;
            Ok(PreviousPresetResult {
                found: count > 0,
                is_complete: count > 0,
            })
        })
    }
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
        E::InvalidTransition(..) | E::DuplicateSchedule { .. } => CoreError::Busy,
        E::Database(e) => crate::error::db_err(&e),
    }
}
