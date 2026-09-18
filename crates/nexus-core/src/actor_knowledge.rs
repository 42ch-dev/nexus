//! Stored Actor `KnowledgeView` ownership and Character knowledge commands
//! (v1.190 P2-T1).
//!
//! Extracted from the daemon `actor_knowledge_view` service and the
//! `actor_knowledge` HTTP handlers: the fail-closed view composer, the
//! guarded Character/binding knowledge-entry writes (activity-lease held),
//! and the bounded admission view. Callers never re-union owner sets
//! themselves; invalid ownership or any failed component query returns an
//! error and no partial page.

use crate::actors::{
    map_wire_one, require_actor_holder, require_active_binding, require_active_owned_character,
    require_active_owned_world, require_admitted_ownership, ActorViewpoint, AdmittedActor,
};
use crate::error::{actor_db_err, actor_insert_db_err, db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequestAudience;
use nexus_contracts::generated::daemon_api::actor_knowledge::update_knowledge_entry_request::UpdateKnowledgeEntryRequestAudience;
use nexus_knowledge::world_kb::errors::KbError;
use nexus_knowledge::world_kb::knowledge_entry::{
    stored_created_at_order_millis, KnowledgeAudience, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::store::{
    KbStore, KbStoreError, KnowledgeReadPolicy, KnowledgeReadScope,
};
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::Row;
use sqlx::SqlitePool;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
const CURSOR_PREFIX: &str = "k2:";
const CURSOR_SEP: char = '\u{1f}';

/// Wire-mapping failure carrier prefixes the daemon adapter re-sends as the
/// retained internal codes.
pub const KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX: &str = "actor_knowledge_view_component_failed";
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

#[allow(clippy::needless_pass_by_value)] // callers move the owned payload in
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
            Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => u32::try_from(n)
                .map_err(|_| CoreError::ActorInput("limit is out of range".to_string())),
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
    pub fn decode_cursor(cursor: &Option<String>) -> CoreResult<Option<(String, String)>> {
        match cursor {
            None => Ok(None),
            Some(raw) => {
                let rest = raw.strip_prefix(CURSOR_PREFIX).ok_or_else(invalid_cursor)?;
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
            let millis = stored_created_at_order_millis(&row.created_at).map_err(timestamp_err)?;
            keyed.push((millis, row.entry_id.clone(), row));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if let Some((created_at, entry_id)) = cursor {
            let cursor_ms =
                stored_created_at_order_millis(&created_at).map_err(|_| invalid_cursor())?;
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

    /// Resolve the `ActorView` selection for one stored Actor + viewpoint
    /// (durable §4.1/§4.2).
    ///
    /// The selection is chosen by this function, never by the request: the
    /// authorized containers come from the stored ownership rows, and the
    /// admitted holder comes from the stored identity's holder registry entry.
    /// Retained read texture (durable §11.2): the owned subject is authorized
    /// without a liveness gate, so an archived World/Character keeps its
    /// containers readable — the resolution still fails closed on a
    /// foreign/missing subject or a missing registry row.
    ///
    /// # Errors
    ///
    /// Ownership/actor-shape failures, the fail-closed `holder_state_invalid`,
    /// and invalid cursors.
    pub async fn actor_view_scope(
        &self,
        caller_creator_id: &str,
        actor: &AdmittedActor,
        world_id: &str,
        binding_id: Option<&str>,
    ) -> CoreResult<KnowledgeReadScope> {
        let mut containers = Vec::new();
        let holder_entry_id = match actor {
            AdmittedActor::Creator { creator_id } => {
                if creator_id != caller_creator_id {
                    return Err(not_found("actor_ref", creator_id));
                }
                // Creator World-selected view is a retained read: it includes
                // owned archived Character/binding scopes and does not drop
                // the World's owned retained history when archived (§11.2).
                require_owned_world(&self.pool, caller_creator_id, world_id).await?;
                containers.push(KnowledgeOwnerRef::world(world_id));
                containers.extend(
                    self.owned_world_containers(caller_creator_id, world_id)
                        .await?,
                );
                require_actor_holder(&self.pool, caller_creator_id, actor).await?
            }
            AdmittedActor::Character { character_id } => {
                let Some(binding_id) = binding_id else {
                    return Err(CoreError::ActorInput(
                        "binding_id is required for Character KnowledgeView".to_string(),
                    ));
                };
                // Retained read (durable §11.2): authorize by owner + stored
                // binding tuple, not liveness. A foreign/missing Character,
                // a cross-Character binding, or a missing/foreign World still
                // fails closed.
                require_owned_character(&self.pool, caller_creator_id, character_id).await?;
                require_owned_world(&self.pool, caller_creator_id, world_id).await?;
                require_stored_binding_tuple(&self.pool, character_id, binding_id, world_id)
                    .await?;
                containers.push(KnowledgeOwnerRef::world(world_id));
                containers.push(KnowledgeOwnerRef::character(character_id.as_str()));
                containers.push(KnowledgeOwnerRef::actor_world_binding(binding_id));
                require_actor_holder(&self.pool, caller_creator_id, actor).await?
            }
        };
        KnowledgeReadScope::actor_view(holder_entry_id, containers).map_err(|err| {
            CoreError::ActorConflict {
                code: "holder_state_invalid".to_string(),
                message: err.to_string(),
            }
        })
    }

    /// Owned Character/binding containers of one World (retained reads include
    /// archived Characters), deduplicated per Character and ordered
    /// World → Character → binding.
    async fn owned_world_containers(
        &self,
        creator_id: &str,
        world_id: &str,
    ) -> CoreResult<Vec<KnowledgeOwnerRef>> {
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

        let mut owners = Vec::new();
        let mut seen_characters = std::collections::BTreeSet::new();
        for row in bindings {
            let character_id: String =
                row.try_get("character_id")
                    .map_err(|err| CoreError::Internal {
                        category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
                    })?;
            let binding_id: String =
                row.try_get("binding_id")
                    .map_err(|err| CoreError::Internal {
                        category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
                    })?;
            if seen_characters.insert(character_id.clone()) {
                owners.push(KnowledgeOwnerRef::character(&character_id));
            }
            owners.push(KnowledgeOwnerRef::actor_world_binding(&binding_id));
        }
        Ok(owners)
    }

    /// Compose one policy-filtered keyset page for an admitted selection.
    ///
    /// Every selection container is fetched through the same §4.2 eligibility
    /// predicate, so the union holds eligible rows only and the merged page —
    /// including its `has_more` flag and next cursor — is computed over that
    /// union. A container the selection never authorized contributes nothing.
    ///
    /// # Errors
    ///
    /// Invalid cursors and any failed component query.
    pub async fn view_in_scope(
        &self,
        selection: &KnowledgeReadScope,
        cursor: Option<String>,
        limit: u32,
    ) -> CoreResult<ActorKnowledgePage> {
        let cursor = Self::decode_cursor(&cursor)?;
        let mut items = Vec::new();
        for owner in selection.containers() {
            items.extend(
                self.component(owner, cursor.as_ref(), limit, selection)
                    .await?,
            );
        }
        Self::paginate(items, None, limit)
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
        let selection = self
            .actor_view_scope(
                caller_creator_id,
                actor,
                &query.world_id,
                query.binding_id.as_deref(),
            )
            .await?;
        self.view_in_scope(&selection, query.cursor, query.limit)
            .await
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
        let holder_entry_id = require_actor_holder(
            &self.pool,
            caller_creator_id,
            &AdmittedActor::Character {
                character_id: character_id.to_string(),
            },
        )
        .await?;
        // The listing is an ActorView over the exact admitted Character: its
        // own container, and only its own `owner-private` rows.
        let selection = KnowledgeReadScope::actor_view(
            holder_entry_id,
            vec![KnowledgeOwnerRef::character(character_id)],
        )
        .map_err(|err| CoreError::ActorConflict {
            code: "holder_state_invalid".to_string(),
            message: err.to_string(),
        })?;
        self.view_in_scope(&selection, cursor, limit).await
    }

    async fn component(
        &self,
        owner: &KnowledgeOwnerRef,
        cursor: Option<&(String, String)>,
        limit: u32,
        selection: &KnowledgeReadScope,
    ) -> CoreResult<Vec<KnowledgeEntryRecord>> {
        self.store
            .list_by_owner_keyset(owner, cursor, limit.saturating_add(1), selection)
            .await
            .map_err(|e| component_err(&e))
    }

    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the actor/KB read is
    /// refused, or the guarded store query fails.
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

    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the actor/KB read is
    /// refused, or the guarded store query fails.
    /// Owned World with no status requirement (retained reads, durable §11.2).
    pub async fn require_owned_world(&self, creator_id: &str, world_id: &str) -> CoreResult<()> {
        require_owned_world(&self.pool, creator_id, world_id).await
    }

    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the actor/KB read is
    /// refused, or the guarded store query fails.
    /// Owned Character with no status requirement (retained reads).
    pub async fn require_owned_character(
        &self,
        creator_id: &str,
        character_id: &str,
    ) -> CoreResult<()> {
        require_owned_character(&self.pool, creator_id, character_id).await
    }

    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the actor/KB read is
    /// refused, or the guarded store query fails.
    /// Stored binding tuple with no status requirement (retained reads).
    pub async fn require_stored_binding_tuple(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> CoreResult<()> {
        require_stored_binding_tuple(&self.pool, character_id, binding_id, world_id).await
    }

    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the actor/KB read is
    /// refused, or the guarded store query fails.
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

// ── Admitted knowledge context (v1.191 P1 T5, durable §§4.1/4.3) ────────
//
// The admission carrier for knowledge reads: a non-Serde, private-field
// context minted only after the principal, the stored owner and the admitted
// Actor checks, holding the resolved server-selected read selection plus the
// shared leases that must be held through every effect it authorizes. Nothing
// here can be built from a request body — a wire payload has no field that
// reaches `KnowledgeReadPolicy`, the resolved holder or the container list.

/// Stored World/Character knowledge revision pair (durable §4.3), read under
/// the knowledge leases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeRevisions {
    /// Owning World `knowledge_revision` (always present).
    pub world: i64,
    /// Admitted Character `knowledge_revision` (`None` for Creator predicates
    /// and for the World-scoped management review).
    pub character: Option<i64>,
}

/// Exact knowledge fingerprint of one admitted context: read-policy kind plus
/// the stored revision pair.
///
/// This is a comparison value, not an authority carrier: it authorizes no row
/// on its own, it is never accepted as a selector, and it carries no Serde or
/// default. It participates in [`crate::ActorSessionKey`] so a session whose
/// policy or stored revisions moved is retired instead of reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActorKnowledgeIdentity {
    policy: KnowledgeReadPolicy,
    world_revision: i64,
    character_revision: Option<i64>,
}

impl ActorKnowledgeIdentity {
    /// Server-chosen read policy kind.
    #[must_use]
    pub const fn policy(&self) -> KnowledgeReadPolicy {
        self.policy
    }

    /// Stored World `knowledge_revision` at admission.
    #[must_use]
    pub const fn world_revision(&self) -> i64 {
        self.world_revision
    }

    /// Stored Character `knowledge_revision` at admission.
    #[must_use]
    pub const fn character_revision(&self) -> Option<i64> {
        self.character_revision
    }
}

/// Admitted knowledge context: the server-selected read selection plus the
/// fences held for as long as the effect that owns this context runs.
///
/// Private fields on purpose. The two policies are chosen by admission, never
/// by a caller or a payload: [`CoreService::admit_actor_knowledge_view`] is
/// always an ActorView (exact admitted holder, authorized containers) and
/// [`CoreService::admit_creator_management_knowledge`] is always
/// CreatorManagement (owned containers plus the known-governance holder set).
pub struct AdmittedKnowledgeContext {
    identity: ActorKnowledgeIdentity,
    scope: KnowledgeReadScope,
    activity: Option<crate::actor_fence::ActorActivityLease>,
    _leases: crate::actor_fence::KnowledgeEffectLeases,
}

impl AdmittedKnowledgeContext {
    /// Server-chosen read policy kind of this context.
    #[must_use]
    pub const fn policy(&self) -> KnowledgeReadPolicy {
        self.identity.policy
    }

    /// Exact knowledge fingerprint (policy + stored revision pair).
    #[must_use]
    pub const fn identity(&self) -> ActorKnowledgeIdentity {
        self.identity
    }

    /// Stored revision pair read under the leases at admission.
    #[must_use]
    pub const fn revisions(&self) -> KnowledgeRevisions {
        KnowledgeRevisions {
            world: self.identity.world_revision,
            character: self.identity.character_revision,
        }
    }

    /// Stored `lifecycle_epoch` witnessed by the held Character activity
    /// lease, if any (Creator predicates and management review hold none).
    #[must_use]
    pub fn activity_epoch(&self) -> Option<i64> {
        self.activity
            .as_ref()
            .map(crate::actor_fence::ActorActivityLease::epoch)
    }

    /// The resolved read selection handed to the lower-layer store (durable
    /// §4.1): typed containers, resolved holder, policy kind — with no
    /// deserialization and no client-supplied authority.
    #[must_use]
    pub fn scope(&self) -> KnowledgeReadScope {
        self.scope.clone()
    }
}

impl std::fmt::Debug for AdmittedKnowledgeContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately non-exhaustive: no lease or resolved-holder detail.
        f.debug_struct("AdmittedKnowledgeContext")
            .field("policy", &self.identity.policy)
            .field("world_revision", &self.identity.world_revision)
            .field("character_revision", &self.identity.character_revision)
            .finish_non_exhaustive()
    }
}

/// Stored World/Character knowledge revisions (durable §4.3), read **under the
/// knowledge leases** so the pair belongs to the same fence window as the
/// operation that will use it.
async fn read_knowledge_revisions(
    pool: &SqlitePool,
    world_id: &str,
    character_id: Option<&str>,
) -> CoreResult<KnowledgeRevisions> {
    let world: Option<i64> =
        sqlx::query_scalar("SELECT knowledge_revision FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(&e))?;
    let world = world.ok_or_else(|| not_found("world", world_id))?;
    let character = match character_id {
        None => None,
        Some(id) => {
            let revision: Option<i64> = sqlx::query_scalar(
                "SELECT knowledge_revision FROM characters WHERE character_id = ?",
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(&e))?;
            Some(revision.ok_or_else(|| not_found("character", id))?)
        }
    };
    Ok(KnowledgeRevisions { world, character })
}

/// Owned containers of one World for the Creator management review (§4.1):
/// the World itself plus every owned Character bound to it and those bindings.
/// Archived Characters stay in the retained management selection.
async fn management_containers(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> CoreResult<(Vec<KnowledgeOwnerRef>, Vec<String>)> {
    let rows = sqlx::query(
        "SELECT b.character_id, b.binding_id \
         FROM actor_world_bindings b \
         INNER JOIN characters c ON c.character_id = b.character_id \
         WHERE b.world_id = ? AND c.owner_creator_id = ?",
    )
    .bind(world_id)
    .bind(creator_id)
    .fetch_all(pool)
    .await
    .map_err(|err| CoreError::Internal {
        category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
    })?;
    let mut containers = vec![KnowledgeOwnerRef::world(world_id)];
    let mut character_ids: Vec<String> = Vec::new();
    for row in rows {
        let character_id: String = row.try_get("character_id").map_err(|err| CoreError::Internal {
            category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
        })?;
        let binding_id: String = row.try_get("binding_id").map_err(|err| CoreError::Internal {
            category: format!("{KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX}: {err}"),
        })?;
        if !character_ids.contains(&character_id) {
            character_ids.push(character_id.clone());
            containers.push(KnowledgeOwnerRef::character(&character_id));
        }
        containers.push(KnowledgeOwnerRef::actor_world_binding(&binding_id));
    }
    Ok((containers, character_ids))
}

/// Server-chosen `CreatorManagement` read selection for one owned World
/// (durable §4.1) plus the owned Character ids it authorizes.
///
/// The owned World/Character/binding containers come from stored ownership
/// rows and the known-governance holder set from the identity registry, so an
/// owned private row stays reviewable. Nothing here reads a request: the
/// selection is derived from the admitted Creator id alone, exactly like
/// [`CoreService::admit_creator_management_knowledge`], and the authoring /
/// maintenance paths that already hold their own fence reuse it without
/// re-taking the shared effect leases.
pub(crate) async fn management_read_scope(
    pool: &SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> CoreResult<(KnowledgeReadScope, Vec<String>)> {
    require_owned_world(pool, creator_id, world_id).await?;
    let (containers, character_ids) = management_containers(pool, creator_id, world_id).await?;
    let mut authorized_holders = vec![
        nexus_local_db::require_creator_holder(pool, creator_id)
            .await
            .map_err(actor_db_err)?,
    ];
    for character_id in &character_ids {
        authorized_holders.push(
            nexus_local_db::require_character_holder(pool, creator_id, character_id)
                .await
                .map_err(actor_db_err)?,
        );
    }
    Ok((
        KnowledgeReadScope::creator_management(containers, authorized_holders),
        character_ids,
    ))
}

impl CoreService {
    /// The `CreatorManagement` read selection for one owned World (durable
    /// §4.1), for the shipped authoring/maintenance surfaces that already hold
    /// their own fence.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] for a failed principal, [`CoreError::NotFound`]
    /// for a foreign/missing World, and the fail-closed `holder_state_invalid`.
    pub async fn creator_management_read_scope(
        &self,
        principal: &Principal,
        world_id: &str,
    ) -> CoreResult<KnowledgeReadScope> {
        self.verify_principal(principal)?;
        let (scope, _) =
            management_read_scope(&self.inner.pool, principal.creator_id(), world_id).await?;
        Ok(scope)
    }

    /// The `ActorView` read selection for one stored Actor plus viewpoint
    /// (durable §4.1): the exact admitted holder and the authorized
    /// containers. Never a management selection, whatever the caller sends.
    ///
    /// # Errors
    /// As [`ActorKnowledgeViewService::actor_view_scope`]: auth/ownership
    /// failures, the fail-closed `holder_state_invalid`, and the missing
    /// `binding_id` for a Character actor.
    pub async fn actor_view_read_scope(
        &self,
        principal: &Principal,
        actor: &AdmittedActor,
        world_id: &str,
        binding_id: Option<&str>,
    ) -> CoreResult<KnowledgeReadScope> {
        self.verify_principal(principal)?;
        ActorKnowledgeViewService::new(self.inner.pool.clone())
            .actor_view_scope(principal.creator_id(), actor, world_id, binding_id)
            .await
    }

    /// The `ActorView` read selection for the admitted Creator over one owned
    /// World — the Creator-context preview (model/run/inspect) that never
    /// reviews owned private facts on another identity's behalf.
    ///
    /// # Errors
    /// As [`Self::actor_view_read_scope`].
    pub async fn creator_view_read_scope(
        &self,
        principal: &Principal,
        world_id: &str,
    ) -> CoreResult<KnowledgeReadScope> {
        self.actor_view_read_scope(
            principal,
            &AdmittedActor::Creator {
                creator_id: principal.creator_id().to_string(),
            },
            world_id,
            None,
        )
        .await
    }

    /// Admit an operation-scoped **ActorView** knowledge context (durable
    /// §4.1): the exact admitted Creator/Character holder plus the authorized
    /// containers, minted only after the principal, stored-owner and admitted
    /// Actor checks.
    ///
    /// The context holds the Character activity lease (Character predicates)
    /// and the shared World-then-Character knowledge leases (durable §4.3) for
    /// as long as the caller keeps it; dropping it releases every fence.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] for a failed principal, the stored-owner
    /// taxonomy (`not_found`, `character_inactive`, `world_inactive`), the
    /// fail-closed `holder_state_invalid`, and `character_busy`/`world_busy`
    /// when a governance edit holds the admitted subject.
    pub async fn admit_actor_knowledge_view(
        &self,
        principal: &Principal,
        actor: &AdmittedActor,
        viewpoint: ActorViewpoint,
    ) -> CoreResult<AdmittedKnowledgeContext> {
        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        require_admitted_ownership(&self.inner.pool, creator_id, actor, &viewpoint).await?;
        let holder_entry_id = require_actor_holder(&self.inner.pool, creator_id, actor).await?;
        let mut containers = vec![KnowledgeOwnerRef::world(viewpoint.world_id.as_str())];
        let mut character_ids = Vec::new();
        let activity = match actor {
            AdmittedActor::Creator { .. } => None,
            AdmittedActor::Character { character_id } => {
                let binding_id = viewpoint.binding_id.clone().ok_or_else(|| {
                    CoreError::ActorInput(
                        "binding_id is required for Character actor_ref".to_string(),
                    )
                })?;
                containers.push(KnowledgeOwnerRef::character(character_id.as_str()));
                containers.push(KnowledgeOwnerRef::actor_world_binding(binding_id));
                character_ids.push(character_id.clone());
                Some(self.acquire_actor_activity(principal, actor).await?)
            }
        };
        let _leases = self
            .inner
            .character_fences
            .try_acquire_knowledge_effect(std::slice::from_ref(&viewpoint.world_id), &character_ids)?;
        let revisions = read_knowledge_revisions(
            &self.inner.pool,
            &viewpoint.world_id,
            actor.character_id(),
        )
        .await?;
        let scope = KnowledgeReadScope::actor_view(holder_entry_id, containers)
            .map_err(|err| CoreError::ActorConflict {
                code: "holder_state_invalid".to_string(),
                message: err.to_string(),
            })?;
        Ok(AdmittedKnowledgeContext {
            identity: ActorKnowledgeIdentity {
                policy: KnowledgeReadPolicy::ActorView,
                world_revision: revisions.world,
                character_revision: revisions.character,
            },
            scope,
            activity,
            _leases,
        })
    }

    /// Admit the **CreatorManagement** knowledge context for one owned World
    /// (durable §4.1): the owned World/Character/binding containers plus the
    /// known-governance holder set, so owned private rows are reviewable.
    ///
    /// Retained reads: an archived World or Character stays reviewable. Never
    /// encoded as an absent viewpoint, and never reachable through the
    /// ActorView path.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`], [`CoreError::NotFound`] for foreign/missing
    /// Worlds, the fail-closed `holder_state_invalid`, and `world_busy` /
    /// `character_busy` while a governance edit holds a selected subject.
    pub async fn admit_creator_management_knowledge(
        &self,
        principal: &Principal,
        world_id: String,
    ) -> CoreResult<AdmittedKnowledgeContext> {
        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        let (scope, character_ids) =
            management_read_scope(&self.inner.pool, creator_id, &world_id).await?;
        let _leases = self
            .inner
            .character_fences
            .try_acquire_knowledge_effect(std::slice::from_ref(&world_id), &character_ids)?;
        let revisions = read_knowledge_revisions(&self.inner.pool, &world_id, None).await?;
        Ok(AdmittedKnowledgeContext {
            identity: ActorKnowledgeIdentity {
                policy: KnowledgeReadPolicy::CreatorManagement,
                world_revision: revisions.world,
                character_revision: None,
            },
            scope,
            activity: None,
            _leases,
        })
    }

    /// Take the exclusive governance lease for one owned World or Character
    /// subject (durable §4.3). A governance edit holds it around the knowledge
    /// transaction: World-governance edits take the World, Character-global /
    /// binding-governance edits take the Character. Any in-flight knowledge
    /// effect on that subject is the busy refusal, so a disclosure change can
    /// never land underneath a running read or stream.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`], [`CoreError::NotFound`] for foreign/missing
    /// subjects, and `world_busy` / `character_busy` when an effect holds it.
    pub async fn acquire_knowledge_governance(
        &self,
        principal: &Principal,
        kind: crate::actor_fence::ActorFenceKind,
        subject_id: String,
    ) -> CoreResult<crate::actor_fence::KnowledgeGovernanceLease> {
        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        match kind {
            crate::actor_fence::ActorFenceKind::World => {
                require_owned_world(&self.inner.pool, creator_id, &subject_id).await?;
            }
            crate::actor_fence::ActorFenceKind::Character => {
                require_owned_character(&self.inner.pool, creator_id, &subject_id).await?;
            }
        }
        self.inner
            .character_fences
            .try_acquire_knowledge_governance(kind, &subject_id)
    }
}

/// Owned World with no status requirement (retained reads, durable §11.2):
/// a World still owned but paused/archived keeps its retained history
/// readable. Missing/foreign Worlds fail closed.
pub async fn require_owned_world(
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
            let owner: String = stored.try_get("owner_creator_id").map_err(|e| db_err(&e))?;
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
pub async fn require_owned_character(
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
pub async fn require_stored_binding_tuple(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: &str,
    world_id: &str,
) -> CoreResult<()> {
    let row =
        sqlx::query("SELECT character_id, world_id FROM actor_world_bindings WHERE binding_id = ?")
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

    /// Compose one policy-filtered keyset page for an already admitted
    /// knowledge context (durable §4.2).
    ///
    /// This is the read that consumes the **`CreatorManagement`** selection:
    /// the authorized Creator reviews owned `owner-private` facts through their
    /// own admitted context, while every `ActorView` (including a Creator's
    /// `ActorView` and every Connect path) keeps the holder-filtered selection.
    /// The page is selected before pagination, so a hidden row consumes no
    /// slot, no `has_more` and no cursor position.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, and the component-query errors otherwise.
    pub async fn admitted_knowledge_page(
        &self,
        principal: &Principal,
        context: &AdmittedKnowledgeContext,
        limit: u32,
        cursor: Option<String>,
    ) -> CoreResult<ActorKnowledgePage> {
        self.verify_principal(principal)?;
        let views = ActorKnowledgeViewService::new(self.inner.pool.clone());
        views.view_in_scope(&context.scope(), cursor, limit).await
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
        match actor {
            // Durable §5.1: the Creator World-selected KnowledgeView is the
            // **management review** — owned containers plus the
            // known-governance holder set, so the authorized Creator can
            // review owned private facts. Server-selected from the
            // authenticated entry point's actor kind, never a request flag.
            AdmittedActor::Creator { .. } => {
                let (selection, _) = management_read_scope(
                    &self.inner.pool,
                    principal.creator_id(),
                    &query.world_id,
                )
                .await?;
                views
                    .view_in_scope(&selection, query.cursor, query.limit)
                    .await
            }
            // A Character actor_ref stays the strict holder-filtered preview:
            // management review is never inherited by a Character view.
            AdmittedActor::Character { .. } => {
                views.view(principal.creator_id(), actor, query).await
            }
        }
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
    #[allow(clippy::too_many_lines)] // one linear domain operation
    pub async fn add_actor_knowledge_entry(
        &self,
        principal: &Principal,
        request: nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequest,
        summary_present: bool,
    ) -> CoreResult<KnowledgeEntryRecord> {
        use nexus_contracts::generated::daemon_api::actor_knowledge::add_knowledge_entry_request::AddKnowledgeEntryRequestOwnerKind;

        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        // The owner container plus its owning Character (durable §3: a
        // Character/binding container authorizes only that Character).
        let (owner, owner_character_id) = match request.owner_kind {
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
                (KnowledgeOwnerRef::world(world_id.to_string()), None)
            }
            AddKnowledgeEntryRequestOwnerKind::Character => {
                let character_id = str_field(request.character_id.as_ref()).ok_or_else(|| {
                    CoreError::ActorInput(
                        "character_id is required for character-owned knowledge".to_string(),
                    )
                })?;
                require_active_owned_character(&self.inner.pool, creator_id, character_id).await?;
                (
                    KnowledgeOwnerRef::character(character_id.to_string()),
                    Some(character_id.to_string()),
                )
            }
            AddKnowledgeEntryRequestOwnerKind::ActorWorldBinding => {
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
                (
                    KnowledgeOwnerRef::actor_world_binding(binding_id.to_string()),
                    Some(character_id.to_string()),
                )
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
                KnowledgeEntryRecord::for_binding(id, block_type, request.canonical_name.as_str())
            }
        };
        // v1.191 P1 (durable §3): the closed `audience` replaces the retired
        // World-only `creator_only` carrier. The author supplies intent only;
        // the permitted identity and its holder are resolved from **stored
        // state inside the authoring transaction** (durable §3's order), so a
        // committed private row can never name an identity that stopped being
        // permitted before the commit.
        let audience = authored_audience(request.audience.as_ref())?;
        let intent = create_audience_intent(audience.as_ref(), creator_id);
        // Leases before the transaction (durable §4.3): a governance create
        // takes the exclusive container fence — World first, then the named
        // Character — while an ordinary shared create keeps the retained
        // shared activity fence.
        let mut governance_leases: Vec<crate::actor_fence::KnowledgeGovernanceLease> = Vec::new();
        if let Some(KnowledgeAudience::AuthorOnly | KnowledgeAudience::CharacterPrivate { .. }) =
            &audience
        {
            match &owner {
                KnowledgeOwnerRef::World(world_id) => {
                    governance_leases.push(
                        self.acquire_knowledge_governance(
                            principal,
                            crate::actor_fence::ActorFenceKind::World,
                            world_id.clone(),
                        )
                        .await?,
                    );
                    if let Some(KnowledgeAudience::CharacterPrivate { character_id }) = &audience {
                        governance_leases.push(
                            self.acquire_knowledge_governance(
                                principal,
                                crate::actor_fence::ActorFenceKind::Character,
                                character_id.clone(),
                            )
                            .await?,
                        );
                    }
                }
                KnowledgeOwnerRef::Character(_) | KnowledgeOwnerRef::ActorWorldBinding(_) => {
                    governance_leases.push(
                        self.acquire_knowledge_governance(
                            principal,
                            crate::actor_fence::ActorFenceKind::Character,
                            owner_character_id.clone().ok_or_else(|| {
                                CoreError::Internal {
                                    category: format!(
                                        "{KNOWLEDGE_INSERT_FAILED_PREFIX}: a Character/binding \
                                         create must carry its admitted Character"
                                    ),
                                }
                            })?,
                        )
                        .await?,
                    );
                }
            }
        }
        let governance_authored = !governance_leases.is_empty();
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
            KnowledgeOwnerRef::World(_) => nexus_local_db::kb_store::author_world_knowledge_entry(
                &self.inner.pool,
                creator_id,
                record.clone(),
                intent,
            )
            .await
            .map_err(actor_insert_db_err)?
            .entry_id,
            KnowledgeOwnerRef::Character(id) => {
                let _activity = if governance_authored {
                    None
                } else {
                    Some(
                        self.acquire_actor_activity(
                            principal,
                            &AdmittedActor::Character {
                                character_id: id.clone(),
                            },
                        )
                        .await?,
                    )
                };
                store
                    .author_actor_owned_key_block(creator_id, id.as_str(), None, record.clone(), intent)
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
                let _activity = if governance_authored {
                    None
                } else {
                    Some(
                        self.acquire_actor_activity(
                            principal,
                            &AdmittedActor::Character {
                                character_id: admitted_character.to_string(),
                            },
                        )
                        .await?,
                    )
                };
                store
                    .author_actor_owned_key_block(
                        creator_id,
                        admitted_character,
                        Some(id.as_str()),
                        record.clone(),
                        intent,
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
    /// lease (CAS-checked), optionally moving the native governance pair.
    ///
    /// `audience` is the admitted author audience (durable §3): `None` omits
    /// the member and preserves the stored `holder_entry_id`/`disclosure`
    /// pair, `Some(Shared)` explicitly clears both, and a private audience
    /// resolves the admitted holder — `author-only` the controlling Creator's
    /// holder, `character-private` only the owning Character of this
    /// Character/binding row.
    ///
    /// Leases (durable §4.3): a governance edit takes the **Character
    /// exclusive** knowledge lease (which already excludes every in-flight
    /// ActorView/stream on that Character); an ordinary content edit keeps the
    /// retained shared activity lease.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the mapped storage conflicts otherwise
    /// (`knowledge_revision_conflict`, `knowledge_entry_not_mutable`,
    /// `holder_state_invalid`, `character_busy`).
    pub async fn patch_actor_knowledge_entry(
        &self,
        principal: &Principal,
        character_id: String,
        entry_id: String,
        expected_revision: i64,
        canonical_name_patch: Option<&str>,
        summary_patch: nexus_local_db::FieldPatch<&str>,
        audience: Option<KnowledgeAudience>,
    ) -> CoreResult<KnowledgeEntryRecord> {
        self.verify_principal(principal)?;
        let creator_id = principal.creator_id();
        let intent = audience_intent(audience.as_ref(), creator_id);
        // Durable §4.3: a disclosure change is a Character-governance edit and
        // takes the exclusive Character lease; an activity lease would be the
        // weaker (shared) fence and would self-block the exclusive one.
        let governance_authored = audience.is_some();
        let _governance_lease = if governance_authored {
            Some(
                self.acquire_knowledge_governance(
                    principal,
                    crate::actor_fence::ActorFenceKind::Character,
                    character_id.clone(),
                )
                .await?,
            )
        } else {
            None
        };
        let _activity = if governance_authored {
            None
        } else {
            Some(
                self.acquire_actor_activity(
                    principal,
                    &AdmittedActor::Character {
                        character_id: character_id.clone(),
                    },
                )
                .await?,
            )
        };

        nexus_local_db::author_actor_knowledge_entry(
            &self.inner.pool,
            creator_id,
            &character_id,
            &entry_id,
            expected_revision,
            nexus_local_db::ActorKnowledgePatch {
                canonical_name: canonical_name_patch,
                summary: summary_patch,
            },
            intent,
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
    // One admitted selection for the whole composition: every merged page is
    // selected by the same policy-filtered eligibility, so the cap counts
    // eligible rows only (durable §4.2).
    let selection = views
        .actor_view_scope(caller_creator_id, actor, world_id, binding_id.as_deref())
        .await?;
    let mut items = Vec::new();
    let mut cursor = None;
    loop {
        if items.len() >= HARD_CAP {
            return Err(view_incomplete());
        }
        let page = views.view_in_scope(&selection, cursor, limit).await?;
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
        KbStoreError::Duplicate { .. }
        | KbStoreError::Validation(_)
        | KbStoreError::ValidationLegacy(_) => CoreError::ActorInput(err.to_string()),
        other => CoreError::Internal {
            category: format!("{KNOWLEDGE_INSERT_FAILED_PREFIX}: {other}"),
        },
    }
}

fn str_field<T: std::ops::Deref<Target = String>>(value: Option<&T>) -> Option<&str> {
    value.map(|s| s.as_str())
}

/// Map the closed wire author audience onto the domain audience (durable §3).
///
/// `audience` is the only governance input on the create command: the author
/// never supplies `holder_entry_id` or a management flag, and omission means
/// in-scope shared. Which holder a private audience names is resolved by
/// admission against the stored identity, not here.
fn authored_audience(
    wire: Option<&AddKnowledgeEntryRequestAudience>,
) -> CoreResult<Option<KnowledgeAudience>> {
    match wire {
        None => Ok(None),
        Some(AddKnowledgeEntryRequestAudience::Shared) => Ok(Some(KnowledgeAudience::Shared)),
        Some(AddKnowledgeEntryRequestAudience::AuthorOnly) => {
            Ok(Some(KnowledgeAudience::AuthorOnly))
        }
        Some(AddKnowledgeEntryRequestAudience::CharacterPrivate(character_id)) => {
            KnowledgeAudience::character_private(character_id.to_string())
                .map(Some)
                .map_err(kb_authoring_err)
        }
    }
}

/// Map an admitted domain audience onto the local-db authoring intent
/// (durable §3). An omitted audience on a **patch** authors no governance.
fn audience_intent<'a>(
    audience: Option<&'a KnowledgeAudience>,
    creator_id: &'a str,
) -> nexus_local_db::kb_store::AuthoredAudience<'a> {
    use nexus_local_db::kb_store::AuthoredAudience;
    match audience {
        None => AuthoredAudience::Keep,
        Some(KnowledgeAudience::Shared) => AuthoredAudience::Shared,
        Some(KnowledgeAudience::AuthorOnly) => AuthoredAudience::Creator { creator_id },
        Some(KnowledgeAudience::CharacterPrivate { character_id }) => AuthoredAudience::Character {
            character_id,
        },
    }
}

/// Create-path sibling of [`audience_intent`]: an **omitted** create audience
/// is in-scope shared (durable §3).
fn create_audience_intent<'a>(
    audience: Option<&'a KnowledgeAudience>,
    creator_id: &'a str,
) -> nexus_local_db::kb_store::AuthoredAudience<'a> {
    match audience {
        None => nexus_local_db::kb_store::AuthoredAudience::Shared,
        Some(audience) => audience_intent(Some(audience), creator_id),
    }
}

/// Map the closed wire **patch** audience onto the domain audience (durable
/// §3).
///
/// The patch sibling of [`authored_audience`]: `None` omits the member (the
/// stored governance pair is preserved), `Some(Shared)` clears both columns,
/// and a private audience is resolved by admission against the permitted
/// stored identity — a caller never supplies a holder id or a management flag.
///
/// # Errors
///
/// Returns [`CoreError::ActorInput`] for a malformed `character-private`
/// Character id (the domain constructor owns that rule).
pub fn authored_patch_audience(
    wire: Option<&UpdateKnowledgeEntryRequestAudience>,
) -> CoreResult<Option<KnowledgeAudience>> {
    match wire {
        None => Ok(None),
        Some(UpdateKnowledgeEntryRequestAudience::Shared) => Ok(Some(KnowledgeAudience::Shared)),
        Some(UpdateKnowledgeEntryRequestAudience::AuthorOnly) => {
            Ok(Some(KnowledgeAudience::AuthorOnly))
        }
        Some(UpdateKnowledgeEntryRequestAudience::CharacterPrivate(character_id)) => {
            KnowledgeAudience::character_private(character_id.to_string())
                .map(Some)
                .map_err(kb_authoring_err)
        }
    }
}

/// Map a domain authoring refusal onto the actor input family, matching the
/// retained create/update `invalid_input` texture for validation refusals.
fn kb_authoring_err(err: KbError) -> CoreError {
    match err {
        KbError::ValidationError(message) => CoreError::ActorInput(message),
        other => CoreError::Internal {
            category: format!("{KNOWLEDGE_INSERT_FAILED_PREFIX}: {other}"),
        },
    }
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
            let err = ActorKnowledgeViewService::decode_cursor(&Some(bad.into())).expect_err(bad);
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
