//! Stored Actor `KnowledgeView` ownership and Character knowledge commands
//! (v1.190 P2-T1).
//!
//! Extracted from the daemon `actor_knowledge_view` service and the
//! `actor_knowledge` HTTP handlers: the fail-closed view composer, the
//! guarded Character/binding knowledge-entry writes (activity-lease held),
//! and the bounded admission view. Callers never re-union owner sets
//! themselves; invalid ownership or any failed component query returns an
//! error and no partial page.

use nexus_knowledge::world_kb::knowledge_entry::{
    stored_created_at_order_millis, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::Row;
use sqlx::SqlitePool;
use crate::actors::{
    map_wire_one, require_active_binding, require_active_owned_character,
    require_active_owned_world, AdmittedActor,
};
use crate::error::{actor_db_err, actor_insert_db_err, db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
const CURSOR_PREFIX: &str = "k2:";
const CURSOR_SEP: char = '\u{1f}';

/// Wire-mapping failure carrier prefixes the daemon adapter re-sends as the
/// retained internal codes.
pub const KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX: &str =
    "actor_knowledge_view_component_failed";
pub const KNOWLEDGE_WIRE_INVALID_PREFIX: &str = "actor_knowledge_wire_invalid";
pub const KNOWLEDGE_INSERT_FAILED_PREFIX: &str = "actor_knowledge_insert_failed";

/// Keyset-paginated view page.
#[derive(Debug, Clone)]
pub struct ActorKnowledgePage {
    pub items: Vec<KnowledgeEntryRecord>,
    pub limit: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Admitted view query after stored-owner checks.
#[derive(Debug, Clone)]
pub struct ActorKnowledgeViewQuery {
    pub world_id: String,
    pub binding_id: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

/// One reusable `KnowledgeView` composer, bound to a workspace pool.
pub struct ActorKnowledgeViewService {
    store: SqliteKbStore,
    pool: SqlitePool,
}

fn invalid_cursor() -> CoreError {
    CoreError::ActorInput("cursor is not a valid opaque two-field keyset token".to_string())
}

fn timestamp_err(err: String) -> CoreError {
    CoreError::Internal {
        category: format!("{KNOWLEDGE_WIRE_INVALID_PREFIX}: {err}"),
    }
}

fn component_err(err: &KbStoreError) -> CoreError {
    CoreError::Internal {
        category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
    }
}

impl ActorKnowledgeViewService {
    /// Bind the service to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            store: SqliteKbStore::new(pool.clone()),
            pool,
        }
    }

    /// Resolve limit (1..=100, default 50).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ActorInput`] when `limit` is out of range.
    pub fn resolve_limit(raw: Option<i64>) -> CoreResult<u32> {
        match raw {
            None => Ok(DEFAULT_LIMIT),
            Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => {
                u32::try_from(n).map_err(|_| {
                    CoreError::ActorInput("limit is out of range".to_string())
                })
            }
            Some(_) => Err(CoreError::ActorInput(format!(
                "limit must be between 1 and {MAX_LIMIT}"
            ))),
        }
    }

    /// Decode the opaque two-field cursor `(created_at, key_block_id)`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ActorInput`] when the token is present but not a
    /// `k2:` pair.
    pub fn decode_cursor(
        cursor: &Option<String>,
    ) -> CoreResult<Option<(String, String)>> {
        match cursor {
            None => Ok(None),
            Some(raw) => {
                let rest = raw
                    .strip_prefix(CURSOR_PREFIX)
                    .ok_or_else(invalid_cursor)?;
                let parts: Vec<&str> = rest.split(CURSOR_SEP).collect();
                if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                    return Err(invalid_cursor());
                }
                Ok(Some((parts[0].to_string(), parts[1].to_string())))
            }
        }
    }

    /// Encode `(created_at, key_block_id)` as an opaque cursor.
    #[must_use]
    pub fn encode_cursor(created_at: &str, entry_id: &str) -> String {
        format!("{CURSOR_PREFIX}{created_at}{CURSOR_SEP}{entry_id}")
    }

    /// Merge already-fetched owner components and take the first `limit` rows
    /// in `(timestamp_millis, entry_id)` order, matching SQL keyset precision.
    ///
    /// # Errors
    ///
    /// Returns the `actor_knowledge_wire_invalid` carrier when a stored
    /// timestamp is neither RFC3339 nor SQLite `datetime('now')`.
    pub fn paginate(
        items: Vec<KnowledgeEntryRecord>,
        cursor: Option<(String, String)>,
        limit: u32,
    ) -> CoreResult<ActorKnowledgePage> {
        let mut keyed = Vec::with_capacity(items.len());
        for row in items {
            let millis =
                stored_created_at_order_millis(&row.created_at).map_err(timestamp_err)?;
            keyed.push((millis, row.entry_id.clone(), row));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if let Some((created_at, entry_id)) = cursor {
            let cursor_ms = stored_created_at_order_millis(&created_at)
                .map_err(|_| invalid_cursor())?;
            keyed.retain(|(ms, id, _)| (*ms, id.as_str()) > (cursor_ms, entry_id.as_str()));
        }
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = keyed.len() > limit_us;
        keyed.truncate(limit_us);
        let items: Vec<KnowledgeEntryRecord> = keyed.into_iter().map(|(_, _, row)| row).collect();
        let next_cursor = if has_more {
            items
                .last()
                .map(|row| Self::encode_cursor(&row.created_at, &row.entry_id))
        } else {
            None
        };
        Ok(ActorKnowledgePage {
            items,
            limit,
            has_more,
            next_cursor,
        })
    }

    /// Admit `actor_ref` from stored rows and compose the locked view.
    ///
    /// # Errors
    ///
    /// Auth/ownership failures, invalid cursors, or any failed component query.
    pub async fn view(
        &self,
        caller_creator_id: &str,
        actor: &AdmittedActor,
        query: ActorKnowledgeViewQuery,
    ) -> CoreResult<ActorKnowledgePage> {
        let cursor = Self::decode_cursor(&query.cursor)?;
        match actor {
            AdmittedActor::Creator { creator_id } => {
                if creator_id != caller_creator_id {
                    return Err(not_found("actor_ref", creator_id));
                }
                // Creator World-selected view is a retained read: it includes
                // owned archived Character/binding scopes and does not drop
                // the World's owned retained history when archived (§11.2).
                require_owned_world(&self.pool, caller_creator_id, &query.world_id).await?;
                let parts = self
                    .creator_union(
                        caller_creator_id,
                        &query.world_id,
                        cursor.as_ref(),
                        query.limit,
                    )
                    .await?;
                Self::paginate(parts, None, query.limit)
            }
            AdmittedActor::Character { character_id } => {
                let Some(binding_id) = query.binding_id.as_deref() else {
                    return Err(CoreError::ActorInput(
                        "binding_id is required for Character KnowledgeView".to_string(),
                    ));
                };
                // Retained read (durable §11.2): authorize by owner + stored
                // binding tuple, not liveness. A foreign/missing Character,
                // a cross-Character binding, or a missing/foreign World still
                // fails closed.
                require_owned_character(&self.pool, caller_creator_id, character_id).await?;
                require_owned_world(&self.pool, caller_creator_id, &query.world_id).await?;
                require_stored_binding_tuple(&self.pool, character_id, binding_id, &query.world_id)
                    .await?;
                let parts = self
                    .character_union(
                        &query.world_id,
                        character_id,
                        binding_id,
                        cursor.as_ref(),
                        query.limit,
                    )
                    .await?;
                Self::paginate(parts, None, query.limit)
            }
        }
    }

    /// Character-owned listing without a World filter.
    ///
    /// # Errors
    ///
    /// Missing Character or a failed owner query.
    pub async fn list_character_owned(
        &self,
        caller_creator_id: &str,
        character_id: &str,
        limit: u32,
        cursor: Option<String>,
    ) -> CoreResult<ActorKnowledgePage> {
        require_owned_character(&self.pool, caller_creator_id, character_id).await?;
        let cursor = Self::decode_cursor(&cursor)?;
        let items = self
            .component(
                KnowledgeOwnerRef::character(character_id),
                cursor.as_ref(),
                limit,
                false,
            )
            .await?;
        Self::paginate(items, None, limit)
    }

    async fn creator_union(
        &self,
        creator_id: &str,
        world_id: &str,
        cursor: Option<&(String, String)>,
        limit: u32,
    ) -> CoreResult<Vec<KnowledgeEntryRecord>> {
        let mut items = self
            .component(KnowledgeOwnerRef::world(world_id), cursor, limit, false)
            .await?;
        // Retained read (durable §11.2): include owned archived Character and
        // binding scopes — never silently drop them with `status = 'active'`.
        let bindings = sqlx::query(
            "SELECT b.character_id, b.binding_id \
             FROM actor_world_bindings b \
             INNER JOIN characters c ON c.character_id = b.character_id \
             WHERE b.world_id = ? AND c.owner_creator_id = ?",
        )
        .bind(world_id)
        .bind(creator_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|err| CoreError::Internal {
            category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
        })?;

        let mut seen_characters = std::collections::BTreeSet::new();
        for row in bindings {
            let character_id: String = row
                .try_get("character_id")
                .map_err(|err| CoreError::Internal {
                    category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
                })?;
            let binding_id: String = row.try_get("binding_id").map_err(|err| {
                CoreError::Internal {
                    category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
                }
            })?;
            if seen_characters.insert(character_id.clone()) {
                items.extend(
                    self.component(
                        KnowledgeOwnerRef::character(&character_id),
                        cursor,
                        limit,
                        false,
                    )
                    .await?,
                );
            }
            items.extend(
                self.component(
                    KnowledgeOwnerRef::actor_world_binding(&binding_id),
                    cursor,
                    limit,
                    false,
                )
                .await?,
            );
        }
        Ok(items)
    }

    async fn character_union(
        &self,
        world_id: &str,
        character_id: &str,
        binding_id: &str,
        cursor: Option<&(String, String)>,
        limit: u32,
    ) -> CoreResult<Vec<KnowledgeEntryRecord>> {
        let mut items = self
            .component(KnowledgeOwnerRef::world(world_id), cursor, limit, true)
            .await?;
        items.extend(
            self.component(
                KnowledgeOwnerRef::character(character_id),
                cursor,
                limit,
                false,
            )
            .await?,
        );
        items.extend(
            self.component(
                KnowledgeOwnerRef::actor_world_binding(binding_id),
                cursor,
                limit,
                false,
            )
            .await?,
        );
        Ok(items)
    }

    async fn component(
        &self,
        owner: KnowledgeOwnerRef,
        cursor: Option<&(String, String)>,
        limit: u32,
        exclude_creator_only: bool,
    ) -> CoreResult<Vec<KnowledgeEntryRecord>> {
        self.store
            .list_by_owner_keyset(
                &owner,
                cursor,
                limit.saturating_add(1),
                exclude_creator_only,
            )
            .await
            .map_err(|e| component_err(&e))
    }

    /// The P1 bounded admission view: keyset pages merged into one
    /// hard-capped 200-item page (`actor_knowledge_view_incomplete` when the
    /// cap cannot be satisfied).
    pub async fn admitted_view(
        &self,
        caller_creator_id: &str,
        actor: &AdmittedActor,
        world_id: &str,
        binding_id: Option<String>,
    ) -> CoreResult<ActorKnowledgePage> {
        complete_view(self, caller_creator_id, actor, world_id, binding_id).await
    }

    /// Owned World with no status requirement (retained reads, durable §11.2).
    pub async fn require_owned_world(
        &self,
        creator_id: &str,
        world_id: &str,
    ) -> CoreResult<()> {
        require_owned_world(&self.pool, creator_id, world_id).await
    }

    /// Owned Character with no status requirement (retained reads).
    pub async fn require_owned_character(
        &self,
        creator_id: &str,
        character_id: &str,
    ) -> CoreResult<()> {
        require_owned_character(&self.pool, creator_id, character_id).await
    }

    /// Stored binding tuple with no status requirement (retained reads).
    pub async fn require_stored_binding_tuple(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> CoreResult<()> {
        require_stored_binding_tuple(&self.pool, character_id, binding_id, world_id).await
    }

    /// Active stored binding tuple (write admission).
    pub async fn require_active_binding(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> CoreResult<()> {
        require_active_binding(&self.pool, character_id, binding_id, world_id).await
    }
}

fn not_found(resource: &str, id: &str) -> CoreError {
    CoreError::NotFound {
        resource: format!("{resource} {id}"),
    }
}

/// Owned World with no status requirement (retained reads, durable §11.2):
/// a World still owned but paused/archived keeps its retained history
/// readable. Missing/foreign Worlds fail closed.
pub(crate) async fn require_owned_world(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    let row = sqlx::query("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
        .bind(world_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(&e))?;
    match row {
        Some(stored) => {
            let owner: String = stored
                .try_get("owner_creator_id")
                .map_err(|e| db_err(&e))?;
            if owner == creator_id {
                Ok(())
            } else {
                Err(not_found("world", world_id))
            }
        }
        None => Err(not_found("world", world_id)),
    }
}

/// Owned Character with no status requirement (retained reads): an archived
/// Character's authorized data stays readable, never filtered out.
/// Foreign/missing Characters are 404.
pub(crate) async fn require_owned_character(
    pool: &SqlitePool,
    creator_id: &str,
    character_id: &str,
) -> CoreResult<()> {
    match nexus_local_db::get_character(pool, creator_id, character_id)
        .await
        .map_err(actor_db_err)?
    {
        Some(_) => Ok(()),
        None => Err(not_found("character", character_id)),
    }
}

/// Stored binding tuple with no status requirement (retained reads): the
/// binding exists, belongs to `character_id`, and targets `world_id`
/// (durable §11.2). A cross-Character binding or missing binding fails
/// closed; binding status is not a liveness gate for reads.
pub(crate) async fn require_stored_binding_tuple(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    let row = sqlx::query(
        "SELECT character_id, world_id FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(binding_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| db_err(&e))?;
    match row {
        Some(stored) => {
            let stored_character: String =
                stored.try_get("character_id").map_err(|e| db_err(&e))?;
            let stored_world: String = stored.try_get("world_id").map_err(|e| db_err(&e))?;
            if stored_character == character_id && stored_world == world_id {
                Ok(())
            } else {
                Err(not_found("actor_world_binding", binding_id))
            }
        }
        None => Err(not_found("actor_world_binding", binding_id)),
    }
}

// ── CoreService commands ────────────────────────────────────────────────

impl CoreService {
    /// Compose the admitted Actor's bounded `KnowledgeView` (the P1 view:
    /// keyset pages merged to one hard-capped 200-item page, `view_incomplete`
    /// when the cap cannot be satisfied).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, the admission ownership errors, and the
    /// `actor_knowledge_view_incomplete` conflict when the bounded page
    /// cannot be composed.
    pub async fn admitted_actor_view(
        &self,
        principal: &Principal,
        actor: &AdmittedActor,
        world_id: String,
        binding_id: Option<String>,
    ) -> CoreResult<ActorKnowledgePage> {
        self.verify_principal(principal)?;
        let views = ActorKnowledgeViewService::new(self.inner.pool.clone());
        complete_view(
            &views,
            principal.creator_id(),
            actor,
            &world_id,
            binding_id.clone(),
        )
        .await
    }

    /// Compose the admitted `KnowledgeView` for an explicit query
    /// (`POST /v1/daemon/actor-knowledge/view`): stored-owner admission and
    /// the locked union.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the view admission errors otherwise.
    pub async fn actor_knowledge_view(
        &self,
        principal: &Principal,
        actor: &AdmittedActor,
        query: ActorKnowledgeViewQuery,
    ) -> CoreResult<ActorKnowledgePage> {
        self.verify_principal(principal)?;
        let views = ActorKnowledgeViewService::new(self.inner.pool.clone());
        views.view(principal.creator_id(), actor, query).await
    }

    /// List one owned Character's knowledge entries without a World filter.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and [`CoreError::NotFound`] for a foreign Character.
    pub async fn list_character_knowledge(
        &self,
        principal: &Principal,
        character_id: String,
        limit: u32,
        cursor: Option<String>,
    ) -> CoreResult<ActorKnowledgePage> {
        self.verify_principal(principal)?;
        let views = ActorKnowledgeViewService::new(self.inner.pool.clone());
        views
            .list_character_owned(principal.creator_id(), &character_id, limit, cursor)
            .await
    }

    /// Create one knowledge entry under a validated stored owner. World-owned
    /// inserts keep the retained route-check texture; Character/binding-owned
    /// inserts revalidate the stored active owner inside the write
    /// transaction (durable §11.3.5) with the activity lease held.
    ///
    /// `summary_present` preserves the retained wire distinction between an
    /// absent and a null `summary` member.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::ActorInput`] for shape violations, and the
    /// mapped storage/kb-store conflicts otherwise.
    pub async fn add_actor_knowledge_entry(
        &self,
        principal: &Principal,
        request: nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequest,
        summary_present: bool,
    ) -> CoreResult<KnowledgeEntryRecord> {
        use nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequestOwnerKind;

        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        let creator_only = request.creator_only.unwrap_or(false);
        let owner = match request.owner_kind {
            AddKnowledgeEntryRequestOwnerKind::World => {
                if summary_present {
                    return Err(CoreError::ActorInput(
                        "summary is not accepted on world-owned knowledge create".to_string(),
                    ));
                }
                let world_id = str_field(request.world_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "world_id is required for world-owned knowledge".to_string(),
                    )
                })?;
                require_active_owned_world(&self.inner.pool, creator_id, world_id).await?;
                KnowledgeOwnerRef::world(world_id.to_string())
            }
            AddKnowledgeEntryRequestOwnerKind::Character => {
                if creator_only {
                    return Err(CoreError::ActorInput(
                        "creator_only is World-owned only".to_string(),
                    ));
                }
                let character_id = str_field(request.character_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "character_id is required for character-owned knowledge".to_string(),
                    )
                })?;
                require_active_owned_character(&self.inner.pool, creator_id, character_id).await?;
                KnowledgeOwnerRef::character(character_id.to_string())
            }
            AddKnowledgeEntryRequestOwnerKind::ActorWorldBinding => {
                if creator_only {
                    return Err(CoreError::ActorInput(
                        "creator_only is World-owned only".to_string(),
                    ));
                }
                let character_id = str_field(request.character_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "character_id is required for binding-owned knowledge".to_string(),
                    )
                })?;
                let binding_id = str_field(request.binding_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "binding_id is required for binding-owned knowledge".to_string(),
                    )
                })?;
                let world_id = str_field(request.world_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "world_id is required for binding-owned knowledge".to_string(),
                    )
                })?;
                require_active_owned_character(&self.inner.pool, creator_id, character_id).await?;
                require_active_owned_world(&self.inner.pool, creator_id, world_id).await?;
                require_active_binding(&self.inner.pool, character_id, binding_id, world_id)
                    .await?;
                KnowledgeOwnerRef::actor_world_binding(binding_id.to_string())
            }
        };
        let block_type: nexus_contracts::BlockType = map_wire_one(request.block_type)?;
        let mut record = match &owner {
            KnowledgeOwnerRef::World(id) => {
                KnowledgeEntryRecord::new(id, block_type, request.canonical_name.as_str())
            }
            KnowledgeOwnerRef::Character(id) => {
                KnowledgeEntryRecord::for_character(id, block_type, request.canonical_name.as_str())
            }
            KnowledgeOwnerRef::ActorWorldBinding(id) => {
                KnowledgeEntryRecord::for_binding(
                    id,
                    block_type,
                    request.canonical_name.as_str(),
                )
            }
        };
        record.creator_only = creator_only;
        if matches!(
            &owner,
            KnowledgeOwnerRef::Character(_) | KnowledgeOwnerRef::ActorWorldBinding(_)
        ) {
            if let Some(text) = str_field(request.summary.as_ref()) {
                record.body = Some(
                    nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody {
                        summary: Some(text.to_string()),
                        ..nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody::default()
                    },
                );
            }
        }
        let store = SqliteKbStore::new(self.inner.pool.clone());
        let inserted = match &owner {
            KnowledgeOwnerRef::World(_) => {
                store
                    .insert_knowledge_entry(record.clone())
                    .await
                    .map_err(kb_insert_err)?
                    .entry_id
            }
            KnowledgeOwnerRef::Character(id) => {
                let _activity = self
                    .acquire_actor_activity(
                        principal,
                        &AdmittedActor::Character {
                            character_id: id.clone(),
                        },
                    )
                    .await?;
                store
                    .insert_actor_owned_key_block(creator_id, id.as_str(), None, record.clone())
                    .await
                    .map_err(actor_insert_db_err)?
                    .entry_id
            }
            KnowledgeOwnerRef::ActorWorldBinding(id) => {
                let admitted_character =
                    str_field(request.character_id.as_ref()).ok_or_else(|| {
                        CoreError::Internal {
                            category: format!(
                                "{KNOWLEDGE_INSERT_FAILED_PREFIX}: binding-owned KE insert \
                                 must carry the admitted Character"
                            ),
                        }
                    })?;
                let _activity = self
                    .acquire_actor_activity(
                        principal,
                        &AdmittedActor::Character {
                            character_id: admitted_character.to_string(),
                        },
                    )
                    .await?;
                store
                    .insert_actor_owned_key_block(
                        creator_id,
                        admitted_character,
                        Some(id.as_str()),
                        record.clone(),
                    )
                    .await
                    .map_err(actor_insert_db_err)?
                    .entry_id
            }
        };
        store
            .get_knowledge_entry(&inserted)
            .await
            .map_err(kb_insert_err)
    }

    /// Read one owned Character knowledge entry (existence hidden for
    /// foreign/missing rows).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and [`CoreError::NotFound`] for a foreign/missing entry.
    pub async fn actor_knowledge_entry(
        &self,
        principal: &Principal,
        character_id: String,
        entry_id: String,
    ) -> CoreResult<KnowledgeEntryRecord> {
        self.verify_principal(principal)?;
        nexus_local_db::get_actor_knowledge_entry(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &entry_id,
        )
        .await
        .map_err(actor_db_err)?
        .ok_or_else(|| not_found("knowledge_entry", &entry_id))
    }

    /// Patch one owned Character knowledge entry behind the shared activity
    /// lease (CAS-checked).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage conflicts otherwise
    /// (`knowledge_revision_conflict`, `knowledge_entry_not_mutable`).
    pub async fn patch_actor_knowledge_entry(
        &self,
        principal: &Principal,
        character_id: String,
        entry_id: String,
        expected_revision: i64,
        canonical_name_patch: Option<&str>,
        summary_patch: nexus_local_db::FieldPatch<&str>,
    ) -> CoreResult<KnowledgeEntryRecord> {
        self.verify_principal(principal)?;
        let _activity = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        nexus_local_db::update_actor_knowledge_entry(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &entry_id,
            expected_revision,
            nexus_local_db::ActorKnowledgePatch {
                canonical_name: canonical_name_patch,
                summary: summary_patch,
            },
        )
        .await
        .map_err(actor_db_err)
    }

    /// Delete one owned Character knowledge entry behind the shared activity
    /// lease (CAS-checked).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage conflicts otherwise
    /// (`knowledge_revision_conflict`, `knowledge_entry_in_use`).
    pub async fn delete_actor_knowledge_entry(
        &self,
        principal: &Principal,
        character_id: String,
        entry_id: String,
        expected_revision: i64,
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
        nexus_local_db::delete_actor_knowledge_entry(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &entry_id,
            expected_revision,
        )
        .await
        .map_err(actor_db_err)
    }
}

