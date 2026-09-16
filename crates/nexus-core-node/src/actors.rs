//! Actor / memory / context native family surface (P5-T2).
//!
//! Every method is a thin napi adapter mirroring `domain.rs` (P5-T1): an owned
//! JSON payload is parsed into a generated wire DTO, the stored [`Principal`]
//! is minted natively via [`NativeCore::json_call`] (never accepted from JS),
//! the single [`nexus_core::CoreService`] (or the pre-selection
//! [`nexus_core::CoreHomeService`] for the Creator family) owns the effect,
//! and the result is serialized back as the schema-owned wire shape. The
//! response-scoped module paths (`daemon_api::characters::…`) disambiguate the
//! inline repeated wire types exactly like the daemon handlers. No SQL, no
//! second engine, no policy.

use napi::bindgen_prelude::*;
use napi_derive::napi;
use nexus_contracts::daemon_api::actor_knowledge::{
    add_knowledge_entry_request::AddKnowledgeEntryRequest,
    add_knowledge_entry_response::{
        AddKnowledgeEntryResponse, NexusActorKnowledgeViewItem as CreatedItem,
    },
    knowledge_entry_detail::{KnowledgeEntryDetail, NexusActorKnowledgeViewItem as DetailItem},
    knowledge_view_item::KnowledgeViewItem,
    list_character_knowledge_query::ListCharacterKnowledgeQuery,
    list_character_knowledge_response::{
        ListCharacterKnowledgeResponse, NexusActorKnowledgeViewItem as ListedItem,
        NexusPaginationInfo as ListedPagination,
    },
    update_knowledge_entry_request::UpdateKnowledgeEntryRequest,
    view_request::{NexusActorRef, ViewRequest},
    view_response::{
        NexusActorKnowledgeViewItem as ViewItem, NexusPaginationInfo as ViewPagination,
        ViewResponse,
    },
};
use nexus_contracts::daemon_api::characters::memory::{
    capture_character_pending_review_request::CaptureCharacterPendingReviewRequest,
    count_character_pending_reviews_query::CountCharacterPendingReviewsQuery,
    list_character_memory_fragments_query::ListCharacterMemoryFragmentsQuery,
    list_character_pending_reviews_query::ListCharacterPendingReviewsQuery,
    promote_character_fragment_request::PromoteCharacterFragmentRequest,
    review_character_memory_request::ReviewCharacterMemoryRequest,
};
use nexus_contracts::daemon_api::characters::soul::character_soul_narrative_request::CharacterSoulNarrativeRequest;
use nexus_contracts::daemon_api::characters::tom::{
    list_character_tom_query::ListCharacterTomQuery,
    record_character_tom_request::RecordCharacterTomRequest,
};
use nexus_contracts::daemon_api::characters::{
    add_character_binding_request::AddCharacterBindingRequest,
    add_character_binding_response::AddCharacterBindingResponse,
    character_binding_detail::CharacterBindingDetail, character_detail::CharacterDetail,
    character_lifecycle_request::CharacterLifecycleRequest,
    create_character_request::CreateCharacterRequest,
    list_character_bindings_query::ListCharacterBindingsQuery,
    list_characters_query::ListCharactersQuery,
    update_character_binding_request::UpdateCharacterBindingRequest,
    update_character_request::UpdateCharacterRequest,
};
use nexus_contracts::daemon_api::creators::{
    active_creator_response::ActiveCreatorResponse, list_creators_query::ListCreatorsQuery,
    logout_response::LogoutResponse, set_active_creator_request::SetActiveCreatorRequest,
    set_active_creator_response::SetActiveCreatorResponse,
};
use nexus_contracts::daemon_api::inspector::{
    moment_directive_request::MomentDirectiveRequest,
    moment_directive_response::MomentDirectiveResponse,
    moment_inspect_request::MomentInspectRequest, moment_inspect_response::MomentInspectResponse,
};
use nexus_contracts::daemon_api::memory::{
    count_pending_reviews_query::CountPendingReviewsQuery,
    count_pending_reviews_response::CountPendingReviewsResponse,
    delete_pending_review_query::DeletePendingReviewQuery,
    delete_pending_review_response::DeletePendingReviewResponse,
    list_memory_fragments_query::ListMemoryFragmentsQuery,
    list_pending_reviews_query::ListPendingReviewsQuery, review_request::ReviewRequest,
    review_response::ReviewResponse, soul_narrative_request::SoulNarrativeRequest,
};
use nexus_contracts::generated::core::CoreCharacterTransitionRequest;
use nexus_core::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, ActorKnowledgeViewService, AdmittedActor,
    CoreError, CHARACTER_WIRE_INVALID_PREFIX,
};
use nexus_knowledge::world_kb::knowledge_entry::{parse_stored_created_at, KnowledgeEntryRecord};
use nexus_local_db::{CharacterPatch, FieldPatch};
use serde::de::DeserializeOwned;

