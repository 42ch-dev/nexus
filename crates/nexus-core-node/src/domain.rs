//! World / Work / content / knowledge native family surface (P5-T1).
//!
//! Every method is a thin napi adapter: an owned JSON payload is parsed into a
//! generated wire DTO, the stored [`Principal`] is minted natively from the
//! opened core (never accepted as identity from JS — the handle argument is an
//! opaque token the native side re-derives and compares), the single
//! [`nexus_core::CoreService`] authority owns the effect, and the result is
//! serialized back as the schema-owned wire shape. Conversions here are
//! projection/carrier work only; no SQL, no second engine, no policy.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::{
    AddKbEntryRequest, AddKbEntryResponse, AppendInspirationRequest, AppendInspirationResponse,
    BatchUpdateFindingsRequest, BatchUpdateFindingsResponse, ChapterBody, ChapterDetail,
    ChapterOutline, CoreChapterContentQuery as WireChapterContentQuery,
    CoreTimelineEventsQuery as WireTimelineEventsQuery,
    CoreTimelineOverviewQuery as WireTimelineOverviewQuery, CoreWorkSelection, CreateForkRequest,
    CreateForkResponse, CreateWorkRequest, CreateWorkResponse, DeleteKbEntryResponse,
    FindingDetailResponse, GetKbEntryResponse, ListChaptersQuery, ListKbEntriesQuery,
    ListWorksQuery, ListWorksResponse, PackExportRequest, PackImportRequest, PatchChapterRequest,
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationListQuery,
    ReadingAnnotationListResponse, ReadingAnnotationPatchRequest, ReadingProgressQuery,
    ReadingProgressRequest, ReadingProgressResponse, TimelineOverviewResponse, WorkDetailResponse,
    WorkInspirationAddRequest, WorkInspirationAddResponse, WorkInspirationArchiveRequest,
    WorkInspirationItem, WorkInspirationListQuery, WorkInspirationListResponse,
    WorkInspirationPromoteRequest, WorkInspirationPromoteResponse,
    WorkPoolArchiveRequest as WirePoolArchiveRequest, WorkPoolArchiveRequest, WorkPoolEntry,
    WorkPoolListQuery, WorkPoolListResponse, WorkPoolPromoteRequest, WorkPoolSetActiveRequest,
    WorkReconcileReport,
};
use nexus_core::{
    AddInspirationRequest, ArchiveInspirationRequest, ArchivePoolRequest, CoreChapterContentQuery,
    CoreError, CoreService, CoreTimelineEventsQuery, CoreTimelineOverviewQuery,
    CreateFindingRequest as DomainCreateFindingRequest,
    ListFindingsQuery as DomainListFindingsQuery, ListInspirationQuery, ListPoolQuery, Principal,
    PromoteInspirationRequest, PromotePoolRequest, ReconcileDryRunQuery, SetPoolActiveRequest,
    UpdateFindingRequest as DomainUpdateFindingRequest, WorkPatchRequest,
    WorkReconcileReport as DomainWorkReconcileReport,
};

use crate::NativeCore;

/// Per-Work lock holder label for this surface. The daemon HTTP adapters pass
/// the same literal, so a 423 `Locked.reason` is byte-identical across the
/// legacy and standalone surfaces (`cli:` stays the CLI's own label).
const HTTP_HOLDER: &str = "http";

/// Parse an owned wire payload into a generated DTO. Parse failures surface as
/// `invalid <label>: …` rejections, which the TS error mapper classifies as
/// 400 `invalid_input` (client error, not a fault).
fn decode<T: serde::de::DeserializeOwned>(payload: Buffer, label: &str) -> Result<T> {
    serde_json::from_slice(payload.as_ref())
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))
}

