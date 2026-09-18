//! Stored Actor admission, Character/binding ownership and lifecycle fencing
//! (v1.190 P2-T1).
//!
//! Extracted from the daemon `actor_admission` service, `actor_knowledge_view`
//! owner checks and the `characters`/`bindings` HTTP handlers: stored
//! ownership validation, the opaque [`AdmittedActor`] token, Host-free
//! activity/transition leases ([`crate::actor_fence`]) and the Character/
//! binding family commands. Request bodies never carry ownership; the
//! principal's creator is the only admitted caller, and
//! `lifecycle_epoch`/ownership are re-read after every fence acquisition.
//!
//! `AdmittedActor` is an opaque Rust token minted after stored validation —
//! it deliberately carries no `Serialize`/`Deserialize` impl, so a serialized
//! admission context can never be replayed as authority. The wire projection
//! that may cross napi is [`AdmittedActorContext`]-shaped data assembled by
//! the host, not this token.

use nexus_contracts::generated::core::core_actor_admission_request::{
    NexusActorRef, NexusSessionViewpoint,
};
use nexus_contracts::generated::core::{
    CoreActorAdmissionRequest, CoreCharacterTransitionRequest, CoreCharacterTransitionResponse,
};
use nexus_contracts::generated::daemon_api::characters::{
    add_character_binding_response::{AddCharacterBindingResponse, NexusActorWorldBinding},
    character_binding_detail::{
        CharacterBindingDetail, NexusActorWorldBinding as DetailBindingWire,
    },
    character_detail::{CharacterDetail, NexusCharacter as DetailCharacterWire},
    create_character_request::CreateCharacterRequest,
    create_character_response::{
        CreateCharacterResponse, NexusActorWorldBinding as CreatedBindingWire,
        NexusCharacter as CreatedCharacterWire,
    },
    list_character_bindings_response::{
        ListCharacterBindingsResponse, NexusActorWorldBinding as ListedBindingWire,
        NexusPaginationInfo as BindingPaginationInfo,
    },
    list_characters_response::{
        ListCharactersResponse, NexusCharacter as ListedCharacterWire, NexusPaginationInfo,
    },
};
use nexus_local_db::actor_world_binding::ActorWorldBindingRecord;
use nexus_local_db::character::CharacterRecord;
use nexus_local_db::{CharacterStatus, CreateBindingParams, CreateCharacterParams};
use sqlx::SqlitePool;

use crate::actor_knowledge::ActorKnowledgePage;
use crate::error::{actor_db_err, db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use sqlx::Row;

/// Wire-mapping failure carrier: the daemon adapter re-sends the retained
/// `CHARACTER_WIRE_INVALID` internal code from this exact category prefix.
pub const CHARACTER_WIRE_INVALID_PREFIX: &str = "character_wire_invalid";

// ── Admission types ─────────────────────────────────────────────────────

/// Pair classification for optional Actor/Viewpoint fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorPairMode {
    /// Both fields absent — exact legacy path.
    Legacy,
    /// Both fields present — Actor mode.
    Actor,
}

/// Optional viewpoint identifiers after pair classification.
#[derive(Debug, Clone)]
pub struct ActorViewpoint {
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
}

/// Discriminant admitted from stored owners (never from payload claims).
///
/// Opaque on purpose: no `Serialize`/`Deserialize`, so a serialized context
/// cannot be replayed as authority.
#[derive(Debug, Clone)]
pub enum AdmittedActor {
    Creator { creator_id: String },
    Character { character_id: String },
}

impl AdmittedActor {
    /// The Character id when this admission is a Character; `None` for Creator.
    #[must_use]
    pub const fn character_id(&self) -> Option<&str> {
        match self {
            Self::Creator { .. } => None,
            Self::Character { character_id } => Some(character_id.as_str()),
        }
    }
}

/// Admitted Actor + viewpoint plus the bounded `KnowledgeView`.
#[derive(Debug, Clone)]
pub struct AdmittedActorContext {
    pub actor: AdmittedActor,
    /// Trusted owner: the active Creator admitted at request time. Stored in
    /// the session index and retired tombstones for later owner authorization;
    /// never taken from a request body.
    pub owner_creator_id: String,
    pub world_id: String,
    pub binding_id: Option<String>,
    pub branch_id: Option<String>,
    pub event_id: Option<String>,
    /// Stored Character `lifecycle_epoch` at admission (`None` for Creator).
    /// Execution compares this indexed epoch against the current stored epoch
    /// after activity admission; a mismatch is `actor_session_stale`.
    pub character_epoch: Option<i64>,
    pub view: ActorKnowledgePage,
}