use crate::NativeCore;

/// Per-family list defaults; byte-identical with the daemon HTTP adapters
/// (`characters.rs` / `character_memory.rs`).
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
/// Creator-scope memory list clamp (`handlers/memory.rs`).
const MEMORY_MAX_LIMIT: usize = 250;
const MEMORY_DEFAULT_LIMIT: i64 = 50;
/// Retained opaque offset-cursor prefix (`api/pagination.rs`).
const OFFSET_CURSOR_PREFIX: &str = "v1:";

type CoreWire<T> = std::result::Result<T, CoreError>;

/// Parse an owned wire payload into a generated DTO. Parse failures surface as
/// `invalid <label>: …` rejections, which the TS error mapper classifies as
/// 400 `invalid_input`.
fn decode<T: DeserializeOwned>(payload: Buffer, label: &str) -> Result<T> {
    serde_json::from_slice(payload.as_ref())
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))
}

/// Parse an owned wire payload once into the typed DTO plus the raw-key view
/// (the raw member presence drives the tri-state patch fields). `Buffer` is
/// not `Clone`, so both views come from the single parse.
fn decode_with_raw<T: DeserializeOwned>(
    payload: Buffer,
    label: &str,
) -> Result<(T, serde_json::Map<String, serde_json::Value>)> {
    let value: serde_json::Value = serde_json::from_slice(payload.as_ref())
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))?;
    let raw = value.as_object().cloned().ok_or_else(|| {
        Error::from_reason(format!(
            "invalid {label}: request body must be a JSON object"
        ))
    })?;
    let typed: T = serde_json::from_value(value)
        .map_err(|error| Error::from_reason(format!("invalid {label}: {error}")))?;
    Ok((typed, raw))
}

/// Wire round-trip for generated DTO construction inside core-call closures; a
/// projection failure is the retained `CHARACTER_WIRE_INVALID` internal
/// category, not a client error.
fn wire_map<T: DeserializeOwned>(value: impl serde::Serialize) -> CoreWire<T> {
    let json = serde_json::to_value(value).map_err(|error| CoreError::Internal {
        category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
    })?;
    serde_json::from_value(json).map_err(|error| CoreError::Internal {
        category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
    })
}

/// Decode the retained opaque offset cursor (`v1:<offset>`); absent → 0.
fn offset_from_cursor(cursor: Option<String>) -> Result<u32> {
    let Some(raw) = cursor else {
        return Ok(0);
    };
    let stripped = raw.strip_prefix(OFFSET_CURSOR_PREFIX).ok_or_else(|| {
        Error::from_reason(
            "invalid cursor: invalid pagination cursor; pass the `next_cursor` value returned \
             by the previous response unchanged",
        )
    })?;
    stripped.parse::<u32>().map_err(|_| {
        Error::from_reason(
            "invalid cursor: invalid pagination cursor; pass the `next_cursor` value returned \
             by the previous response unchanged",
        )
    })
}

/// Character/memory family list clamp (default 50, max 100).
fn resolve_limit(raw: Option<i64>) -> Result<u32> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => {
            u32::try_from(n).map_err(|_| Error::from_reason("invalid limit: limit is out of range"))
        }
        Some(_) => Err(Error::from_reason(format!(
            "invalid limit: limit must be between 1 and {MAX_LIMIT}"
        ))),
    }
}

/// Creator-scope memory list clamp (default 50, 1..=250) as `usize`.
fn resolve_query_limit(raw: Option<i64>) -> usize {
    let clamped = raw
        .unwrap_or(MEMORY_DEFAULT_LIMIT)
        .clamp(1, i64::try_from(MEMORY_MAX_LIMIT).unwrap_or(i64::MAX));
    usize::try_from(clamped).unwrap_or(MEMORY_MAX_LIMIT)
}

