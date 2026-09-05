//! Reusable fail-closed Actor KnowledgeView service (v1.184 P1 Task 3).
//!
//! HTTP, CLI, and later P2/P4 callers compose through this type. Callers never
//! re-union owner sets themselves. Invalid ownership or any failed component
//! query returns an error and no partial page.

use crate::api::errors::NexusApiError;
use nexus_knowledge::world_kb::knowledge_entry::{stored_created_at_order_millis, KnowledgeEntryRecord, KnowledgeOwnerRef};
use nexus_knowledge::world_kb::store::KbStoreError;
use nexus_local_db::kb_store::SqliteKbStore;
use sqlx::SqlitePool;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;
const CURSOR_PREFIX: &str = "k2:";
const CURSOR_SEP: char = '\u{1f}';

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

/// Discriminant admitted from stored owners (never from payload claims).
#[derive(Debug, Clone)]
pub enum AdmittedActor {
    Creator { creator_id: String },
    Character { character_id: String },
}

/// One reusable KnowledgeView composer.
pub struct ActorKnowledgeViewService {
    store: SqliteKbStore,
    pool: SqlitePool,
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
    /// Returns `invalid_input` when `limit` is out of range.
    pub fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
        match raw {
            None => Ok(DEFAULT_LIMIT),
            Some(n) if n > 0 && n <= i64::from(MAX_LIMIT) => u32::try_from(n).map_err(|_| {
                NexusApiError::BadRequest {
                    code: "invalid_input".into(),
                    message: "limit is out of range".into(),
                }
            }),
            Some(_) => Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: format!("limit must be between 1 and {MAX_LIMIT}"),
            }),
        }
    }

    /// Decode the opaque two-field cursor `(created_at, key_block_id)`.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when the token is present but not a `k2:` pair.
    pub fn decode_cursor(cursor: &Option<String>) -> Result<Option<(String, String)>, NexusApiError> {
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
    /// Returns `ACTOR_KNOWLEDGE_WIRE_INVALID` when a stored timestamp is neither
    /// RFC3339 nor SQLite `datetime('now')`.
    pub fn paginate(
        items: Vec<KnowledgeEntryRecord>,
        cursor: Option<(String, String)>,
        limit: u32,
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        let mut keyed = Vec::with_capacity(items.len());
        for row in items {
            let millis = stored_created_at_order_millis(&row.created_at).map_err(timestamp_err)?;
            keyed.push((millis, row.entry_id.clone(), row));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if let Some((created_at, entry_id)) = cursor {
            let cursor_ms = stored_created_at_order_millis(&created_at).map_err(|_| invalid_cursor())?;
            keyed.retain(|(ms, id, _)| (*ms, id.as_str()) > (cursor_ms, entry_id.as_str()));
        }
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = keyed.len() > limit_us;
        keyed.truncate(limit_us);
        let items: Vec<KnowledgeEntryRecord> = keyed.into_iter().map(|(_, _, row)| row).collect();
        let next_cursor = if has_more {
            items.last().map(|row| Self::encode_cursor(&row.created_at, &row.entry_id))
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
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        let cursor = Self::decode_cursor(&query.cursor)?;
        match actor {
            AdmittedActor::Creator { creator_id } => {
                if creator_id != caller_creator_id {
                    return Err(not_found("actor_ref", creator_id));
                }
                self.require_owned_world(caller_creator_id, &query.world_id)
                    .await?;
                let parts = self
                    .creator_union(caller_creator_id, &query.world_id, cursor.as_ref(), query.limit)
                    .await?;
                Self::paginate(parts, None, query.limit)
            }
            AdmittedActor::Character { character_id } => {
                let Some(binding_id) = query.binding_id.as_deref() else {
                    return Err(NexusApiError::BadRequest {
                        code: "invalid_input".into(),
                        message: "binding_id is required for Character KnowledgeView".into(),
                    });
                };
                self.require_owned_character(caller_creator_id, character_id)
                    .await?;
                self.require_owned_world(caller_creator_id, &query.world_id)
                    .await?;
                self.require_active_binding(character_id, binding_id, &query.world_id)
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
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        self.require_owned_character(caller_creator_id, character_id)
            .await?;
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
    ) -> Result<Vec<KnowledgeEntryRecord>, NexusApiError> {
        let mut items = self
            .component(KnowledgeOwnerRef::world(world_id), cursor, limit, false)
            .await?;
        let bindings = sqlx::query!(
            r#"SELECT b.character_id AS "character_id!", b.binding_id AS "binding_id!"
              FROM actor_world_bindings b
              INNER JOIN characters c ON c.character_id = b.character_id
              WHERE b.world_id = ?
                AND b.status = 'active'
                AND c.owner_creator_id = ?
                AND c.status = 'active'"#,
            world_id,
            creator_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|err| NexusApiError::Internal {
            code: "ACTOR_KNOWLEDGE_VIEW_COMPONENT_FAILED".into(),
            message: err.to_string(),
        })?;

        let mut seen_characters = std::collections::BTreeSet::new();
        for row in bindings {
            let (character_id, binding_id) = (row.character_id, row.binding_id);
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
    ) -> Result<Vec<KnowledgeEntryRecord>, NexusApiError> {
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
    ) -> Result<Vec<KnowledgeEntryRecord>, NexusApiError> {
        self.store
            .list_by_owner_keyset(
                &owner,
                cursor,
                limit.saturating_add(1),
                exclude_creator_only,
            )
            .await
            .map_err(component_err)
    }

    /// Owned **and active** World admission (PR #240 finding 1): public
    /// actor-knowledge operations reject foreign/missing Worlds (404) and
    /// owned-but-inactive Worlds (409 `world_inactive`), matching Host
    /// admission lifecycle parity.
    pub(crate) async fn require_owned_world(
        &self,
        creator_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        let row = sqlx::query!(
            r#"SELECT owner_creator_id AS "owner_creator_id!", status AS "status!"
               FROM narrative_worlds WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        match row {
            Some(stored) if stored.owner_creator_id == creator_id && stored.status == "active" => {
                Ok(())
            }
            Some(stored) if stored.owner_creator_id == creator_id => Err(
                NexusApiError::ConflictCoded {
                    code: "world_inactive".into(),
                    message: format!("world {world_id} is {}", stored.status),
                },
            ),
            _ => Err(not_found("world", world_id)),
        }
    }

    /// Owned **and active** Character admission (PR #240 finding 1): foreign
    /// or missing Characters are 404; owned-but-archived Characters are 409
    /// `character_inactive`.
    pub(crate) async fn require_owned_character(
        &self,
        creator_id: &str,
        character_id: &str,
    ) -> Result<(), NexusApiError> {
        match nexus_local_db::get_character(&self.pool, creator_id, character_id).await? {
            Some(stored) if stored.status == "active" => Ok(()),
            Some(stored) => Err(NexusApiError::ConflictCoded {
                code: "character_inactive".into(),
                message: format!("character {character_id} is {}", stored.status),
            }),
            None => Err(not_found("character", character_id)),
        }
    }

    pub(crate) async fn require_active_binding(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        let row = sqlx::query!(
            r#"SELECT character_id AS "character_id!", world_id AS "world_id!",
                      status AS "status!"
               FROM actor_world_bindings WHERE binding_id = ?"#,
            binding_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        match row {
            Some(stored)
                if stored.character_id == character_id
                    && stored.world_id == world_id
                    && stored.status == "active" =>
            {
                Ok(())
            }
            _ => Err(not_found("actor_world_binding", binding_id)),
        }
    }
}

fn invalid_cursor() -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: "cursor is not a valid opaque two-field keyset token".into(),
    }
}

fn timestamp_err(err: String) -> NexusApiError {
    NexusApiError::Internal {
        code: "ACTOR_KNOWLEDGE_WIRE_INVALID".into(),
        message: err,
    }
}

fn not_found(resource: &str, id: &str) -> NexusApiError {
    NexusApiError::NotFound(format!("{resource} {id}"))
}

fn component_err(err: KbStoreError) -> NexusApiError {
    NexusApiError::Internal {
        code: "ACTOR_KNOWLEDGE_VIEW_COMPONENT_FAILED".into(),
        message: err.to_string(),
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
        let decoded = ActorKnowledgeViewService::decode_cursor(&Some(cursor.clone()))
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
                NexusApiError::BadRequest { code, .. } => assert_eq!(code, "invalid_input"),
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
            page1.items.iter().map(|r| r.entry_id.as_str()).collect::<Vec<_>>(),
            vec!["kb_a"]
        );
        assert!(page1.has_more);
        let cursor = ActorKnowledgeViewService::decode_cursor(&page1.next_cursor)
            .unwrap()
            .expect("cursor");
        let page2 = ActorKnowledgeViewService::paginate(rows, Some(cursor), 1).expect("p2");
        assert_eq!(
            page2.items.iter().map(|r| r.entry_id.as_str()).collect::<Vec<_>>(),
            vec!["kb_m"]
        );
        assert!(!page2.has_more);
    }
}