/// Classify the optional pair. Partial pairs are a stable 400.
///
/// # Errors
///
/// Returns [`CoreError::ActorInput`] when exactly one of the pair is present.
pub fn classify_pair(actor_present: bool, viewpoint_present: bool) -> CoreResult<ActorPairMode> {
    match (actor_present, viewpoint_present) {
        (false, false) => Ok(ActorPairMode::Legacy),
        (true, true) => Ok(ActorPairMode::Actor),
        (true, false) | (false, true) => Err(CoreError::ActorInput(
            "actor_ref and viewpoint must both be present or both absent".to_string(),
        )),
    }
}

fn not_found(resource: &str, id: &str) -> CoreError {
    CoreError::NotFound {
        resource: format!("{resource} {id}"),
    }
}

fn invalid_input(message: impl Into<String>) -> CoreError {
    CoreError::ActorInput(message.into())
}

// ── Wire mapping (record → generated DTO) ───────────────────────────────

pub fn wire_err(err: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("{CHARACTER_WIRE_INVALID_PREFIX}: {err}"),
    }
}

pub fn parse_rfc3339(raw: &str) -> CoreResult<chrono::DateTime<chrono::Utc>> {
    raw.parse().map_err(wire_err)
}

fn parse_optional<T>(raw: Option<&str>) -> CoreResult<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    raw.map(str::parse).transpose().map_err(wire_err)
}

pub fn nexus_character_from_record(record: &CharacterRecord) -> CoreResult<DetailCharacterWire> {
    let persona: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&record.persona_json).map_err(wire_err)?;
    build_wire(
        DetailCharacterWire::builder()
            .schema_version(1u64)
            .character_id(record.character_id.as_str())
            .owner_creator_id(record.owner_creator_id.as_str())
            .display_name(record.display_name.as_str())
            .status(record.status.as_str())
            .revision(record.revision)
            .persona(persona)
            .image_uri(parse_optional(record.image_uri.as_deref())?)
            .created_at(parse_rfc3339(&record.created_at)?)
            .updated_at(parse_rfc3339(&record.updated_at)?)
            .try_into(),
    )
}

fn binding_wire_from_record(record: &ActorWorldBindingRecord) -> CoreResult<DetailBindingWire> {
    build_wire(
        DetailBindingWire::builder()
            .schema_version(1u64)
            .binding_id(record.binding_id.as_str())
            .character_id(record.character_id.as_str())
            .world_id(record.world_id.as_str())
            .status(record.status.as_str())
            .revision(record.revision)
            .world_sheet_entry_id(parse_optional(record.world_sheet_entry_id.as_deref())?)
            .created_at(parse_rfc3339(&record.created_at)?)
            .updated_at(parse_rfc3339(&record.updated_at)?)
            .try_into(),
    )
}

pub fn build_wire<T, E: std::fmt::Display>(value: Result<T, E>) -> CoreResult<T> {
    value.map_err(wire_err)
}

pub fn pagination_wire(
    limit: u32,
    has_more: bool,
    next_cursor: Option<String>,
) -> CoreResult<NexusPaginationInfo> {
    build_wire(
        NexusPaginationInfo::builder()
            .limit(i64::from(limit))
            .has_more(has_more)
            .next_cursor(next_cursor)
            .try_into(),
    )
}

pub fn binding_pagination_wire(
    limit: u32,
    has_more: bool,
    next_cursor: Option<String>,
) -> CoreResult<BindingPaginationInfo> {
    build_wire(
        BindingPaginationInfo::builder()
            .limit(i64::from(limit))
            .has_more(has_more)
            .next_cursor(next_cursor)
            .try_into(),
    )
}

// ── Admission + fencing commands ────────────────────────────────────────

impl CoreService {
    /// Admit a stored Actor: validate the closed `actor_ref`/`viewpoint`
    /// against stored ownership (active Creator self-ownership, active owned
    /// World, active owned Character and its active stored binding) and mint
    /// the opaque [`AdmittedActor`] token. No host/MCA/session side effects.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for missing/foreign actor/world/
    /// binding rows, [`CoreError::ActorConflict`] for inactive rows, and
    /// [`CoreError::ActorInput`] for pair-shape violations.
    pub async fn admit_actor(
        &self,
        principal: &Principal,
        request: CoreActorAdmissionRequest,
    ) -> CoreResult<AdmittedActor> {
        self.verify_principal(principal)?;
        let actor = admit_actor_token(principal.creator_id(), request.actor_ref)?;
        let viewpoint = admit_viewpoint(request.viewpoint);
        require_admitted_ownership(&self.inner.pool, principal.creator_id(), &actor, &viewpoint)
            .await?;
        Ok(actor)
    }