/// The P1 bounded admission view: merge keyset pages up to the hard cap.
async fn complete_view(
    views: &ActorKnowledgeViewService,
    caller_creator_id: &str,
    actor: &AdmittedActor,
    world_id: &str,
    binding_id: Option<String>,
) -> CoreResult<ActorKnowledgePage> {
    const HARD_CAP: usize = 200;
    let limit = ActorKnowledgeViewService::resolve_limit(Some(100))?;
    let mut items = Vec::new();
    let mut cursor = None;
    loop {
        if items.len() >= HARD_CAP {
            return Err(view_incomplete());
        }
        let page = views
            .view(
                caller_creator_id,
                actor,
                ActorKnowledgeViewQuery {
                    world_id: world_id.to_string(),
                    binding_id: binding_id.clone(),
                    limit,
                    cursor,
                },
            )
            .await?;
        if page.has_more && page.next_cursor.is_none() {
            return Err(view_incomplete());
        }
        items.extend(page.items);
        if items.len() > HARD_CAP {
            return Err(view_incomplete());
        }
        if !page.has_more {
            return Ok(ActorKnowledgePage {
                items,
                limit,
                has_more: false,
                next_cursor: None,
            });
        }
        if items.len() >= HARD_CAP {
            return Err(view_incomplete());
        }
        cursor = page.next_cursor;
    }
}