/// Retained creator-id shape guard (`ctr_` + ASCII alphanumerics).
fn valid_creator_id(creator_id: &str) -> bool {
    creator_id.len() > 4
        && creator_id.starts_with("ctr_")
        && creator_id[4..].bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The standalone napi surface carries no ACP capability registry, so the
/// provider-backed SOUL synthesizer factory is `None` — the daemon's own
/// "missing registry" semantics: observational reflects succeed without any
/// provider effect, and a forced regeneration surfaces the core's retained
/// 503 after authorization (never a fake success). Provider-backed synthesis
/// wiring lands with the P5-T3 provider surface.
struct UnavailableSoulSynthesizer;

impl nexus_creator_memory::soul_narrative::SoulNarrativeSynthesizer for UnavailableSoulSynthesizer {
    async fn synthesize(
        &self,
        _bearer: nexus_creator_memory::MemoryBearerRef<'_>,
        _input: nexus_creator_memory::soul_narrative::SoulNarrativeSynthesisInput,
        _session_scope: Option<&str>,
    ) -> std::result::Result<
        nexus_creator_memory::soul_narrative::SoulNarrativeDraft,
        nexus_creator_memory::MemoryError,
    > {
        unreachable!("the unavailable synthesizer is never constructed by the core")
    }
}

// ── Knowledge wire projections (mirror handlers/actor_knowledge.rs) ────────

fn knowledge_item_from_record(record: &KnowledgeEntryRecord) -> CoreWire<KnowledgeViewItem> {
    let created_at =
        parse_stored_created_at(&record.created_at).map_err(|error| CoreError::Internal {
            category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
        })?;
    let owner = serde_json::json!({
        "kind": record.owner.kind(),
        "id": record.owner.id(),
    });
    let block_type =
        serde_json::to_value(&record.block_type).map_err(|error| CoreError::Internal {
            category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
        })?;
    let value = serde_json::json!({
        "entry_id": record.entry_id,
        "owner": owner,
        "creator_only": record.creator_only,
        "block_type": block_type,
        "canonical_name": record.canonical_name,
        "status": record.status,
        "revision": record.revision.unwrap_or(0),
        "created_at": created_at,
    });
    serde_json::from_value(value).map_err(|error| CoreError::Internal {
        category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
    })
}

/// The retained summary wire member projects from the stored body block.
fn knowledge_summary_wire_value(record: &KnowledgeEntryRecord) -> Option<&str> {
    record
        .body
        .as_ref()
        .and_then(|body| body.summary.as_deref())
}

fn knowledge_detail_from_record(record: &KnowledgeEntryRecord) -> CoreWire<KnowledgeEntryDetail> {
    let item = knowledge_item_from_record(record)?;
    let summary = knowledge_summary_wire_value(record);
    let value = serde_json::json!({
        "item": wire_map::<DetailItem>(item)?,
        "summary": summary,
    });
    serde_json::from_value(value).map_err(|error| CoreError::Internal {
        category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {error}"),
    })
}

fn knowledge_page(page: &ActorKnowledgePage) -> CoreWire<(Vec<ViewItem>, ViewPagination)> {
    let mut items = Vec::with_capacity(page.items.len());
    for record in &page.items {
        items.push(wire_map::<ViewItem>(knowledge_item_from_record(record)?)?);
    }
    let pagination: ViewPagination = wire_map(serde_json::json!({
        "limit": i64::from(page.limit),
        "has_more": page.has_more,
        "next_cursor": page.next_cursor,
    }))?;
    Ok((items, pagination))
}

/// Derive the opaque [`AdmittedActor`] token from the wire actor_ref exactly
/// like the daemon view handler: stored ownership is re-validated inside the
/// core call, so this is projection only.
fn admitted_from_ref(actor_ref: &NexusActorRef) -> AdmittedActor {
    match actor_ref {
        NexusActorRef::CreatorActorRef { creator_id, .. } => AdmittedActor::Creator {
            creator_id: creator_id.to_string(),
        },
        NexusActorRef::CharacterActorRef { character_id, .. } => AdmittedActor::Character {
            character_id: character_id.to_string(),
        },
    }
}

/// Tri-state knowledge summary patch: absent → keep, null → clear, string → set.
fn knowledge_summary_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateKnowledgeEntryRequest,
) -> Result<FieldPatch<&'a str>> {
    match raw.get("summary") {
        None => Ok(FieldPatch::Keep),
        Some(serde_json::Value::Null) => Ok(FieldPatch::Clear),
        Some(_) => Ok(FieldPatch::Set(
            req.summary
                .as_ref()
                .ok_or_else(|| {
                    Error::from_reason(
                        "invalid summary: summary must be a string or null when present",
                    )
                })?
                .as_str(),
        )),
    }
}