    /// Admit one side-effecting Character activity: owner-check before
    /// allocating a fence, hold the shared lease, then re-read status/epoch
    /// under the fence (durable §11.3.1). The caller holds the returned
    /// lease through every DB/file/provider/terminal-capture effect; drop
    /// releases the fence.
    ///
    /// Acquisition is non-blocking, so a Character whose exclusive transition
    /// lease is already held by another core refuses here with `character_busy`
    /// rather than parking behind it (arch §2 / durable §11.3: "shared effect
    /// leases/exclusive transition leases are nonblocking — busy is
    /// observable"). Callers retry once the transition settles.
    ///
    /// Creator actors hold no activity lease (there is no Character lifecycle
    /// to fence); hosts reproduce the daemon's `Creator → None` shape by not
    /// calling this for a Creator token.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::ActorInput`] for a Creator token,
    /// [`CoreError::NotFound`] for a missing/foreign Character,
    /// [`CoreError::ActorConflict`] `character_busy` when a transition lease is
    /// outstanding, and [`CoreError::ActorConflict`] `character_inactive` for
    /// an owned archived Character.
    pub async fn acquire_actor_activity(
        &self,
        principal: &Principal,
        actor: &AdmittedActor,
    ) -> CoreResult<crate::actor_fence::ActorActivityLease> {
        self.verify_principal(principal)?;
        let Some(character_id) = actor.character_id() else {
            return Err(invalid_input("activity leases fence Character actors only"));
        };
        let pool = &self.inner.pool;
        // Owner-check before allocating a fence (no fence state leaks existence).
        require_character_row(pool, principal.creator_id(), character_id).await?;
        let parts = self
            .inner
            .character_fences
            .try_acquire_activity(character_id)?;
        // Re-read under the fence: a transition holds the exclusive write
        // guard, so this status/epoch pair is exact for the lease's lifetime.
        let stored = require_character_row(pool, principal.creator_id(), character_id).await?;
        if stored.status != "active" {
            return Err(CoreError::ActorConflict {
                code: "character_inactive".to_string(),
                message: format!("character {character_id} is {}", stored.status),
            });
        }
        Ok(crate::actor_fence::ActorActivityLease::new(
            parts,
            principal.creator_id().to_string(),
            character_id.to_string(),
            stored.lifecycle_epoch,
        ))
    }

    /// Try to take the exclusive lifecycle lease for archive/restore
    /// (durable §11.3.2): busy refusal, never forced cancellation or waiting
    /// on a provider. The lease re-reads ownership and the pre-transition
    /// lifecycle epoch under the fence.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for a missing/foreign Character,
    /// and [`CoreError::ActorConflict`] `character_busy` when any Character
    /// activity is outstanding.
    pub async fn acquire_character_transition(
        &self,
        principal: &Principal,
        character_id: String,
    ) -> CoreResult<crate::actor_fence::CharacterTransitionLease> {
        self.verify_principal(principal)?;
        let pool = &self.inner.pool;
        require_character_row(pool, principal.creator_id(), &character_id).await?;
        let parts = self
            .inner
            .character_fences
            .try_acquire_transition(&character_id)?;
        // Re-read under the exclusive fence so the pre-transition epoch is
        // exact for material-vs-no-op session retirement (§11.3.2–§11.3.3).
        let stored = require_character_row(pool, principal.creator_id(), &character_id).await?;
        Ok(crate::actor_fence::CharacterTransitionLease::new(
            parts,
            principal.creator_id().to_string(),
            character_id,
            stored.lifecycle_epoch,
        ))
    }