/// Optional bounded query parameter. Overflow/out-of-range is a client error
/// (400 `invalid_input` via the "invalid …" rejection convention), never a
/// silently-absent filter.
fn query_u32_bounded<T>(value: Option<T>, field: &str) -> Result<Option<u32>>
where
    T: Copy + TryInto<u32>,
    T::Error: std::fmt::Display,
{
    value
        .map(|v| {
            v.try_into()
                .map_err(|e| Error::from_reason(format!("invalid {field}: {e}")))
        })
        .transpose()
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Decode a tri-state nullable patch field out of the generated carrier.
///
/// The wire schemas mark these fields `x-nexus-tri-state`; the generator adds
/// a presence-preserving deserializer, so an explicit `null` arrives as
/// `Some(Value::Null)` instead of collapsing into `None`. The three states
/// therefore survive the single generated-DTO parse: absent → `None` (keep),
/// `null` → `Some(None)` (clear), string → `Some(Some(_))` (set). Anything
/// else is a 400 before any stored effect. Regression:
/// `crates/nexus-contracts/tests/tri_state_presence.rs`.
fn tri_state_string(
    value: Option<serde_json::Value>,
    field: &str,
) -> Result<Option<Option<String>>> {
    match value {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(Some(None)),
        Some(serde_json::Value::String(text)) => Ok(Some(Some(text))),
        Some(other) => Err(Error::from_reason(format!(
            "invalid {field}: expected a string, null, or omission, got {}",
            json_kind(&other)
        ))),
    }
}

#[napi]
impl NativeCore {
    // ── World lifecycle (P0-T1 authority) ───────────────────────────────────

    /// `GET /v1/daemon/narrative/worlds` — read-model World list.
    #[napi]
    pub async fn narrative_list_worlds(&self, principal_handle: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let worlds = core.list_worlds(&principal).await?;
            Ok(serde_json::json!({ "worlds": worlds }))
        })
        .await
    }

    /// `GET /v1/daemon/narrative/worlds/{world_id}` — single World read model.
    #[napi]
    pub async fn narrative_get_world(
        &self,
        principal_handle: String,
        world_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let world = core.get_world(&principal, world_id).await?;
            Ok(serde_json::json!({ "world": world }))
        })
        .await
    }

    /// `POST /v1/daemon/worlds` — create a World (201 at the adapter).
    #[napi]
    pub async fn create_world(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::CreateWorldRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.create_world(&principal, request).await
        })
        .await
    }

    /// `DELETE /v1/daemon/worlds/{world_id}` — hard delete (204 at the
    /// adapter); blocked bindings ride the core marker.
    #[napi]
    pub async fn delete_world(&self, principal_handle: String, world_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_world(&principal, world_id).await?;
            Ok(())
        })
        .await
    }

    // ── World KB mutations + projection (P0-T1) ─────────────────────────────

    /// `POST /v1/daemon/worlds/{world_id}/kb/promote-candidate`.
    #[napi]
    pub async fn promote_world_kb_candidate(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::WorldKbPromoteCandidateRequest =
            decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.promote_world_kb_candidate(&principal, world_id, request)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/worlds/{world_id}/kb/patch-relationship`.
    #[napi]
    pub async fn patch_world_kb_relationship(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::WorldKbPatchRelationshipRequest =
            decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_world_kb_relationship(&principal, world_id, request)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/worlds/{world_id}/kb/key-blocks/{key_block_id}/state`.
    #[napi]
    pub async fn world_kb_key_block_state(
        &self,
        principal_handle: String,
        world_id: String,
        key_block_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.world_kb_key_block_state(&principal, world_id, key_block_id)
                .await
        })
        .await
    }

    // ── Fork, pack, rules, world findings (P0-T2) ───────────────────────────

    /// `POST /v1/daemon/worlds/{world_id}/forks`.
    #[napi]
    pub async fn create_world_fork(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CreateForkRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.create_fork(&principal, world_id, request).await
        })
        .await
    }

    /// `POST /v1/daemon/worlds/{world_id}/kb/pack/export`.
    #[napi]
    pub async fn export_world_pack(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: PackExportRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.export_world_pack(&principal, world_id, request).await
        })
        .await
    }

    /// `POST /v1/daemon/worlds/{world_id}/kb/pack/import` (conflict policy,
    /// dry-run preview, id remapping and provenance stay core-owned).
    #[napi]
    pub async fn import_world_pack(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: PackImportRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.import_world_pack(&principal, world_id, request).await
        })
        .await
    }

    /// `GET /v1/daemon/worlds/{world_id}/rules`.
    #[napi]
    pub async fn list_world_rules(
        &self,
        principal_handle: String,
        world_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.list_world_rules(&principal, world_id).await
        })
        .await
    }

    /// `POST /v1/daemon/worlds/{world_id}/rules` (201 at the adapter).
    #[napi]
    pub async fn create_world_rule(
        &self,
        principal_handle: String,
        world_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::WorldRuleCreateRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.create_world_rule(&principal, world_id, request).await
        })
        .await
    }

    /// `PATCH /v1/daemon/worlds/{world_id}/rules/{rule_id}`.
    #[napi]
    pub async fn update_world_rule(
        &self,
        principal_handle: String,
        world_id: String,
        rule_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::WorldRuleUpdateRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.update_world_rule(&principal, world_id, rule_id, request)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/worlds/{world_id}/findings`.
    #[napi]
    pub async fn list_world_findings(
        &self,
        principal_handle: String,
        world_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.list_world_findings(&principal, world_id).await
        })
        .await
    }

    // ── Timeline reads (P0-T3) ──────────────────────────────────────────────

    /// `GET /v1/daemon/timeline/overview?cursor=`.
    #[napi]
    pub async fn timeline_overview(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireTimelineOverviewQuery = decode(query_json, "query")?;
        // The wire cursor is a length-bounded newtype; the domain carrier is
        // the plain string. Re-serialize through `to_string` (never a lossy
        // debug print).
        let domain = CoreTimelineOverviewQuery {
            cursor: query.cursor.map(|cursor| cursor.to_string()),
        };
        self.json_call(principal_handle, async move |core, principal| {
            let response: TimelineOverviewResponse =
                core.timeline_overview(&principal, domain).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/worlds/{world_id}/timeline/events`.
    #[napi]
    pub async fn list_timeline_events(
        &self,
        principal_handle: String,
        world_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireTimelineEventsQuery = decode(query_json, "query")?;
        let domain = CoreTimelineEventsQuery {
            branch_id: query.branch_id.map(|branch| branch.to_string()),
            status: query.status.map(|status| status.to_string()),
            event_type: query.event_type,
            limit: query
                .limit
                .map(|limit| u32::try_from(u64::from(limit)).unwrap_or(u32::MAX)),
            cursor: query.cursor.map(|cursor| cursor.to_string()),
        };
        self.json_call(principal_handle, async move |core, principal| {
            core.list_timeline_events(&principal, world_id, domain)
                .await
        })
        .await
    }

    // ── Work lifecycle (P1-T1 authority) ────────────────────────────────────

    /// `GET /v1/daemon/works` — generated list envelope straight through.
    #[napi]
    pub async fn list_works(&self, principal_handle: String, query_json: Buffer) -> Result<Buffer> {
        let query: ListWorksQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: ListWorksResponse = core.list_works(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}` — Work detail projection.
    #[napi]
    pub async fn get_work(&self, principal_handle: String, work_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let details = core.get_work(&principal, work_id).await?;
            Ok(work_detail_wire(details))
        })
        .await
    }

    /// `POST /v1/daemon/works` — returns `[created, response]`; the adapter
    /// maps the flag onto the retained 201 (new) / 200 (idempotent replay).
    #[napi]
    pub async fn create_work(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CreateWorkRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let (created, response) = core.create_work_with_outcome(&principal, request).await?;
            Ok(serde_json::json!([created, response]))
        })
        .await
    }

    /// `PATCH /v1/daemon/works/{work_id}` — tri-state binding patch.
    #[napi]
    pub async fn patch_work(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::PatchWorkRequest = decode(request_json, "request")?;
        let domain = WorkPatchRequest {
            title: request.title,
            long_term_goal: request.long_term_goal,
            creative_brief: request.creative_brief,
            intake_status: request.intake_status,
            status: request.status,
            world_id: tri_state_string(request.world_id, "world_id")?,
            story_ref: tri_state_string(request.story_ref, "story_ref")?,
            primary_preset_id: request.primary_preset_id,
            current_stage: request.current_stage,
            stage_status: request.stage_status,
            force: request.force,
            auto_review_master_on_timeout: request.auto_review_master_on_timeout,
            auto_chain_interrupted: request.auto_chain_interrupted,
            work_profile: request.work_profile,
            // The schedule-id list is the host-tool seam (`nexus.work.schedule.set`);
            // no HTTP caller supplies it, and the core treats `None` as "untouched".
            schedule_ids: None,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let details = core
                .patch_work(&principal, work_id, HTTP_HOLDER, domain)
                .await?;
            Ok(work_detail_wire(details))
        })
        .await
    }

    /// `DELETE /v1/daemon/works/{work_id}` (204 at the adapter).
    #[napi]
    pub async fn delete_work(&self, principal_handle: String, work_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_work(&principal, work_id, HTTP_HOLDER).await?;
            Ok(())
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/inspiration`.
    #[napi]
    pub async fn append_work_inspiration(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: AppendInspirationRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let result = core
                .append_work_inspiration(&principal, work_id, HTTP_HOLDER, request)
                .await?;
            let response: AppendInspirationResponse = AppendInspirationResponse {
                work_id: result.work_id,
                inspiration_count: result.inspiration_count.max(0),
            };
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/completion-lock/release`.
    #[napi]
    pub async fn release_work_completion_lock(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::ReleaseCompletionLockRequest =
            decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let details = core
                .release_work_completion_lock(&principal, work_id, request)
                .await?;
            Ok(work_detail_wire(details))
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/reconcile-chapters?dry_run=`.
    #[napi]
    pub async fn reconcile_work_chapters(
        &self,
        principal_handle: String,
        work_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ReconcileQuery {
            dry_run: Option<bool>,
        }
        let query: ReconcileQuery = decode(query_json, "query")?;
        let domain = ReconcileDryRunQuery {
            dry_run: query.dry_run,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let report: DomainWorkReconcileReport = core
                .reconcile_work_chapters(&principal, work_id, HTTP_HOLDER, domain)
                .await?;
            let wire: WorkReconcileReport = WorkReconcileReport {
                created: u64::from(report.created),
                updated: u64::from(report.updated),
                resynced: u64::from(report.resynced),
                preserved: u64::from(report.preserved),
            };
            Ok(wire)
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/use`-equivalent durable selection —
    /// the pool-active mutation the Work surface exposes over HTTP is
    /// `set_pool_active`; `select_work` is the same persisted selection seam
    /// the CLI `works use` path uses.
    #[napi]
    pub async fn select_work(&self, principal_handle: String, work_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let selection: CoreWorkSelection = core.select_work(&principal, work_id).await?;
            Ok(selection)
        })
        .await
    }

    // ── Authoring pool + inspiration (P1-T1) ────────────────────────────────

    /// `GET /v1/daemon/works/pool`.
    #[napi]
    pub async fn list_work_pool(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WorkPoolListQuery = decode(query_json, "query")?;
        let domain = ListPoolQuery {
            status: query.status,
            limit: query_u32_bounded(query.limit, "limit")?,
            offset: query_u32_bounded(query.offset, "offset")?,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let page = core.list_work_pool(&principal, domain).await?;
            let entries: Vec<nexus_contracts::generated::core::works::work_pool_list_response::WorkPoolEntry> =
                page.entries.into_iter().map(|e| nexus_contracts::generated::core::works::work_pool_list_response::WorkPoolEntry {
                    entry_id: e.entry_id,
                    work_id: e.work_id,
                    status: e.status,
                    title: e.title,
                    promoted_at: e.promoted_at,
                    note: e.note,
                })
                .collect();
            let wire: WorkPoolListResponse = WorkPoolListResponse {
                entries,
                total: u64::from(page.total),
                limit: u64::from(page.limit),
                offset: u64::from(page.offset),
            };
            Ok(wire)
        })
        .await
    }

    /// `POST /v1/daemon/works/pool` — durable pool-active selection.
    #[napi]
    pub async fn set_work_pool_active(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorkPoolSetActiveRequest = decode(request_json, "request")?;
        let domain = SetPoolActiveRequest {
            action: request.action,
            work_id: request.work_id.to_string(),
            creator_id: request.creator_id,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let entry = core.set_work_pool_active(&principal, domain).await?;
            Ok(pool_entry_wire(entry))
        })
        .await
    }

    /// `POST /v1/daemon/works/pool/promote`.
    #[napi]
    pub async fn promote_work_pool_entry(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorkPoolPromoteRequest = decode(request_json, "request")?;
        let domain = PromotePoolRequest {
            work_id: request.work_id.to_string(),
            set_default: request.set_default,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let entry = core.promote_work_pool_entry(&principal, domain).await?;
            Ok(pool_entry_wire(entry))
        })
        .await
    }

    /// `POST /v1/daemon/works/pool/archive`.
    #[napi]
    pub async fn archive_work_pool_entry(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WirePoolArchiveRequest = decode(request_json, "request")?;
        let domain = ArchivePoolRequest {
            entry_id: request.entry_id.to_string(),
        };
        self.json_call(principal_handle, async move |core, principal| {
            let entry = core.archive_work_pool_entry(&principal, domain).await?;
            Ok(pool_entry_wire(entry))
        })
        .await
    }

    /// `POST /v1/daemon/works/pool/inspiration` (201 at the adapter).
    #[napi]
    pub async fn add_work_inspiration(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorkInspirationAddRequest = decode(request_json, "request")?;
        let domain = AddInspirationRequest {
            title: request.title.to_string(),
        };
        self.json_call(principal_handle, async move |core, principal| {
            let added = core.add_work_inspiration(&principal, domain).await?;
            let wire: WorkInspirationAddResponse = WorkInspirationAddResponse {
                item_id: added.item_id,
                rel_path: added.rel_path,
            };
            Ok(wire)
        })
        .await
    }

    /// `GET /v1/daemon/works/pool/inspiration`.
    #[napi]
    pub async fn list_work_inspiration(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WorkInspirationListQuery = decode(query_json, "query")?;
        let domain = ListInspirationQuery {
            status: query.status,
            limit: query_u32_bounded(query.limit, "limit")?,
            offset: query_u32_bounded(query.offset, "offset")?,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let page = core.list_work_inspiration(&principal, domain).await?;
            let items: Vec<nexus_contracts::generated::core::works::work_inspiration_list_response::WorkInspirationItem> = page
                .items
                .into_iter()
                .map(|item| nexus_contracts::generated::core::works::work_inspiration_list_response::WorkInspirationItem {
                    item_id: item.item_id,
                    rel_path: item.rel_path,
                    title: item.title,
                    status: item.status,
                    promoted_work_id: item.promoted_work_id,
                    created_at: item.created_at,
                    promoted_at: item.promoted_at,
                })
                .collect();
            let wire: WorkInspirationListResponse = WorkInspirationListResponse {
                items,
                total: u64::from(page.total),
                limit: u64::from(page.limit),
                offset: u64::from(page.offset),
            };
            Ok(wire)
        })
        .await
    }

    /// `POST /v1/daemon/works/pool/inspiration/promote` (atomic three-write
    /// transaction stays core-owned).
    #[napi]
    pub async fn promote_work_inspiration(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorkInspirationPromoteRequest = decode(request_json, "request")?;
        let domain = PromoteInspirationRequest {
            item_id: request.item_id.to_string(),
            idea: request.idea,
            set_default: request.set_default,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let promoted = core.promote_work_inspiration(&principal, domain).await?;
            let wire: WorkInspirationPromoteResponse = WorkInspirationPromoteResponse {
                work_id: promoted.work_id,
                pool_entry_id: promoted.pool_entry_id,
            };
            Ok(wire)
        })
        .await
    }

    /// `POST /v1/daemon/works/pool/inspiration/archive`.
    #[napi]
    pub async fn archive_work_inspiration(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: WorkInspirationArchiveRequest = decode(request_json, "request")?;
        let domain = ArchiveInspirationRequest {
            item_id: request.item_id.to_string(),
        };
        self.json_call(principal_handle, async move |core, principal| {
            let item = core.archive_work_inspiration(&principal, domain).await?;
            let wire: WorkInspirationItem = WorkInspirationItem {
                item_id: item.item_id,
                rel_path: item.rel_path,
                title: item.title,
                status: item.status,
                promoted_work_id: item.promoted_work_id,
                created_at: item.created_at,
                promoted_at: item.promoted_at,
            };
            Ok(wire)
        })
        .await
    }

    // ── Chapter / outline content (P1-T2) ───────────────────────────────────

    /// `GET /v1/daemon/works/{work_id}/chapters/`.
    #[napi]
    pub async fn list_chapters(
        &self,
        principal_handle: String,
        work_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListChaptersQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_chapters(&principal, work_id, query).await
        })
        .await
    }

    fn chapter_query(query: WireChapterContentQuery) -> CoreChapterContentQuery {
        CoreChapterContentQuery {
            volume: query.volume,
        }
    }

    /// `GET /v1/daemon/works/{work_id}/chapters/{n}`.
    #[napi]
    pub async fn chapter_detail(
        &self,
        principal_handle: String,
        work_id: String,
        chapter_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireChapterContentQuery = decode(query_json, "query")?;
        let query = Self::chapter_query(query);
        self.json_call(principal_handle, async move |core, principal| {
            let detail: ChapterDetail = core
                .chapter_detail(&principal, work_id, chapter_id, query)
                .await?;
            Ok(detail)
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}/chapters/{n}/outline`.
    #[napi]
    pub async fn chapter_outline(
        &self,
        principal_handle: String,
        work_id: String,
        chapter_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireChapterContentQuery = decode(query_json, "query")?;
        let query = Self::chapter_query(query);
        self.json_call(principal_handle, async move |core, principal| {
            let outline: ChapterOutline = core
                .chapter_outline(&principal, work_id, chapter_id, query)
                .await?;
            Ok(outline)
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}/chapters/{n}/body` — raw content
    /// download stays the JSON `ChapterBody` envelope (markdown inside).
    #[napi]
    pub async fn chapter_body(
        &self,
        principal_handle: String,
        work_id: String,
        chapter_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireChapterContentQuery = decode(query_json, "query")?;
        let query = Self::chapter_query(query);
        self.json_call(principal_handle, async move |core, principal| {
            let body: ChapterBody = core
                .chapter_body(&principal, work_id, chapter_id, query)
                .await?;
            Ok(body)
        })
        .await
    }

    /// `PATCH /v1/daemon/works/{work_id}/chapters/{n}`.
    #[napi]
    pub async fn patch_chapter(
        &self,
        principal_handle: String,
        work_id: String,
        chapter_id: String,
        query_json: Buffer,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let query: WireChapterContentQuery = decode(query_json, "query")?;
        let query = Self::chapter_query(query);
        let request: PatchChapterRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let detail: ChapterDetail = core
                .patch_chapter(&principal, HTTP_HOLDER, work_id, chapter_id, query, request)
                .await?;
            Ok(detail)
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}/outline`.
    #[napi]
    pub async fn get_work_outline(
        &self,
        principal_handle: String,
        work_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.work_outline(&principal, work_id).await
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/outline/patch`.
    #[napi]
    pub async fn patch_outline_structure(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::OutlinePatchStructureRequest =
            decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_outline_structure(&principal, HTTP_HOLDER, work_id, request)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/chapters/{n}/patch`.
    #[napi]
    pub async fn patch_outline_chapter(
        &self,
        principal_handle: String,
        work_id: String,
        chapter_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::OutlinePatchChapterRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_outline_chapter(&principal, HTTP_HOLDER, work_id, chapter_id, request)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/timeline/patch`.
    #[napi]
    pub async fn patch_timeline_event(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::TimelinePatchEventRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_timeline_event(&principal, HTTP_HOLDER, work_id, request)
                .await
        })
        .await
    }

    // ── Work KB index (P1-T3) ───────────────────────────────────────────────

    /// `GET /v1/daemon/kb/entries` — local work-scope file index (`scope=work`).
    #[napi]
    pub async fn list_kb_entries(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListKbEntriesQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_kb_entries(&principal, query).await
        })
        .await
    }

    /// `POST /v1/daemon/kb/entries` (201 at the adapter).
    #[napi]
    pub async fn add_kb_entry(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: AddKbEntryRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.add_kb_entry(&principal, request).await
        })
        .await
    }

    /// `GET /v1/daemon/kb/entries/{entry_id}`.
    #[napi]
    pub async fn get_kb_entry(&self, principal_handle: String, entry_id: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let response: GetKbEntryResponse = core.get_kb_entry(&principal, entry_id).await?;
            Ok(response)
        })
        .await
    }

    /// `DELETE /v1/daemon/kb/entries/{entry_id}`.
    #[napi]
    pub async fn delete_kb_entry(
        &self,
        principal_handle: String,
        entry_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_kb_entry(&principal, entry_id).await
        })
        .await
    }

    // ── Findings (P1-T3) ────────────────────────────────────────────────────

    /// Map a generated create-finding wire body onto the core carrier,
    /// applying the legacy wire defaults (`target_executor` = `none`,
    /// `kind` = `craft`).
    fn finding_create(
        request: nexus_contracts::CreateFindingRequest,
    ) -> Result<DomainCreateFindingRequest> {
        Ok(DomainCreateFindingRequest {
            chapter: request.chapter,
            severity: request.severity,
            title: request.title,
            description: request.description.unwrap_or_default(),
            target_executor: request
                .target_executor
                .unwrap_or_else(|| "none".to_string()),
            kind: request.kind.unwrap_or_else(|| "craft".to_string()),
            rule_suggestion: request.rule_suggestion,
        })
    }

    /// `POST /v1/daemon/works/{work_id}/findings` (201 at the adapter).
    #[napi]
    pub async fn create_finding(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::CreateFindingRequest = decode(request_json, "request")?;
        let domain = Self::finding_create(request)?;
        self.json_call(principal_handle, async move |core, principal| {
            let finding: FindingDetailResponse =
                core.create_finding(&principal, work_id, domain).await?;
            Ok(finding)
        })
        .await
    }

    /// `POST /v1/daemon/works/{work_id}/findings/from-review`.
    #[napi]
    pub async fn create_finding_from_review(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::CreateFindingRequest = decode(request_json, "request")?;
        let domain = Self::finding_create(request)?;
        self.json_call(principal_handle, async move |core, principal| {
            let finding: FindingDetailResponse = core
                .create_finding_from_review(&principal, work_id, domain)
                .await?;
            Ok(finding)
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}/findings` — cursor-paginated page.
    #[napi]
    pub async fn list_findings(
        &self,
        principal_handle: String,
        work_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: nexus_contracts::ListFindingsQuery = decode(query_json, "query")?;
        let domain = DomainListFindingsQuery {
            chapter: query.chapter,
            status: query.status,
            severity: query.severity,
            limit: query_u32_bounded(query.limit, "limit")?,
            cursor: query.cursor,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let page = core.list_findings(&principal, work_id, domain).await?;
            // The core page carries the same generated item/pagination types;
            // only the carrier struct is non-Serializable.
            Ok(serde_json::json!({
                "items": page.items,
                "pagination": page.pagination,
            }))
        })
        .await
    }

    /// `GET /v1/daemon/works/{work_id}/findings/{finding_id}`.
    #[napi]
    pub async fn get_work_finding(
        &self,
        principal_handle: String,
        work_id: String,
        finding_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let finding: FindingDetailResponse = core
                .get_work_finding(&principal, work_id, finding_id)
                .await?;
            Ok(finding)
        })
        .await
    }

    /// `GET /v1/daemon/findings/{finding_id}` — creator-scoped lookup.
    #[napi]
    pub async fn get_finding(
        &self,
        principal_handle: String,
        finding_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let finding: FindingDetailResponse = core.get_finding(&principal, finding_id).await?;
            Ok(finding)
        })
        .await
    }

    /// `PATCH /v1/daemon/works/{work_id}/findings/{finding_id}` — tri-state
    /// `rule_suggestion` (R-V1190-FINDINGS-TRISTATE-DUP): absent keeps the
    /// stored column, `null` clears it, a string sets it.
    #[napi]
    pub async fn update_finding(
        &self,
        principal_handle: String,
        finding_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: nexus_contracts::UpdateFindingRequest = decode(request_json, "request")?;
        let domain = DomainUpdateFindingRequest {
            severity: request.severity,
            status: request.status,
            title: request.title,
            description: request.description,
            target_executor: request.target_executor,
            kind: request.kind,
            rule_suggestion: tri_state_string(request.rule_suggestion, "rule_suggestion")?,
        };
        self.json_call(principal_handle, async move |core, principal| {
            let finding: FindingDetailResponse =
                core.update_finding(&principal, finding_id, domain).await?;
            Ok(finding)
        })
        .await
    }

    /// `DELETE /v1/daemon/works/{work_id}/findings/{finding_id}` (204).
    #[napi]
    pub async fn delete_finding(
        &self,
        principal_handle: String,
        finding_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_finding(&principal, finding_id).await?;
            Ok(())
        })
        .await
    }

    /// `PATCH /v1/daemon/findings/batch`.
    #[napi]
    pub async fn batch_update_findings(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: BatchUpdateFindingsRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: BatchUpdateFindingsResponse =
                core.batch_update_findings(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/findings/stale?threshold_seconds=` — the threshold
    /// resolution stays at the adapter (daemon env default 96h), matching the
    /// legacy surface.
    #[napi]
    pub async fn list_stale_findings(
        &self,
        principal_handle: String,
        threshold_seconds: i64,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let report = core
                .list_stale_findings(&principal, threshold_seconds)
                .await?;
            // StaleFindingEntry carries no Serialize: map field-wise.
            let findings: Vec<serde_json::Value> = report
                .findings
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "finding_id": f.finding_id,
                        "work_id": f.work_id,
                        "severity": f.severity,
                        "created_at": f.created_at,
                        "age_seconds": f.age_seconds,
                    })
                })
                .collect();
            Ok(serde_json::json!({
                "stale_count": report.stale_count,
                "threshold_seconds": report.threshold_seconds,
                "now_epoch": report.now_epoch,
                "findings": findings,
            }))
        })
        .await
    }

    /// `POST /v1/daemon/findings/prune?older_than_days=&dry_run=`.
    #[napi]
    pub async fn prune_findings(
        &self,
        principal_handle: String,
        older_than_days: Option<i64>,
        dry_run: bool,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let outcome = core
                .prune_findings(&principal, older_than_days, dry_run)
                .await?;
            Ok(serde_json::json!({
                "count": outcome.count,
                "older_than_days": outcome.older_than_days,
                "dry_run": outcome.dry_run,
                "now_epoch": outcome.now_epoch,
            }))
        })
        .await
    }

    // ── Reading depth (P1-T3) ───────────────────────────────────────────────

    /// `GET /v1/daemon/reading/progress?work_id=&chapter=`.
    #[napi]
    pub async fn get_reading_progress(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ReadingProgressQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: ReadingProgressResponse =
                core.get_reading_progress(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `PUT /v1/daemon/reading/progress`.
    #[napi]
    pub async fn put_reading_progress(
        &self,
        principal_handle: String,
        work_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ReadingProgressRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: ReadingProgressResponse = core
                .put_reading_progress(&principal, work_id, request)
                .await?;
            Ok(response)
        })
        .await
    }

    /// `DELETE /v1/daemon/reading/progress?work_id=&chapter=` (204).
    #[napi]
    pub async fn delete_reading_progress(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ReadingProgressQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_reading_progress(&principal, query).await?;
            Ok(())
        })
        .await
    }

    /// `GET /v1/daemon/reading/annotations?work_id=&chapter=`.
    #[napi]
    pub async fn list_annotations(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ReadingAnnotationListQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: ReadingAnnotationListResponse =
                core.list_annotations(&principal, query).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/reading/annotations` (201 at the adapter).
    #[napi]
    pub async fn create_annotation(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ReadingAnnotationCreateRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let annotation: ReadingAnnotation = core.create_annotation(&principal, request).await?;
            Ok(annotation)
        })
        .await
    }

    /// `PATCH /v1/daemon/reading/annotations/{annotation_id}` — nullable
    /// `note` keeps explicit-null clear semantics via the generated carrier.
    #[napi]
    pub async fn patch_annotation(
        &self,
        principal_handle: String,
        annotation_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ReadingAnnotationPatchRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let annotation: ReadingAnnotation = core
                .patch_annotation(&principal, annotation_id, request)
                .await?;
            Ok(annotation)
        })
        .await
    }

    /// `DELETE /v1/daemon/reading/annotations/{annotation_id}` (204).
    #[napi]
    pub async fn delete_annotation(
        &self,
        principal_handle: String,
        annotation_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_annotation(&principal, annotation_id).await?;
            Ok(())
        })
        .await
    }

    // ── Reference registry reads (P1-T3) ────────────────────────────────────

    /// `GET /v1/daemon/references` — the core projection serializes to the
    /// schema-owned `{references: [...]}` envelope.
    #[napi]
    pub async fn list_references(&self, principal_handle: String) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.list_references(&principal).await
        })
        .await
    }

    /// `GET /v1/daemon/references/{reference_id}`.
    #[napi]
    pub async fn get_reference(
        &self,
        principal_handle: String,
        reference_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.get_reference(&principal, reference_id).await
        })
        .await
    }
}