/// Tri-state character patch (`build_character_patch` in the daemon adapter):
/// presence of the raw member distinguishes set/clear from keep, and the
/// persona JSON string borrows from `persona_buf` to satisfy the core's
/// `CharacterPatch<'_>` lifetime.
fn build_character_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateCharacterRequest,
    persona_buf: &'a mut Option<String>,
) -> Result<CharacterPatch<'a>> {
    let display_name = if raw.contains_key("display_name") {
        Some(
            req.display_name
                .as_ref()
                .ok_or_else(|| {
                    Error::from_reason(
                        "invalid display_name: display_name must be a non-null string when present",
                    )
                })?
                .as_str(),
        )
    } else {
        None
    };
    let image_uri = if !raw.contains_key("image_uri") {
        FieldPatch::Keep
    } else if req.image_uri.is_none() {
        FieldPatch::Clear
    } else {
        FieldPatch::Set(
            req.image_uri
                .as_ref()
                .expect("image_uri member present implies Some")
                .as_str(),
        )
    };
    let persona_json = if !raw.contains_key("persona") {
        FieldPatch::Keep
    } else if req.persona.is_none() {
        FieldPatch::Clear
    } else {
        let encoded = serde_json::Value::Object(
            req.persona
                .clone()
                .expect("persona member present implies Some"),
        )
        .to_string();
        *persona_buf = Some(encoded);
        FieldPatch::Set(persona_buf.as_deref().expect("persona buffer set"))
    };
    Ok(CharacterPatch {
        display_name,
        image_uri,
        persona_json,
    })
}

/// Binding sheet tri-state patch: absent → 400 (empty patch), null → clear,
/// string → set.
fn build_binding_sheet_patch<'a>(
    raw: &'a serde_json::Map<String, serde_json::Value>,
    req: &'a UpdateCharacterBindingRequest,
) -> Result<FieldPatch<&'a str>> {
    match raw.get("world_sheet_entry_id") {
        None => Err(Error::from_reason(
            "invalid patch: patch must include at least one mutable field",
        )),
        Some(serde_json::Value::Null) => Ok(FieldPatch::Clear),
        Some(_) => Ok(FieldPatch::Set(
            req.world_sheet_entry_id
                .as_ref()
                .ok_or_else(|| {
                    Error::from_reason(
                        "invalid world_sheet_entry_id: world_sheet_entry_id must be a non-null \
                         string when present",
                    )
                })?
                .as_str(),
        )),
    }
}

#[napi]
impl NativeCore {
    // ── Character identity + bindings (P2-T1 authority) ─────────────────────