    /// Commit a Character lifecycle transition while holding the exclusive
    /// lease from [`Self::acquire_character_transition`] (durable §11.3.2–
    /// §11.3.3). The lease proves the fence; the committed record's epoch is
    /// written back onto the lease so hosts can detect a material transition
    /// and retire old-epoch sessions while the fence is still held.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::InvalidInput`] when the lease does not
    /// belong to this principal/Character, [`CoreError::Forbidden`] under
    /// read-only access, and the mapped storage conflicts otherwise
    /// (`character_revision_conflict`, `character_restore_requires_active_binding`).
    pub async fn commit_character_transition(
        &self,
        principal: &Principal,
        lease: &mut crate::actor_fence::CharacterTransitionLease,
        request: CoreCharacterTransitionRequest,
    ) -> CoreResult<CoreCharacterTransitionResponse> {
        self.verify_principal(principal)?;
        if lease.owner_creator_id() != principal.creator_id()
            || lease.character_id() != request.character_id.as_str()
        {
            return Err(CoreError::InvalidInput {
                field: "character_id".to_string(),
                reason: "transition lease does not cover this Character".to_string(),
            });
        }
        if self.inner.access == crate::CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "character_transition: read-only core access".to_string(),
            });
        }
        let target = match request.target_status {
            nexus_contracts::generated::core::CoreCharacterTransitionRequestTargetStatus::Archived => {
                CharacterStatus::Archived
            }
            nexus_contracts::generated::core::CoreCharacterTransitionRequestTargetStatus::Active => {
                CharacterStatus::Active
            }
        };
        // §2.2: resolve the identity's holder through the same exported read the
        // admission paths use, before the lifecycle write. The storage layer
        // re-resolves it inside the transaction, so the archive/restore commit
        // still fails closed on a registry that changed under the fence.
        require_actor_holder(
            &self.inner.pool,
            principal.creator_id(),
            &AdmittedActor::Character {
                character_id: lease.character_id().to_string(),
            },
        )
        .await?;
        let record = nexus_local_db::transition_character(
            &self.inner.pool,
            principal.creator_id(),
            lease.character_id(),
            request.expected_revision,
            target,
        )
        .await
        .map_err(actor_db_err)?;
        lease.set_epoch(record.lifecycle_epoch);
        let character: nexus_contracts::generated::core::core_character_transition_response::NexusCharacter =
            map_wire_one(nexus_character_from_record(&record)?)?;
        CoreCharacterTransitionResponse::builder()
            .character(character)
            .try_into()
            .map_err(wire_err)
    }

    /// One-call Character lifecycle transition: exclusive lease acquisition
    /// (busy refusal + epoch/ownership re-read) then the revision-checked
    /// commit. Host session retirement is a host concern — hosts that must
    /// retire old-epoch sessions under the fence compose
    /// [`Self::acquire_character_transition`] +
    /// [`Self::commit_character_transition`] instead.
    ///
    /// # Errors
    /// See [`Self::acquire_character_transition`] and
    /// [`Self::commit_character_transition`].
    pub async fn transition_character(
        &self,
        principal: &Principal,
        request: CoreCharacterTransitionRequest,
    ) -> CoreResult<CoreCharacterTransitionResponse> {
        let mut lease = self
            .acquire_character_transition(principal, request.character_id.as_str().to_string())
            .await?;
        self.commit_character_transition(principal, &mut lease, request)
            .await
    }
}

/// Mint the token from the closed wire sum (schema-derived discriminator —
/// never an ownership claim).
fn admit_actor_token(
    caller_creator_id: &str,
    actor_ref: NexusActorRef,
) -> CoreResult<AdmittedActor> {
    Ok(match actor_ref {
        NexusActorRef::CreatorActorRef { creator_id, .. } => {
            let creator_id = creator_id.to_string();
            if creator_id != caller_creator_id {
                return Err(not_found("actor_ref", &creator_id));
            }
            AdmittedActor::Creator { creator_id }
        }
        NexusActorRef::CharacterActorRef { character_id, .. } => AdmittedActor::Character {
            character_id: character_id.to_string(),
        },
    })
}

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
fn admit_viewpoint(viewpoint: NexusSessionViewpoint) -> ActorViewpoint {
    ActorViewpoint {
        world_id: viewpoint.world_id.to_string(),
        binding_id: viewpoint.binding_id.as_ref().map(|id| id.to_string()),
        branch_id: viewpoint.branch_id.as_ref().map(|id| id.to_string()),
        event_id: viewpoint.event_id.as_ref().map(|id| id.to_string()),
    }
}