/// Work detail wire projection. Field-for-field with the schema-owned
/// `WorkDetailResponse`; the core carrier widens `i32` counters to the wire
/// `i64` and the optional chapter list serializes as a (possibly empty) array.
fn work_detail_wire(details: nexus_core::WorkDetails) -> WorkDetailResponse {
    WorkDetailResponse {
        work_id: details.work_id,
        status: details.status,
        title: details.title,
        long_term_goal: details.long_term_goal,
        initial_idea: details.initial_idea,
        creative_brief: details.creative_brief,
        intake_status: details.intake_status,
        world_id: details.world_id,
        story_ref: details.story_ref,
        inspiration_log: details.inspiration_log,
        primary_preset_id: details.primary_preset_id,
        schedule_ids: details.schedule_ids,
        created_at: details.created_at,
        updated_at: details.updated_at,
        current_stage: details.current_stage,
        stage_status: details.stage_status,
        work_profile: details.work_profile,
        work_ref: details.work_ref,
        total_planned_chapters: details.total_planned_chapters.map(i64::from),
        current_chapter: i64::from(details.current_chapter),
        chapters: details.chapters.unwrap_or_default(),
        next_chapter: details.next_chapter.map(i64::from),
        next_chapter_volume: details.next_chapter_volume.map(i64::from),
        auto_chain_enabled: details.auto_chain_enabled,
        driver_schedule_id: details.driver_schedule_id,
        auto_chain_interrupted: details.auto_chain_interrupted,
        auto_review_master_on_timeout: details.auto_review_master_on_timeout,
        runtime_lock_holder: details.runtime_lock_holder,
        runtime_lock_acquired_at: details.runtime_lock_acquired_at,
        completion_locked_at: details.completion_locked_at,
        novel_completion_status: details.novel_completion_status,
        lineage_from_work_id: details.lineage_from_work_id,
    }
}

/// Pool entry wire projection: the stored `creator_id` never crosses the wire
/// (R-V141P1-11 — local-first, always the active creator).
fn pool_entry_wire(entry: nexus_core::WorkPoolEntry) -> WorkPoolEntry {
    WorkPoolEntry {
        entry_id: entry.entry_id,
        work_id: entry.work_id,
        status: entry.status,
        title: entry.title,
        promoted_at: entry.promoted_at,
        note: entry.note,
    }
}