/// The retained `409 view_incomplete` refusal (verbatim daemon code and
/// wording; the adapter re-renders it via `ActorConflict`).
fn view_incomplete() -> CoreError {
    CoreError::ActorConflict {
        code: "view_incomplete".to_string(),
        message: "admitted KnowledgeView exceeded the hard entry cap or pagination was malformed"
            .to_string(),
    }
}

/// Retained guarded-create kb-store mapping: contract/validation conflicts
/// are plain `invalid_input`, everything else is the insert-failed carrier.
fn kb_insert_err(err: KbStoreError) -> CoreError {
    match err {
        KbStoreError::Duplicate { .. } | KbStoreError::Validation(_) | KbStoreError::ValidationLegacy(_) => {
            CoreError::ActorInput(err.to_string())
        }
        other => CoreError::Internal {
            category: format!("{KNOWLEDGE_INSERT_FAILED_PREFIX}: {other}"),
        },
    }
}

fn str_field<'a, T: std::ops::Deref<Target = String>>(value: Option<&'a T>) -> Option<&'a str> {
    value.map(|s| s.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

    fn rec(created_at: &str, entry_id: &str) -> KnowledgeEntryRecord {
        let mut row = KnowledgeEntryRecord::new("wld_worldA", BlockType::Character, entry_id);
        row.created_at = created_at.to_string();
        row.entry_id = entry_id.to_string();
        row
    }

    #[test]
    fn paginate_orders_created_at_then_entry_id_and_emits_k2_cursor() {
        let page = ActorKnowledgeViewService::paginate(
            vec![
                rec("2026-01-01T00:00:02Z", "kb_b"),
                rec("2026-01-01T00:00:01Z", "kb_z"),
                rec("2026-01-01T00:00:01Z", "kb_a"),
                rec("2026-01-01T00:00:03Z", "kb_c"),
            ],
            None,
            2,
        )
        .expect("page1");
        assert_eq!(
            page.items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_a", "kb_z"]
        );
        assert!(page.has_more);
        let cursor = page.next_cursor.expect("next_cursor");
        assert!(cursor.starts_with("k2:"));
        assert!(cursor.contains('\u{1f}'));
        let decoded = ActorKnowledgeViewService::decode_cursor(&Some(cursor))
            .expect("decode")
            .expect("pair");
        assert_eq!(decoded, ("2026-01-01T00:00:01Z".into(), "kb_z".into()));

        let page2 = ActorKnowledgeViewService::paginate(
            vec![
                rec("2026-01-01T00:00:02Z", "kb_b"),
                rec("2026-01-01T00:00:01Z", "kb_z"),
                rec("2026-01-01T00:00:01Z", "kb_a"),
                rec("2026-01-01T00:00:03Z", "kb_c"),
            ],
            Some(decoded),
            2,
        )
        .expect("page2");
        assert_eq!(
            page2
                .items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_b", "kb_c"]
        );
        assert!(!page2.has_more);
        assert!(page2.next_cursor.is_none());
    }

    #[test]
    fn decode_cursor_rejects_malformed_tokens() {
        for bad in [
            "v1:12",
            "k2:",
            "k2:only",
            "",
            "k2:\u{1f}id",
            "k2:ts\u{1f}",
            "k2:ts\u{1f}id\u{1f}unexpected",
            "k2:ts\u{1f}id\u{1f}",
        ] {
            let err = ActorKnowledgeViewService::decode_cursor(&Some(bad.into()))
                .expect_err(bad);
            match err {
                CoreError::ActorInput(_) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(ActorKnowledgeViewService::decode_cursor(&None)
            .expect("none")
            .is_none());
    }

    #[test]
    fn paginate_orders_mixed_sqlite_and_rfc3339_chronologically() {
        let page = ActorKnowledgeViewService::paginate(
            vec![
                rec("2026-01-01 00:00:02", "kb_space_late"),
                rec("2026-01-01T00:00:01Z", "kb_rfc_early"),
                rec("2026-01-01T00:00:02+00:00", "kb_rfc_tie"),
            ],
            None,
            2,
        )
        .expect("mixed");
        assert_eq!(
            page.items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_rfc_early", "kb_rfc_tie"]
        );
        assert!(page.has_more);
        let cursor = page.next_cursor.expect("cursor");
        let page2 = ActorKnowledgeViewService::paginate(
            vec![
                rec("2026-01-01 00:00:02", "kb_space_late"),
                rec("2026-01-01T00:00:01Z", "kb_rfc_early"),
                rec("2026-01-01T00:00:02+00:00", "kb_rfc_tie"),
            ],
            ActorKnowledgeViewService::decode_cursor(&Some(cursor)).unwrap(),
            2,
        )
        .expect("mixed2");
        assert_eq!(
            page2
                .items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_space_late"]
        );
        assert!(!page2.has_more);
    }

    #[test]
    fn paginate_same_millisecond_reverse_ids_does_not_skip() {
        let rows = vec![
            rec("2026-01-01T10:00:00.123200Z", "kb_m"),
            rec("2026-01-01T10:00:00.123300Z", "kb_a"),
        ];
        let page1 = ActorKnowledgeViewService::paginate(rows.clone(), None, 1).expect("p1");
        assert_eq!(
            page1
                .items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_a"]
        );
        assert!(page1.has_more);
        let cursor = ActorKnowledgeViewService::decode_cursor(&page1.next_cursor)
            .unwrap()
            .expect("cursor");
        let page2 = ActorKnowledgeViewService::paginate(rows, Some(cursor), 1).expect("p2");
        assert_eq!(
            page2
                .items
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec!["kb_m"]
        );
        assert!(!page2.has_more);
    }
}