/// Validate stored ownership for an admission (durable §11.3 admission
/// ordering: Creator self-ownership, binding shape, active owned World,
/// active owned Character, active stored binding).
async fn require_admitted_ownership(
    pool: &SqlitePool,
    caller_creator_id: &str,
    actor: &AdmittedActor,
    viewpoint: &ActorViewpoint,
) -> CoreResult<()> {
    match actor {
        AdmittedActor::Creator { creator_id: _ } => {
            if viewpoint.binding_id.is_some() {
                return Err(invalid_input(
                    "binding_id must be omitted for Creator actor_ref",
                ));
            }
            require_active_owned_world(pool, caller_creator_id, &viewpoint.world_id).await?;
        }
        AdmittedActor::Character { character_id } => {
            let Some(binding) = viewpoint.binding_id.as_deref() else {
                return Err(invalid_input(
                    "binding_id is required for Character actor_ref",
                ));
            };
            require_active_owned_character(pool, caller_creator_id, character_id).await?;
            require_active_owned_world(pool, caller_creator_id, &viewpoint.world_id).await?;
            require_active_binding(pool, character_id, binding, &viewpoint.world_id).await?;
        }
    }
    // §2.1: the identity is usable only while its holder registry row exists;
    // admission is a normal read, so missing/corrupt registry state fails
    // closed here instead of provisioning a holder.
    require_actor_holder(pool, caller_creator_id, actor).await?;
    Ok(())
}

/// Resolve the stable holder of an admitted Actor's stored identity for a
/// normal read (durable §2.2).
///
/// Fails closed when the registry row is missing or corrupt
/// (`holder_state_invalid`): a normal read never provisions a holder, so an
/// identity whose subject committed without its registry row is not usable.
/// Foreign/missing rows stay the retained not-found shape, so this read does
/// not widen existence.
///
/// # Errors
///
/// Returns [`CoreError::NotFound`] for a foreign or missing subject,
/// [`CoreError::ActorConflict`] `holder_state_invalid` for missing/corrupt
/// registry state, and the mapped storage error otherwise.
pub async fn require_actor_holder(
    pool: &SqlitePool,
    caller_creator_id: &str,
    actor: &AdmittedActor,
) -> CoreResult<String> {
    match actor {
        AdmittedActor::Creator { creator_id } => {
            if creator_id != caller_creator_id {
                return Err(not_found("actor_ref", creator_id));
            }
            nexus_local_db::require_creator_holder(pool, creator_id)
                .await
                .map_err(actor_db_err)
        }
        AdmittedActor::Character { character_id } => {
            nexus_local_db::require_character_holder(pool, caller_creator_id, character_id)
                .await
                .map_err(actor_db_err)
        }
    }
}