    /// `GET /v1/daemon/characters` — read-model Character list.
    #[napi]
    pub async fn list_characters(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharactersQuery = decode(query_json, "query")?;
        let limit = resolve_limit(query.limit)?;
        let offset = offset_from_cursor(query.cursor)?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_characters(&principal, limit, offset).await
        })
        .await
    }

    /// `POST /v1/daemon/characters` — 201 at the adapter.
    #[napi]
    pub async fn create_character(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CreateCharacterRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.create_character(&principal, request).await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}` — a foreign/missing
    /// Character is the core's 404 (existence hidden).
    #[napi]
    pub async fn get_character(
        &self,
        principal_handle: String,
        character_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.character(&principal, character_id).await
        })
        .await
    }

    /// `PATCH /v1/daemon/characters/{character_id}` — tri-state patch over the
    /// raw member view + typed DTO; both parse from the single owned payload.
    /// Wire-shape validation intentionally precedes owner/404 admission so an
    /// empty patch on a foreign/missing Character returns 400, not 404.
    #[napi]
    pub async fn patch_character(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let (req, raw) = decode_with_raw::<UpdateCharacterRequest>(request_json, "request")?;
        let mut persona_buf: Option<String> = None;
        let patch = build_character_patch(&raw, &req, &mut persona_buf)?;
        if patch.is_empty() {
            return Err(Error::from_reason(
                "invalid patch: patch must include at least one mutable field",
            ));
        }
        self.json_call(principal_handle, async move |core, principal| {
            core.patch_character(&principal, character_id, req.expected_revision, patch)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/archive` — the standalone
    /// lifecycle path goes through the core's fenced transition (the daemon's
    /// registry-fence + Host-retirement half belongs to the Host session
    /// surface, P4-T2).
    #[napi]
    pub async fn archive_character(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        self.character_transition(principal_handle, character_id, request_json, "archived")
            .await
    }

    /// `POST /v1/daemon/characters/{character_id}/restore`.
    #[napi]
    pub async fn restore_character(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        self.character_transition(principal_handle, character_id, request_json, "active")
            .await
    }

    async fn character_transition(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
        target_status: &'static str,
    ) -> Result<Buffer> {
        let lifecycle: CharacterLifecycleRequest = decode(request_json, "request")?;
        let request: CoreCharacterTransitionRequest = wire_map(serde_json::json!({
            "character_id": character_id,
            "expected_revision": lifecycle.expected_revision,
            "target_status": target_status,
        }))
        .map_err(|error| Error::from_reason(error.to_string()))?;
        self.json_call(principal_handle, async move |core, principal| {
            let response = core.transition_character(&principal, request).await?;
            // The retained archive/restore envelope is `CharacterDetail`.
            let detail: CharacterDetail = wire_map(serde_json::json!({
                "character": response.character,
            }))?;
            Ok(detail)
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/bindings` — 201 at the adapter.
    #[napi]
    pub async fn add_character_binding(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: AddCharacterBindingRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: AddCharacterBindingResponse = core
                .add_binding(
                    &principal,
                    character_id,
                    request.world_id.to_string(),
                    request
                        .world_sheet_entry_id
                        .as_ref()
                        .map(|value| value.to_string()),
                )
                .await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/bindings`.
    #[napi]
    pub async fn list_character_bindings(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharacterBindingsQuery = decode(query_json, "query")?;
        let limit = resolve_limit(query.limit)?;
        let offset = offset_from_cursor(query.cursor)?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_bindings(&principal, character_id, limit, offset)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/bindings/{binding_id}`.
    #[napi]
    pub async fn get_character_binding(
        &self,
        principal_handle: String,
        character_id: String,
        binding_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let detail: CharacterBindingDetail =
                core.binding(&principal, character_id, binding_id).await?;
            Ok(detail)
        })
        .await
    }

    /// `PATCH /v1/daemon/characters/{character_id}/bindings/{binding_id}` —
    /// a stale `expected_revision` is the core's retained 409; a stale
    /// binding sheet value never silently wins.
    #[napi]
    pub async fn patch_character_binding(
        &self,
        principal_handle: String,
        character_id: String,
        binding_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let (req, raw) = decode_with_raw::<UpdateCharacterBindingRequest>(request_json, "request")?;
        let world_sheet_entry_id = build_binding_sheet_patch(&raw, &req)?;
        self.json_call(principal_handle, async move |core, principal| {
            let detail: CharacterBindingDetail = core
                .patch_binding(
                    &principal,
                    character_id,
                    binding_id,
                    req.expected_revision,
                    world_sheet_entry_id,
                )
                .await?;
            Ok(detail)
        })
        .await
    }

    /// `DELETE /v1/daemon/characters/{character_id}/bindings/{binding_id}` —
    /// 204 at the adapter.
    #[napi]
    pub async fn remove_character_binding(
        &self,
        principal_handle: String,
        character_id: String,
        binding_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.remove_binding(&principal, character_id, binding_id)
                .await?;
            Ok(serde_json::Value::Null)
        })
        .await
    }

    // ── Actor KnowledgeView (P2-T1 authority) ───────────────────────────────

    /// `POST /v1/daemon/actor-knowledge/view` — the admitted-actor context
    /// read: a foreign/unknown actor ref rejects before any storage effect.
    #[napi]
    pub async fn actor_knowledge_view(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let req: ViewRequest = decode(request_json, "request")?;
        let actor = admitted_from_ref(&req.actor_ref);
        let limit = ActorKnowledgeViewService::resolve_limit(req.limit)
            .map_err(|error| Error::from_reason(format!("invalid limit: {error}")))?;
        let query = ActorKnowledgeViewQuery {
            world_id: req.world_id.to_string(),
            binding_id: req.binding_id.as_ref().map(|value| value.to_string()),
            limit,
            cursor: req.cursor.clone(),
        };
        self.json_call(principal_handle, async move |core, principal| {
            let page = core.actor_knowledge_view(&principal, &actor, query).await?;
            let (items, pagination) = knowledge_page(&page)?;
            let response: ViewResponse = wire_map(serde_json::json!({
                "items": items,
                "pagination": pagination,
            }))?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/actor-knowledge/entries` — 201 at the adapter.
    #[napi]
    pub async fn add_actor_knowledge_entry(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let (req, raw) = decode_with_raw::<AddKnowledgeEntryRequest>(request_json, "request")?;
        // The retained wire distinction between an absent and a null `summary`
        // member is a request-shape concern, so it stays at the adapter.
        let summary_present = raw.contains_key("summary");
        self.json_call(principal_handle, async move |core, principal| {
            let stored = core
                .add_actor_knowledge_entry(&principal, req, summary_present)
                .await?;
            let response: AddKnowledgeEntryResponse = wire_map(serde_json::json!({
                "item": wire_map::<CreatedItem>(knowledge_item_from_record(&stored)?)?,
            }))?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/knowledge`.
    #[napi]
    pub async fn list_character_knowledge(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharacterKnowledgeQuery = decode(query_json, "query")?;
        let limit = ActorKnowledgeViewService::resolve_limit(query.limit)
            .map_err(|error| Error::from_reason(format!("invalid limit: {error}")))?;
        self.json_call(principal_handle, async move |core, principal| {
            let page = core
                .list_character_knowledge(&principal, character_id, limit, query.cursor)
                .await?;
            let mut items = Vec::with_capacity(page.items.len());
            for record in &page.items {
                items.push(wire_map::<ListedItem>(knowledge_item_from_record(record)?)?);
            }
            let pagination: ListedPagination = wire_map(serde_json::json!({
                "limit": i64::from(page.limit),
                "has_more": page.has_more,
                "next_cursor": page.next_cursor,
            }))?;
            let response: ListCharacterKnowledgeResponse = wire_map(serde_json::json!({
                "items": items,
                "pagination": pagination,
            }))?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/knowledge/{entry_id}`.
    #[napi]
    pub async fn get_knowledge_entry(
        &self,
        principal_handle: String,
        character_id: String,
        entry_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            let record = core
                .actor_knowledge_entry(&principal, character_id, entry_id)
                .await?;
            Ok(knowledge_detail_from_record(&record)?)
        })
        .await
    }

    /// `PATCH /v1/daemon/characters/{character_id}/knowledge/{entry_id}`.
    #[napi]
    pub async fn patch_knowledge_entry(
        &self,
        principal_handle: String,
        character_id: String,
        entry_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let (req, raw) = decode_with_raw::<UpdateKnowledgeEntryRequest>(request_json, "request")?;
        if !raw.contains_key("canonical_name") && !raw.contains_key("summary") {
            return Err(Error::from_reason(
                "invalid patch: patch must include at least one mutable field",
            ));
        }
        let canonical_name_patch = if raw.contains_key("canonical_name") {
            Some(
                req.canonical_name
                    .as_ref()
                    .ok_or_else(|| {
                        Error::from_reason(
                            "invalid canonical_name: canonical_name must be a non-null string \
                             when present",
                        )
                    })?
                    .as_str(),
            )
        } else {
            None
        };
        let summary_patch = knowledge_summary_patch(&raw, &req)?;
        self.json_call(principal_handle, async move |core, principal| {
            let record = core
                .patch_actor_knowledge_entry(
                    &principal,
                    character_id,
                    entry_id,
                    req.expected_revision,
                    canonical_name_patch,
                    summary_patch,
                )
                .await?;
            Ok(knowledge_detail_from_record(&record)?)
        })
        .await
    }

    /// `DELETE /v1/daemon/characters/{character_id}/knowledge/{entry_id}` —
    /// 204 at the adapter; `expected_revision` is a required query parameter.
    #[napi]
    pub async fn delete_knowledge_entry(
        &self,
        principal_handle: String,
        character_id: String,
        entry_id: String,
        expected_revision: i64,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_actor_knowledge_entry(
                &principal,
                character_id,
                entry_id,
                expected_revision,
            )
            .await?;
            Ok(serde_json::Value::Null)
        })
        .await
    }

    // ── Creator identity family (CoreHomeService; Tier-1 — no principal) ────

    /// Open the pre-selection home-control entry against this core's raw user
    /// home (`nexus_home` is the raw home with `.nexus42` appended exactly
    /// once, so the parent is the retained user-home argument).
    fn home_service(&self) -> Result<nexus_core::CoreHomeService> {
        self.deny_service_only()?;
        let core = self
            .inner
            .core
            .lock()
            .map_err(|_| Error::from_reason("core mutex poisoned"))?
            .clone()
            .ok_or_else(|| Error::from_reason("core not open"))?;
        let user_home = core
            .nexus_home()
            .parent()
            .ok_or_else(|| Error::from_reason("invalid home layout"))?;
        nexus_core::CoreHomeService::open(user_home.to_path_buf())
            .map_err(crate::core_error::napi_error_from_domain)
    }

    /// `GET /v1/daemon/creators`.
    #[napi]
    pub async fn list_creators(&self, query_json: Buffer) -> Result<Buffer> {
        let query: ListCreatorsQuery = decode(query_json, "query")?;
        let home = self.home_service()?;
        let response = home
            .list_creators(query)
            .await
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    /// `POST /v1/daemon/creators` — 201 at the adapter. The display name
    /// crosses as a plain argument: no generated create-DTO exists for the
    /// creators family (schema gap recorded in the task report).
    #[napi]
    pub async fn create_creator(&self, display_name: String) -> Result<Buffer> {
        let home = self.home_service()?;
        let detail = home
            .create_creator(display_name)
            .await
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&detail)?))
    }

    /// `GET /v1/daemon/creators/{creator_id}`.
    #[napi]
    pub async fn get_creator(&self, creator_id: String) -> Result<Buffer> {
        let home = self.home_service()?;
        let detail = home
            .creator_detail(&creator_id)
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&detail)?))
    }

    /// `PATCH /v1/daemon/creators/{creator_id}`.
    #[napi]
    pub async fn patch_creator(
        &self,
        creator_id: String,
        display_name: Option<String>,
    ) -> Result<Buffer> {
        let home = self.home_service()?;
        let detail = home
            .patch_creator(&creator_id, display_name)
            .await
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&detail)?))
    }

    /// `PUT /v1/daemon/creators/active`.
    #[napi]
    pub async fn set_active_creator(&self, request_json: Buffer) -> Result<Buffer> {
        let request: SetActiveCreatorRequest = decode(request_json, "request")?;
        let home = self.home_service()?;
        let response: SetActiveCreatorResponse = home
            .use_creator(request)
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    /// `GET /v1/daemon/creators/active`.
    #[napi]
    pub async fn get_active_creator(&self) -> Result<Buffer> {
        let home = self.home_service()?;
        let response: ActiveCreatorResponse = home
            .active_creator()
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    /// `POST /v1/daemon/creators/{creator_id}` — retained `:logout` verb.
    #[napi]
    pub async fn logout_creator(&self, creator_id: String) -> Result<Buffer> {
        let home = self.home_service()?;
        let response: LogoutResponse = home
            .logout_creator(&creator_id)
            .map_err(crate::core_error::napi_error_from_domain)?;
        Ok(Buffer::from(serde_json::to_vec(&response)?))
    }

    // ── Character memory / SOUL / ToM (P2-T2 authority) ─────────────────────

    /// `POST /v1/daemon/characters/{character_id}/memory/pending-review` — 201.
    #[napi]
    pub async fn capture_character_pending_review(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CaptureCharacterPendingReviewRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.capture_character_pending_review(&principal, character_id, request)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/memory/pending-review`.
    #[napi]
    pub async fn list_character_pending_reviews(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharacterPendingReviewsQuery = decode(query_json, "query")?;
        let binding_id = query.binding_id.as_ref().map(|value| value.to_string());
        let limit = resolve_limit(query.limit)?;
        let offset = offset_from_cursor(query.cursor)?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_character_pending_reviews(&principal, character_id, binding_id, limit, offset)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/memory/pending-review/count`.
    #[napi]
    pub async fn count_character_pending_reviews(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: CountCharacterPendingReviewsQuery = decode(query_json, "query")?;
        let binding_id = query.binding_id.as_ref().map(|value| value.to_string());
        self.json_call(principal_handle, async move |core, principal| {
            core.count_character_pending_reviews(&principal, character_id, binding_id)
                .await
        })
        .await
    }

    /// `DELETE /v1/daemon/characters/{character_id}/memory/pending-review/{pending_id}`.
    #[napi]
    pub async fn delete_character_pending_review(
        &self,
        principal_handle: String,
        character_id: String,
        pending_id: String,
    ) -> Result<Buffer> {
        self.json_call(principal_handle, async move |core, principal| {
            core.delete_character_pending_review(&principal, character_id, pending_id)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/memory/review`.
    #[napi]
    pub async fn review_character_memory(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ReviewCharacterMemoryRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.review_character_memory(&principal, character_id, request)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/memory/fragments`.
    #[napi]
    pub async fn list_character_memory_fragments(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharacterMemoryFragmentsQuery = decode(query_json, "query")?;
        let binding_id = query.binding_id.as_ref().map(|value| value.to_string());
        let limit = resolve_limit(query.limit)?;
        let offset = offset_from_cursor(query.cursor)?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_character_memory_fragments(
                &principal,
                character_id,
                binding_id,
                limit,
                offset,
            )
            .await
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/memory/fragments/{fragment_id}:promote`.
    #[napi]
    pub async fn promote_character_fragment(
        &self,
        principal_handle: String,
        character_id: String,
        fragment_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: PromoteCharacterFragmentRequest = decode(request_json, "request")?;
        let expected_revision = i64::try_from(request.expected_revision).map_err(|_| {
            Error::from_reason("invalid expected_revision: expected_revision is out of range")
        })?;
        self.json_call(principal_handle, async move |core, principal| {
            core.promote_character_fragment(
                &principal,
                character_id,
                fragment_id,
                expected_revision,
            )
            .await
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/soul/reflect` — the
    /// synthesizer factory is `None` (no ACP registry on this surface);
    /// authorization still precedes any provider consideration.
    #[napi]
    pub async fn reflect_character_soul(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: CharacterSoulNarrativeRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.reflect_character_soul(&principal, character_id, request, || {
                Option::<UnavailableSoulSynthesizer>::None
            })
            .await
        })
        .await
    }

    /// `POST /v1/daemon/characters/{character_id}/tom`.
    #[napi]
    pub async fn record_character_tom(
        &self,
        principal_handle: String,
        character_id: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: RecordCharacterTomRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.record_character_tom(&principal, character_id, request)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/characters/{character_id}/tom`.
    #[napi]
    pub async fn list_character_tom(
        &self,
        principal_handle: String,
        character_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListCharacterTomQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            core.list_character_tom(&principal, character_id, query)
                .await
        })
        .await
    }

    // ── Creator-scope memory family (active-creator equality kept here) ─────

    /// `GET /v1/daemon/memory/pending-review` — the wire `creator_id` must be
    /// the natively minted active creator; a mismatch is the retained 403.
    #[napi]
    pub async fn list_pending_reviews(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListPendingReviewsQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            if query.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "pending_review".to_string(),
                });
            }
            if !valid_creator_id(query.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            let limit = resolve_query_limit(query.limit);
            core.list_pending_reviews(&principal, query.cursor, limit)
                .await
        })
        .await
    }

    /// `GET /v1/daemon/memory/pending-review/count?creator_id=…` — the wire
    /// `creator_id` must be the natively minted active creator; a mismatch is
    /// the retained 403 (same equality + format guard as the list route).
    #[napi]
    pub async fn count_pending_reviews(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: CountPendingReviewsQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            if query.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "pending_review".to_string(),
                });
            }
            if !valid_creator_id(query.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            let response: CountPendingReviewsResponse =
                core.count_pending_reviews(&principal).await?;
            Ok(response)
        })
        .await
    }

    /// `DELETE /v1/daemon/memory/pending-review/{pending_id}?creator_id=…` —
    /// the wire `creator_id` must be the natively minted active creator; a
    /// mismatch is the retained 403 (same guard as the list route).
    #[napi]
    pub async fn delete_pending_review(
        &self,
        principal_handle: String,
        pending_id: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: DeletePendingReviewQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            if query.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "pending_review".to_string(),
                });
            }
            if !valid_creator_id(query.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            let response: DeletePendingReviewResponse =
                core.delete_pending_review(&principal, pending_id).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/memory/review`.
    #[napi]
    pub async fn review_memory(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: ReviewRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            if request.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "pending_review".to_string(),
                });
            }
            if !valid_creator_id(request.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            let response: ReviewResponse = core.review_memory(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `GET /v1/daemon/memory/fragments`.
    #[napi]
    pub async fn list_memory_fragments(
        &self,
        principal_handle: String,
        query_json: Buffer,
    ) -> Result<Buffer> {
        let query: ListMemoryFragmentsQuery = decode(query_json, "query")?;
        self.json_call(principal_handle, async move |core, principal| {
            if query.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "memory_fragments".to_string(),
                });
            }
            if !valid_creator_id(query.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            let limit = resolve_query_limit(query.limit);
            core.list_memory_fragments(&principal, query.keyword, query.world_id, limit)
                .await
        })
        .await
    }

    /// `POST /v1/daemon/memory/soul/reflect` — Creator-SOUL; synthesizer
    /// factory is `None` (same missing-registry semantics as the Character
    /// reflect above).
    #[napi]
    pub async fn reflect_creator_soul(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: SoulNarrativeRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            if request.creator_id.as_str() != principal.creator_id() {
                return Err(CoreError::Forbidden {
                    resource: "soul_narrative".to_string(),
                });
            }
            if !valid_creator_id(request.creator_id.as_str()) {
                return Err(CoreError::InvalidInput {
                    field: "creator_id".to_string(),
                    reason: "creator_id must start with 'ctr_' followed by alphanumeric characters"
                        .to_string(),
                });
            }
            core.reflect_creator_soul(&principal, request, || {
                Option::<UnavailableSoulSynthesizer>::None
            })
            .await
        })
        .await
    }

    // ── Moment inspector + directive (P2-T2 authority) ──────────────────────

    /// `POST /v1/daemon/inspector/moment` — observational read-only assembly.
    #[napi]
    pub async fn inspect_moment(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: MomentInspectRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: MomentInspectResponse = core.inspect_moment(&principal, request).await?;
            Ok(response)
        })
        .await
    }

    /// `POST /v1/daemon/moment-directive`.
    #[napi]
    pub async fn moment_directive(
        &self,
        principal_handle: String,
        request_json: Buffer,
    ) -> Result<Buffer> {
        let request: MomentDirectiveRequest = decode(request_json, "request")?;
        self.json_call(principal_handle, async move |core, principal| {
            let response: MomentDirectiveResponse =
                core.moment_directive(&principal, request).await?;
            Ok(response)
        })
        .await
    }
}