/// Active owned World (PR #240 finding 1): foreign/missing → 404, owned but
/// inactive → 409 `world_inactive`.
pub async fn require_active_owned_world(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    let row =
        sqlx::query("SELECT owner_creator_id, status FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(&e))?;
    match row {
        Some(stored) => {
            let owner: String = stored.try_get("owner_creator_id").map_err(|e| db_err(&e))?;
            let status: String = stored.try_get("status").map_err(|e| db_err(&e))?;
            if owner == creator_id && status == "active" {
                Ok(())
            } else if owner == creator_id {
                Err(CoreError::ActorConflict {
                    code: "world_inactive".to_string(),
                    message: format!("world {world_id} is {status}"),
                })
            } else {
                Err(not_found("world", world_id))
            }
        }
        None => Err(not_found("world", world_id)),
    }
}

/// Active owned Character: foreign/missing → 404, owned but archived → 409
/// `character_inactive`. Returns the stored record for epoch capture.
pub async fn require_active_owned_character(
    pool: &SqlitePool,
    creator_id: &str,
    character_id: &str,
) -> CoreResult<CharacterRecord> {
    let stored = require_character_row(pool, creator_id, character_id).await?;
    if stored.status != "active" {
        return Err(CoreError::ActorConflict {
            code: "character_inactive".to_string(),
            message: format!("character {character_id} is {}", stored.status),
        });
    }
    Ok(stored)
}

/// Ownership-scoped Character row or a 404 (foreign ids are not
/// distinguished from missing).
pub async fn require_character_row(
    pool: &SqlitePool,
    creator_id: &str,
    character_id: &str,
) -> CoreResult<CharacterRecord> {
    nexus_local_db::get_character(pool, creator_id, character_id)
        .await
        .map_err(actor_db_err)?
        .ok_or_else(|| not_found("character", character_id))
}

/// Active stored binding tuple: the binding exists, belongs to
/// `character_id`, targets `world_id`, and is active; anything else is 404
/// (existence hidden).
pub async fn require_active_binding(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    let row = sqlx::query(
        "SELECT character_id, world_id, status FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(binding_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| db_err(&e))?;
    let Some(row) = row else {
        return Err(not_found("actor_world_binding", binding_id));
    };
    let stored_character: String = row.try_get("character_id").map_err(|e| db_err(&e))?;
    let stored_world: String = row.try_get("world_id").map_err(|e| db_err(&e))?;
    let status: String = row.try_get("status").map_err(|e| db_err(&e))?;
    if stored_character == character_id && stored_world == world_id && status == "active" {
        Ok(())
    } else {
        Err(not_found("actor_world_binding", binding_id))
    }
}

// ── Character / binding family ──────────────────────────────────────────

impl CoreService {
    /// Create an owned Character with its initial active binding in one
    /// atomic write (display name uniqueness, persona/`WorldSheet` validation
    /// and the FK-prechecks stay in the shared storage repository).
    ///
    /// # Errors
    /// Returns the mapped storage conflicts (`duplicate_character_display_name`,
    /// `invalid_world_sheet`, `world_has_actor_bindings` is delete-only) and
    /// [`CoreError::AuthRequired`] when the principal fails verification.
    pub async fn create_character(
        &self,
        principal: &Principal,
        request: CreateCharacterRequest,
    ) -> CoreResult<CreateCharacterResponse> {
        self.verify_principal(principal)?;
        require_write_access(self, "character_create")?;
        let persona = serde_json::Value::Object(request.persona.clone()).to_string();
        let created = nexus_local_db::create_character_with_initial_binding(
            &self.inner.pool,
            CreateCharacterParams {
                owner_creator_id: principal.creator_id(),
                display_name: request.display_name.as_str(),
                image_uri: optional_str(request.image_uri.as_ref()),
                persona_json: &persona,
                world_id: request.world_id.as_str(),
                world_sheet_entry_id: optional_str(request.world_sheet_entry_id.as_ref()),
            },
        )
        .await
        .map_err(actor_db_err)?;
        let character: DetailCharacterWire = nexus_character_from_record(&created.character)?;
        let binding: DetailBindingWire = binding_wire_from_record(&created.binding)?;
        let character: CreatedCharacterWire = map_wire_one(character)?;
        let binding: CreatedBindingWire = map_wire_one(binding)?;
        CreateCharacterResponse::builder()
            .character(character)
            .binding(binding)
            .try_into()
            .map_err(wire_err)
    }

    /// List the principal's Characters, oldest first, with offset pagination.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage error otherwise.
    pub async fn list_characters(
        &self,
        principal: &Principal,
        limit: u32,
        offset: u32,
    ) -> CoreResult<ListCharactersResponse> {
        self.verify_principal(principal)?;
        let fetch_limit = i64::from(limit) + 1;
        let page = nexus_local_db::list_characters(
            &self.inner.pool,
            principal.creator_id(),
            fetch_limit,
            i64::from(offset),
        )
        .await
        .map_err(actor_db_err)?;
        let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
        let items = page
            .iter()
            .take(limit as usize)
            .map(nexus_character_from_record)
            .collect::<CoreResult<Vec<_>>>()?;
        let mapped: Vec<ListedCharacterWire> = map_wire_items(items)?;
        ListCharactersResponse::builder()
            .items(mapped)
            .pagination(pagination_wire(limit, has_more, next_cursor)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Project one owned Character; foreign/unknown ids are 404.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and [`CoreError::NotFound`] for a foreign/unknown id.
    pub async fn character(
        &self,
        principal: &Principal,
        character_id: String,
    ) -> CoreResult<CharacterDetail> {
        self.verify_principal(principal)?;
        let row =
            require_character_row(&self.inner.pool, principal.creator_id(), &character_id).await?;
        CharacterDetail::builder()
            .character(nexus_character_from_record(&row)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Patch an owned, active Character behind the shared activity lease
    /// (CAS-checked in one `BEGIN IMMEDIATE` write).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, the mapped storage conflicts otherwise
    /// (`character_revision_conflict`, `character_inactive`), and the wire
    /// carrier when the committed record fails to project.
    pub async fn patch_character(
        &self,
        principal: &Principal,
        character_id: String,
        expected_revision: i64,
        patch: nexus_local_db::CharacterPatch<'_>,
    ) -> CoreResult<CharacterDetail> {
        self.verify_principal(principal)?;
        let _activity = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        // §2.2: the lifecycle write resolves the identity's holder through the
        // same exported read the admission paths use (the storage layer
        // re-resolves it inside its own transaction).
        require_actor_holder(
            &self.inner.pool,
            principal.creator_id(),
            &AdmittedActor::Character {
                character_id: character_id.clone(),
            },
        )
        .await?;
        let record = nexus_local_db::update_character(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            expected_revision,
            patch,
        )
        .await
        .map_err(actor_db_err)?;
        CharacterDetail::builder()
            .character(nexus_character_from_record(&record)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Add one active `ActorWorldBinding` behind the shared activity lease.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for foreign Character/World, and
    /// the mapped storage conflicts otherwise
    /// (`duplicate_active_actor_world_binding`, `invalid_world_sheet`).
    pub async fn add_binding(
        &self,
        principal: &Principal,
        character_id: String,
        world_id: String,
        world_sheet_entry_id: Option<String>,
    ) -> CoreResult<AddCharacterBindingResponse> {
        self.verify_principal(principal)?;
        let _activity = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        let binding = nexus_local_db::add_actor_world_binding(
            &self.inner.pool,
            CreateBindingParams {
                owner_creator_id: principal.creator_id(),
                character_id: &character_id,
                world_id: &world_id,
                world_sheet_entry_id: world_sheet_entry_id.as_deref(),
            },
        )
        .await
        .map_err(actor_db_err)?;
        let wire: DetailBindingWire = binding_wire_from_record(&binding)?;
        let wire: NexusActorWorldBinding = map_wire_one(wire)?;
        AddCharacterBindingResponse::builder()
            .binding(wire)
            .try_into()
            .map_err(wire_err)
    }

    /// List one owned Character's bindings; foreign Characters are 404.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and [`CoreError::NotFound`] for a foreign Character.
    pub async fn list_bindings(
        &self,
        principal: &Principal,
        character_id: String,
        limit: u32,
        offset: u32,
    ) -> CoreResult<ListCharacterBindingsResponse> {
        self.verify_principal(principal)?;
        let fetch_limit = i64::from(limit) + 1;
        let page = nexus_local_db::list_bindings_for_character(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            fetch_limit,
            i64::from(offset),
        )
        .await
        .map_err(actor_db_err)?;
        let (next_cursor, has_more) = offset_page_meta(page.len(), limit, offset);
        let items = page
            .iter()
            .take(limit as usize)
            .map(binding_wire_from_record)
            .collect::<CoreResult<Vec<_>>>()?;
        let mapped: Vec<ListedBindingWire> = map_wire_items(items)?;
        ListCharacterBindingsResponse::builder()
            .items(mapped)
            .pagination(binding_pagination_wire(limit, has_more, next_cursor)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Project one owned binding (retained reads tolerate archived
    /// Character/World status); foreign/missing/tuple-mismatched ids are 404.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and [`CoreError::NotFound`] for foreign/missing bindings.
    pub async fn binding(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: String,
    ) -> CoreResult<CharacterBindingDetail> {
        self.verify_principal(principal)?;
        let row = self
            .owned_binding_row(principal.creator_id(), &character_id, &binding_id)
            .await?
            .ok_or_else(|| not_found("binding", &binding_id))?;
        CharacterBindingDetail::builder()
            .binding(binding_wire_from_record(&row)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Patch the optional `WorldSheet` link on an owned active binding behind
    /// the shared activity lease (CAS-checked).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage conflicts otherwise
    /// (`binding_revision_conflict`, `character_inactive`, `invalid_world_sheet`).
    pub async fn patch_binding(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: String,
        expected_revision: i64,
        world_sheet_entry_id: nexus_local_db::FieldPatch<&str>,
    ) -> CoreResult<CharacterBindingDetail> {
        self.verify_principal(principal)?;
        let _activity = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        let record = nexus_local_db::update_actor_world_binding(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &binding_id,
            expected_revision,
            world_sheet_entry_id,
        )
        .await
        .map_err(actor_db_err)?;
        CharacterBindingDetail::builder()
            .binding(binding_wire_from_record(&record)?)
            .try_into()
            .map_err(wire_err)
    }

    /// Remove one active binding behind the shared activity lease. The last
    /// active binding is a zero-mutation 409
    /// (`last_active_actor_world_binding`), as is a binding that still owns
    /// knowledge or local memory.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage conflicts otherwise.
    pub async fn remove_binding(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: String,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        let _activity = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        nexus_local_db::remove_binding(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &binding_id,
        )
        .await
        .map_err(actor_db_err)
    }

    /// Owner-scoped binding detail row (`None` for foreign/missing rows).
    async fn owned_binding_row(
        &self,
        owner: &str,
        character_id: &str,
        binding_id: &str,
    ) -> CoreResult<Option<ActorWorldBindingRecord>> {
        nexus_local_db::get_actor_world_binding(&self.inner.pool, owner, character_id, binding_id)
            .await
            .map_err(actor_db_err)
    }
}

fn require_write_access(service: &CoreService, what: &str) -> CoreResult<()> {
    if service.inner.access == crate::CoreAccess::ReadOnly {
        return Err(CoreError::Forbidden {
            resource: format!("{what}: read-only core access"),
        });
    }
    Ok(())
}

fn optional_str(value: Option<&impl std::ops::Deref<Target = String>>) -> Option<&str> {
    value.map(|s| s.as_str())
}

/// Offset-backed page meta (retained `v1:` cursor convention).
pub fn offset_page_meta(fetched: usize, limit: u32, offset: u32) -> (Option<String>, bool) {
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    if fetched > limit_us {
        (Some(format!("v1:{}", offset.saturating_add(limit))), true)
    } else {
        (None, false)
    }
}

/// serde round-trip between the shared and response-local generated clones of
/// a wire type (the retained `map_wire` translation, now core-side).
pub fn map_wire_items<T: serde::Serialize, U: serde::de::DeserializeOwned>(
    items: Vec<T>,
) -> CoreResult<Vec<U>> {
    let value = serde_json::to_value(items).map_err(wire_err)?;
    serde_json::from_value(value).map_err(wire_err)
}

/// serde round-trip between the shared and response-local generated clones of
/// a wire type (the retained `map_wire` translation, now core-side).
pub fn map_wire_one<T: serde::Serialize, U: serde::de::DeserializeOwned>(
    value: T,
) -> CoreResult<U> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

/// Pool-bound stored admission for hosts that compose admission around
/// their own session registry (the daemon agent-host shim until P4-T2
/// consumes the core leases).
///
/// Carries the same stored-ownership business as [`CoreService::admit_actor`]
/// plus the bounded [`AdmittedActorContext`] assembly — no host/MCA/session
/// side effects.
pub struct CoreActorAdmission {
    views: crate::actor_knowledge::ActorKnowledgeViewService,
    pool: SqlitePool,
}

impl CoreActorAdmission {
    /// Bind admission to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            views: crate::actor_knowledge::ActorKnowledgeViewService::new(pool.clone()),
            pool,
        }
    }

    /// Admit stored Creator/Character/World/binding ownership and load the
    /// bounded view (durable §11.3 admission ordering).
    ///
    /// # Errors
    /// Auth, ownership, status, or view-composition failures.
    pub async fn admit(
        &self,
        caller_creator_id: &str,
        actor: AdmittedActor,
        viewpoint: ActorViewpoint,
    ) -> CoreResult<AdmittedActorContext> {
        let mut character_epoch = None;
        match &actor {
            AdmittedActor::Creator { creator_id } => {
                if creator_id != caller_creator_id {
                    return Err(not_found("actor_ref", creator_id));
                }
                if viewpoint.binding_id.is_some() {
                    return Err(invalid_input(
                        "binding_id must be omitted for Creator actor_ref",
                    ));
                }
                require_active_owned_world(&self.pool, caller_creator_id, &viewpoint.world_id)
                    .await?;
            }
            AdmittedActor::Character { character_id } => {
                let Some(binding) = viewpoint.binding_id.as_deref() else {
                    return Err(invalid_input(
                        "binding_id is required for Character actor_ref",
                    ));
                };
                let stored =
                    require_active_owned_character(&self.pool, caller_creator_id, character_id)
                        .await?;
                character_epoch = Some(stored.lifecycle_epoch);
                require_active_owned_world(&self.pool, caller_creator_id, &viewpoint.world_id)
                    .await?;
                self.views
                    .require_active_binding(character_id, binding, &viewpoint.world_id)
                    .await?;
            }
        }
        // §2.1/§2.2: the same fail-closed holder resolution as the core
        // admission path — a normal read never provisions a missing holder.
        require_actor_holder(&self.pool, caller_creator_id, &actor).await?;
        let view = self
            .views
            .admitted_view(
                caller_creator_id,
                &actor,
                &viewpoint.world_id,
                viewpoint.binding_id.clone(),
            )
            .await?;
        Ok(AdmittedActorContext {
            actor,
            owner_creator_id: caller_creator_id.to_string(),
            world_id: viewpoint.world_id,
            binding_id: viewpoint.binding_id,
            branch_id: viewpoint.branch_id,
            event_id: viewpoint.event_id,
            character_epoch,
            view,
        })
    }
}
