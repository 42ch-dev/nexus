//! Character ToM L1/L2 record and bounded query (v1.184 P4 Task 2).
//!
//! Composes P1 [`ActorKnowledgeViewService`] admission, P2 stored-owner checks,
//! and the Task 1 atomic carrier CAS + derivative MindState seam. Record and
//! query make zero provider calls.

use crate::actor_knowledge_view::ActorKnowledgeViewService;
use crate::api::errors::NexusApiError;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_knowledge::world_kb::knowledge_entry::{
    validate_character_tom_belief_row, BeliefPropositionRaw, KnowledgeEntryRecord, KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::LocalDbError;
use nexus_spoke_adapter::adapter::mind_state::atomic_cas_carrier_modules_and_insert_mind_state_in_tx;
use serde_json::{json, Value};
use sqlx::SqlitePool;

const CURSOR_PREFIX: &str = "tom3:";
const CURSOR_SEP: char = '\u{1f}';
/// Fixed query-work bounds (fix round 1, review I4): the corpus is admitted
/// only up to these caps; exceeding them fails closed before materialization.
const MAX_CARRIERS_PER_SCOPE: u32 = 200;
const MAX_BELIEF_ROWS_PER_CARRIER: usize = 200;

/// One belief row in keyset order `(order, carrier_entry_id, row_ordinal)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterTomBeliefRow {
    pub carrier_entry_id: String,
    pub row_ordinal: u32,
    pub belief: BeliefPropositionRaw,
    pub carrier_recorded_at: Option<String>,
}

/// Bounded keyset page.
#[derive(Debug, Clone)]
pub struct CharacterTomPage {
    pub items: Vec<CharacterTomBeliefRow>,
    pub limit: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Admitted list query.
#[derive(Debug, Clone)]
pub struct CharacterTomListQuery {
    pub world_id: String,
    pub binding_id: String,
    pub limit: u32,
    pub cursor: Option<String>,
}

/// Admitted record mutation (after wire DTO mapping).
#[derive(Debug, Clone)]
pub struct CharacterTomRecordInput {
    pub world_id: String,
    pub binding_id: String,
    pub carrier_entry_id: String,
    pub expected_revision: i64,
    pub belief: BeliefPropositionRaw,
    pub occurred_at: Option<String>,
    pub sort_key: Option<String>,
    pub event_id: Option<String>,
}

/// Reusable Character ToM composer (record + query).
pub struct CharacterTomService {
    views: ActorKnowledgeViewService,
    store: SqliteKbStore,
    pool: SqlitePool,
}

impl CharacterTomService {
    /// Bind the service to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            views: ActorKnowledgeViewService::new(pool.clone()),
            store: SqliteKbStore::new(pool.clone()),
            pool,
        }
    }

    /// Resolve limit (1..=100, default 50).
    pub fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
        ActorKnowledgeViewService::resolve_limit(raw)
    }

    /// Decode opaque `(order, carrier_entry_id, row_ordinal)` cursor.
    pub fn decode_cursor(cursor: &Option<String>) -> Result<Option<(i64, String, u32)>, NexusApiError> {
        match cursor {
            None => Ok(None),
            Some(raw) => {
                let rest = raw.strip_prefix(CURSOR_PREFIX).ok_or_else(invalid_cursor)?;
                let parts: Vec<&str> = rest.split(CURSOR_SEP).collect();
                if parts.len() != 3 || parts[0].is_empty() || parts[1].is_empty() || parts[2].is_empty() {
                    return Err(invalid_cursor());
                }
                let order = parts[0]
                    .parse::<i64>()
                    .map_err(|_| invalid_cursor())?;
                let ordinal = parts[2]
                    .parse::<u32>()
                    .map_err(|_| invalid_cursor())?;
                Ok(Some((order, parts[1].to_string(), ordinal)))
            }
        }
    }

    /// Encode keyset cursor.
    #[must_use]
    pub fn encode_cursor(order: i64, carrier_entry_id: &str, row_ordinal: u32) -> String {
        format!("{CURSOR_PREFIX}{order}{CURSOR_SEP}{carrier_entry_id}{CURSOR_SEP}{row_ordinal}")
    }

    /// Keyset page over pre-sorted rows.
    pub fn paginate(
        mut rows: Vec<(i64, String, u32, CharacterTomBeliefRow)>,
        cursor: Option<(i64, String, u32)>,
        limit: u32,
    ) -> CharacterTomPage {
        rows.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        if let Some((order, carrier, ordinal)) = cursor {
            rows.retain(|(o, c, ord, _)| (*o, c.as_str(), *ord) > (order, carrier.as_str(), ordinal));
        }
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = rows.len() > limit_us;
        rows.truncate(limit_us);
        let items: Vec<CharacterTomBeliefRow> = rows.into_iter().map(|(_, _, _, row)| row).collect();
        let next_cursor = if has_more {
            items.last().map(|row| {
                Self::encode_cursor(
                    row.belief.order.unwrap_or(0),
                    &row.carrier_entry_id,
                    row.row_ordinal,
                )
            })
        } else {
            None
        };
        CharacterTomPage {
            items,
            limit,
            has_more,
            next_cursor,
        }
    }

    /// List Character ToM rows from authorized carriers only.
    ///
    /// Work is bounded before materialization: each owner scope admits at
    /// most `MAX_CARRIERS_PER_SCOPE` carriers and each carrier at most
    /// `MAX_BELIEF_ROWS_PER_CARRIER` belief rows; exceeding either cap fails
    /// closed. `row_ordinal` is the physical `modules.belief` array index —
    /// malformed legacy elements are skipped without renumbering so keyset
    /// cursors never skip/duplicate around them.
    pub async fn list(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        query: CharacterTomListQuery,
    ) -> Result<CharacterTomPage, NexusApiError> {
        self.admit_viewer(caller_creator_id, viewer_character_id, &query.world_id, &query.binding_id)
            .await?;
        let cursor = Self::decode_cursor(&query.cursor)?;
        // DB-side pre-parse bounds (fix round 2): carrier counts, belief-array
        // lengths, and modules JSON validity are enforced in SQLite before any
        // `modules_json` text is parsed or materialized into records.
        self.probe_scope_belief_bounds(&KnowledgeOwnerRef::character(viewer_character_id))
            .await?;
        self.probe_scope_belief_bounds(&KnowledgeOwnerRef::actor_world_binding(&query.binding_id))
            .await?;
        let carriers = self.authorized_carriers(viewer_character_id, &query.binding_id).await?;
        let recorded = self
            .carrier_recorded_at_map(viewer_character_id, &query.binding_id)
            .await?;
        let mut keyed = Vec::new();
        for carrier in carriers {
            let recorded_at = recorded.get(&carrier.entry_id).cloned().flatten();
            let rows = carrier_belief_elements(carrier.modules.as_ref())?;
            for (ordinal, element) in rows.iter().enumerate() {
                let Ok(belief) = serde_json::from_value::<BeliefPropositionRaw>(element.clone())
                else {
                    continue; // malformed legacy element: skip, keep physical ordinal
                };
                if validate_character_tom_belief_row(&belief, viewer_character_id).is_err() {
                    continue;
                }
                let order = belief.order.unwrap_or(0);
                let ordinal = u32::try_from(ordinal).map_err(|_| corpus_exceeded("belief rows per carrier"))?;
                keyed.push((
                    order,
                    carrier.entry_id.clone(),
                    ordinal,
                    CharacterTomBeliefRow {
                        carrier_entry_id: carrier.entry_id.clone(),
                        row_ordinal: ordinal,
                        belief,
                        carrier_recorded_at: recorded_at.clone(),
                    },
                ));
            }
        }
        Ok(Self::paginate(keyed, cursor, query.limit))
    }

    /// Record one L1/L2 belief on an authorized carrier (atomic CAS + MindState).
    pub async fn record(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        input: CharacterTomRecordInput,
    ) -> Result<(String, u64, String), NexusApiError> {
        self.admit_viewer(
            caller_creator_id,
            viewer_character_id,
            &input.world_id,
            &input.binding_id,
        )
        .await?;
        validate_character_tom_belief_row(&input.belief, viewer_character_id)
            .map_err(map_kb_validation)?;
        if input.belief.order == Some(2) {
            let subject = input.belief.holder.as_deref().unwrap_or_default();
            self.require_active_subject_binding(caller_creator_id, subject, &input.world_id)
                .await?;
        }
        let carrier = self
            .require_admitted_carrier(
                viewer_character_id,
                &input.binding_id,
                &input.carrier_entry_id,
            )
            .await?;
        // DB-side pre-parse guard (fix round 2): distinguish invalid persisted
        // `modules_json` text (fail closed, never overwritten) from absent
        // modules, and bound the belief array before the append payload is
        // built.
        match self.probe_carrier_belief_rows(&input.carrier_entry_id).await? {
            -1 => return Err(invalid_modules_json()),
            -2 => return Err(modules_malformed()),
            n if n >= i64::try_from(MAX_BELIEF_ROWS_PER_CARRIER).unwrap_or(i64::MAX) => {
                return Err(corpus_exceeded("belief rows per carrier"));
            }
            _ => {}
        }
        if input.expected_revision >= i64::MAX {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "expected_revision exceeds the CAS increment domain".into(),
            });
        }
        let mut modules = carrier.modules.clone().unwrap_or_else(|| json!({}));
        append_belief_row(&mut modules, &input.belief)?;
        let modules_str = serde_json::to_string(&modules).map_err(internal_wire)?;
        let mind_state_id = format!("ms_{}", uuid::Uuid::new_v4().simple());
        let mind_state_wire = build_derivative_mind_state_wire(
            &input.carrier_entry_id,
            &mind_state_id,
            &input,
        )?;
        let mut tx = self.pool.begin().await.map_err(NexusApiError::from)?;
        let new_revision = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
            &mut tx,
            &input.carrier_entry_id,
            input.expected_revision,
            &modules_str,
            &mind_state_wire,
        )
        .await
        .map_err(map_local_db)?;
        tx.commit().await.map_err(NexusApiError::from)?;
        Ok((input.carrier_entry_id, new_revision, mind_state_id))
    }

    /// Stored admission: active owned viewer Character, active owned World,
    /// and the viewer's active selected binding (P2 `ActorAdmissionService`
    /// parity). Foreign/missing rows are 404; inactive rows are 409.
    async fn admit_viewer(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> Result<(), NexusApiError> {
        self.require_active_owned_character(caller_creator_id, viewer_character_id)
            .await?;
        self.require_active_owned_world(caller_creator_id, world_id)
            .await?;
        self.views
            .require_active_binding(viewer_character_id, binding_id, world_id)
            .await?;
        Ok(())
    }

    async fn require_active_owned_character(
        &self,
        creator_id: &str,
        character_id: &str,
    ) -> Result<(), NexusApiError> {
        let row = nexus_local_db::get_character(&self.pool, creator_id, character_id).await?;
        match row {
            Some(stored) if stored.status == "active" => Ok(()),
            Some(stored) => Err(NexusApiError::ConflictCoded {
                code: "character_inactive".into(),
                message: format!("character {character_id} is {}", stored.status),
            }),
            None => Err(not_found("character", character_id)),
        }
    }

    async fn require_active_owned_world(
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
            Some(row) if row.owner_creator_id == creator_id && row.status == "active" => Ok(()),
            Some(row) if row.owner_creator_id == creator_id => Err(NexusApiError::ConflictCoded {
                code: "world_inactive".into(),
                message: format!("world {world_id} is {}", row.status),
            }),
            _ => Err(not_found("world", world_id)),
        }
    }

    /// L2 subject admission: the subject Character must be an active owned
    /// Character with its own active binding to the selected World.
    async fn require_active_subject_binding(
        &self,
        caller_creator_id: &str,
        subject_character_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        self.require_active_owned_character(caller_creator_id, subject_character_id)
            .await?;
        let row = sqlx::query_scalar!(
            r#"SELECT binding_id AS "binding_id!" FROM actor_world_bindings
               WHERE character_id = ? AND world_id = ? AND status = 'active' LIMIT 1"#,
            subject_character_id,
            world_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        if row.is_some() {
            Ok(())
        } else {
            Err(not_found("character_world_binding", subject_character_id))
        }
    }

    /// Bounded carrier enumeration: each owner scope admits at most
    /// `MAX_CARRIERS_PER_SCOPE` rows (fetched as cap + 1 to detect overflow);
    /// exceeding the cap fails closed before any row is materialized into the
    /// belief-row working set.
    async fn authorized_carriers(
        &self,
        viewer_character_id: &str,
        binding_id: &str,
    ) -> Result<Vec<KnowledgeEntryRecord>, NexusApiError> {
        let probe = MAX_CARRIERS_PER_SCOPE + 1;
        let mut out = self
            .store
            .list_by_owner_keyset(&KnowledgeOwnerRef::character(viewer_character_id), None, probe, false)
            .await
            .map_err(map_kb_store)?;
        if out.len() > MAX_CARRIERS_PER_SCOPE as usize {
            return Err(corpus_exceeded("character-owned carriers"));
        }
        let binding_carriers = self
            .store
            .list_by_owner_keyset(&KnowledgeOwnerRef::actor_world_binding(binding_id), None, probe, false)
            .await
            .map_err(map_kb_store)?;
        if binding_carriers.len() > MAX_CARRIERS_PER_SCOPE as usize {
            return Err(corpus_exceeded("binding-owned carriers"));
        }
        out.extend(binding_carriers);
        Ok(out)
    }

    async fn require_admitted_carrier(
        &self,
        viewer_character_id: &str,
        binding_id: &str,
        carrier_entry_id: &str,
    ) -> Result<KnowledgeEntryRecord, NexusApiError> {
        let carrier = self
            .store
            .get_knowledge_entry(carrier_entry_id)
            .await
            .map_err(|err| match err {
                KbStoreError::NotFound(_) => not_found("carrier_entry", carrier_entry_id),
                other => map_kb_store(other),
            })?;
        if matches!(carrier.status.as_str(), "deleted" | "merged" | "deprecated") {
            return Err(not_found("carrier_entry", carrier_entry_id));
        }
        match &carrier.owner {
            KnowledgeOwnerRef::Character(id) if id == viewer_character_id => Ok(carrier),
            KnowledgeOwnerRef::ActorWorldBinding(id) if id == binding_id => Ok(carrier),
            KnowledgeOwnerRef::World(_) => Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "World-owned KnowledgeEntry cannot be a Character ToM carrier".into(),
            }),
            _ => Err(not_found("carrier_entry", carrier_entry_id)),
        }
    }

    /// Latest derivative MindState `occurred_at` per carrier in the viewer's
    /// two admitted owner scopes. The `GROUP BY holder_entry_id` aggregation
    /// with `MAX(created_at)` returns at most one row per carrier — derivative
    /// history is never fetched unbounded (fix round 2). Callers run this only
    /// after carrier-count bounds have been enforced, so the result set is
    /// bounded by `2 * MAX_CARRIERS_PER_SCOPE`.
    async fn carrier_recorded_at_map(
        &self,
        viewer_character_id: &str,
        binding_id: &str,
    ) -> Result<std::collections::HashMap<String, Option<String>>, NexusApiError> {
        #[derive(sqlx::FromRow)]
        struct LatestDerivative {
            carrier_entry_id: String,
            occurred_at: Option<String>,
        }
        let rows = sqlx::query_as!(
            LatestDerivative,
            r#"SELECT m.holder_entry_id AS "carrier_entry_id!", m.occurred_at
               FROM mind_states m
               INNER JOIN kb_key_blocks kb ON kb.key_block_id = m.holder_entry_id
               WHERE (kb.character_id = ? OR kb.actor_world_binding_id = ?)
                 AND NOT EXISTS (
                   SELECT 1 FROM mind_states m2
                   WHERE m2.holder_entry_id = m.holder_entry_id
                     AND (m2.created_at > m.created_at
                          OR (m2.created_at = m.created_at
                              AND m2.mind_state_id > m.mind_state_id))
                 )"#,
            viewer_character_id,
            binding_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        // Anti-join picks exactly one row per carrier — the latest derivative
        // by `(created_at, mind_state_id)` — so derivative history is never
        // materialized unbounded (one bounded SQL result per carrier).
        Ok(rows
            .into_iter()
            .map(|row| (row.carrier_entry_id, row.occurred_at))
            .collect())
    }

    /// Pre-parse storage bound probe (fix round 2): counts each carrier's
    /// `modules.belief` elements via SQLite JSON functions before any
    /// `modules_json` text is parsed or materialized into a record.
    ///
    /// `belief_rows` semantics: `>= 0` element count; `-1` non-NULL invalid
    /// JSON text (fail closed, never coerced to absent); `-2` present
    /// non-array `belief` member. The `LIMIT` probe of cap + 1 detects
    /// over-cap carrier corpora without materializing them.
    async fn probe_scope_belief_bounds(
        &self,
        owner: &KnowledgeOwnerRef,
    ) -> Result<(), NexusApiError> {
        #[derive(sqlx::FromRow)]
        struct BeliefProbeRow {
            belief_rows: i64,
        }
        let probe = i64::from(MAX_CARRIERS_PER_SCOPE) + 1;
        // SAFETY: runtime query_as — SQLite cannot provide a declared type for
        // the JSON1 expression columns (`json_valid` / `json_array_length`) so
        // the compile-time `query_as!` macro reports a NULL column type. Same
        // documented JSON1-projection rationale as
        // `nexus-orchestration::storage::sqlite` (`list_checkpoint_rows`).
        let rows: Vec<BeliefProbeRow> = match owner {
            KnowledgeOwnerRef::Character(id) => sqlx::query_as::<_, BeliefProbeRow>(
                r#"SELECT
                          CAST(
                            CASE
                              WHEN modules_json IS NULL THEN 0
                              WHEN json_valid(modules_json) = 0 THEN -1
                              ELSE COALESCE(json_array_length(modules_json, '$.belief'), -2)
                            END AS INTEGER
                          ) AS belief_rows
                   FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT ?"#,
            )
            .bind(id)
            .bind(probe)
            .fetch_all(&self.pool)
            .await
            .map_err(NexusApiError::from)?,
            KnowledgeOwnerRef::ActorWorldBinding(id) => sqlx::query_as::<_, BeliefProbeRow>(
                r#"SELECT
                          CAST(
                            CASE
                              WHEN modules_json IS NULL THEN 0
                              WHEN json_valid(modules_json) = 0 THEN -1
                              ELSE COALESCE(json_array_length(modules_json, '$.belief'), -2)
                            END AS INTEGER
                          ) AS belief_rows
                   FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT ?"#,
            )
            .bind(id)
            .bind(probe)
            .fetch_all(&self.pool)
            .await
            .map_err(NexusApiError::from)?,
            KnowledgeOwnerRef::World(_) => {
                return Err(NexusApiError::Internal {
                    code: "CHARACTER_TOM_SCOPE_INVALID".into(),
                    message: "ToM carrier scope is never World-owned".into(),
                });
            }
        };
        if rows.len() > MAX_CARRIERS_PER_SCOPE as usize {
            return Err(corpus_exceeded(match owner {
                KnowledgeOwnerRef::Character(_) => "character-owned carriers",
                _ => "binding-owned carriers",
            }));
        }
        for row in rows {
            match row.belief_rows {
                -1 => return Err(invalid_modules_json()),
                -2 => return Err(modules_malformed()),
                n if n > i64::from(MAX_BELIEF_ROWS_PER_CARRIER as u32) => {
                    return Err(corpus_exceeded("belief rows per carrier"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Single-carrier variant of the bound probe for the record path.
    async fn probe_carrier_belief_rows(
        &self,
        carrier_entry_id: &str,
    ) -> Result<i64, NexusApiError> {
        #[derive(sqlx::FromRow)]
        struct BeliefProbeRow {
            belief_rows: i64,
        }
        // SAFETY: runtime query_as — JSON1 expression columns (`json_valid` /
        // `json_array_length`) have no SQLite declared type, so the compile-time
        // macro reports NULL. Same rationale as the scope probe above.
        let rows = sqlx::query_as::<_, BeliefProbeRow>(
            r#"SELECT CAST(
                        CASE
                          WHEN modules_json IS NULL THEN 0
                          WHEN json_valid(modules_json) = 0 THEN -1
                          ELSE COALESCE(json_array_length(modules_json, '$.belief'), -2)
                        END AS INTEGER
                      ) AS belief_rows
               FROM kb_key_blocks
               WHERE key_block_id = ?"#,
        )
        .bind(carrier_entry_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        // Carrier existence was already established by require_admitted_carrier.
        Ok(rows.map_or(0, |row| row.belief_rows))
    }
}

/// Borrow the carrier's `modules.belief` array elements.
///
/// Malformed stored shapes fail closed: a present non-object `modules` value
/// or a present non-array `belief` member is a deterministic
/// `carrier_modules_malformed` conflict — never a panic, never a silent
/// rewrite. Absent `modules`/`belief` yields an empty slice.
fn carrier_belief_elements(modules: Option<&Value>) -> Result<&[Value], NexusApiError> {
    let Some(modules) = modules else {
        return Ok(&[]);
    };
    let obj = modules.as_object().ok_or_else(modules_malformed)?;
    match obj.get("belief") {
        None => Ok(&[]),
        Some(value) => value.as_array().map(Vec::as_slice).ok_or_else(modules_malformed),
    }
}

/// Non-NULL `modules_json` that is not valid JSON text — distinguishable from
/// absent modules and from shape-malformed modules; fail-closed, never
/// overwritten (fix round 2).
fn invalid_modules_json() -> NexusApiError {
    NexusApiError::ConflictCoded {
        code: "carrier_modules_invalid_json".into(),
        message: "carrier modules_json is not valid JSON text; refusing to read or overwrite it"
            .into(),
    }
}

fn modules_malformed() -> NexusApiError {
    NexusApiError::ConflictCoded {
        code: "carrier_modules_malformed".into(),
        message: "carrier modules must be an object and modules.belief, when present, an array"
            .into(),
    }
}

fn corpus_exceeded(what: &str) -> NexusApiError {
    NexusApiError::ConflictCoded {
        code: "view_incomplete".into(),
        message: format!(
            "Character ToM corpus exceeds the fixed {what} bound; refusing unbounded work"
        ),
    }
}

/// Append `row` to `modules.belief`, preserving every unknown sibling module
/// key and existing element verbatim. Malformed stored shapes reject without
/// mutation instead of panicking or overwriting a non-array `belief` value.
fn append_belief_row(modules: &mut Value, row: &BeliefPropositionRaw) -> Result<(), NexusApiError> {
    if !modules.is_object() {
        return Err(modules_malformed());
    }
    let obj = modules.as_object_mut().expect("checked is_object above");
    if let Some(existing) = obj.get("belief") {
        if !existing.is_array() {
            return Err(modules_malformed());
        }
    }
    let belief = obj.entry("belief").or_insert_with(|| json!([]));
    belief
        .as_array_mut()
        .expect("checked is_array above")
        .push(serde_json::to_value(row).map_err(internal_wire)?);
    Ok(())
}

fn build_derivative_mind_state_wire(
    carrier_entry_id: &str,
    mind_state_id: &str,
    input: &CharacterTomRecordInput,
) -> Result<Value, NexusApiError> {
    let occurred_at = input
        .occurred_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let mut wire = json!({
        "schema_version": 1,
        "mind_state_id": mind_state_id,
        "holder_entry_id": carrier_entry_id,
        "canonical_name": input.belief.proposition.clone().unwrap_or_default(),
        "occurred_at": occurred_at,
        "sort_key": input.sort_key.clone().unwrap_or_else(|| "0001".to_string()),
        "snapshot": {
            "belief": serde_json::to_value(&input.belief).map_err(internal_wire)?
        },
        "deltas": [],
        "extensions": { "nexus": { "character_tom": true } }
    });
    if let Some(event_id) = &input.event_id {
        wire["source_anchor"] = json!({ "event_id": event_id });
    }
    Ok(wire)
}

/// Map a generated record request into domain input + belief row.
pub fn record_input_from_request(
    req: &RecordCharacterTomRequest,
    expected_revision: i64,
) -> Result<CharacterTomRecordInput, NexusApiError> {
    let belief = BeliefPropositionRaw {
        holder: Some(newtype_wire_string(&req.holder)),
        proposition: Some(newtype_wire_string(&req.proposition)),
        order: Some(req.order),
        truth: req.truth.as_ref().map(enum_wire_string),
        access: req.access.as_ref().map(enum_wire_string),
        representation: req.representation.as_ref().map(enum_wire_string),
        content_type: req.content_type.as_ref().map(enum_wire_string),
        source: req.source.as_ref().map(enum_wire_string),
        context: req.context.as_ref().map(enum_wire_string),
    };
    Ok(CharacterTomRecordInput {
        world_id: newtype_wire_string(&req.world_id),
        binding_id: newtype_wire_string(&req.binding_id),
        carrier_entry_id: req.carrier_entry_id.clone(),
        expected_revision,
        belief,
        occurred_at: req.occurred_at.map(|dt| dt.to_rfc3339()),
        sort_key: req.sort_key.clone(),
        event_id: req
            .event_id
            .as_ref()
            .map(newtype_wire_string),
    })
}

fn newtype_wire_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn enum_wire_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn invalid_cursor() -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: "cursor is not a valid opaque Character ToM keyset token".into(),
    }
}

fn not_found(resource: &str, id: &str) -> NexusApiError {
    NexusApiError::NotFound(format!("{resource} {id}"))
}

fn internal_wire(err: serde_json::Error) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_TOM_WIRE_INVALID".into(),
        message: err.to_string(),
    }
}

fn map_kb_validation(err: nexus_knowledge::world_kb::KbError) -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    }
}

fn map_kb_store(err: KbStoreError) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_TOM_KB_FAILED".into(),
        message: err.to_string(),
    }
}

fn map_local_db(err: LocalDbError) -> NexusApiError {
    match err {
        LocalDbError::VersionMismatch { .. } => NexusApiError::ConflictCoded {
            code: "version_mismatch".into(),
            message: err.to_string(),
        },
        LocalDbError::ValidationError(msg) => NexusApiError::BadRequest {
            code: "invalid_input".into(),
            message: msg,
        },
        other => NexusApiError::Internal {
            code: "CHARACTER_TOM_DB_FAILED".into(),
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_orders_l1_before_l2_then_carrier_then_ordinal() {
        let l2 = BeliefPropositionRaw {
            holder: Some("chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()),
            proposition: Some("they".into()),
            order: Some(2),
            truth: None,
            access: None,
            representation: None,
            content_type: None,
            source: None,
            context: None,
        };
        let l1 = BeliefPropositionRaw {
            holder: Some("chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            proposition: Some("me".into()),
            order: Some(1),
            ..l2.clone()
        };
        let rows = vec![
            (
                2,
                "kb_b".into(),
                0,
                CharacterTomBeliefRow {
                    carrier_entry_id: "kb_b".into(),
                    row_ordinal: 0,
                    belief: l2,
                    carrier_recorded_at: None,
                },
            ),
            (
                1,
                "kb_a".into(),
                1,
                CharacterTomBeliefRow {
                    carrier_entry_id: "kb_a".into(),
                    row_ordinal: 1,
                    belief: l1,
                    carrier_recorded_at: None,
                },
            ),
        ];
        let page = CharacterTomService::paginate(rows, None, 10);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].belief.order, Some(1));
        assert_eq!(page.items[1].belief.order, Some(2));
    }

    #[test]
    fn cursor_round_trip_and_keyset_skips_prior_rows() {
        let base = BeliefPropositionRaw {
            holder: Some("chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            proposition: Some("p".into()),
            order: Some(1),
            truth: None,
            access: None,
            representation: None,
            content_type: None,
            source: None,
            context: None,
        };
        let rows = vec![
            (
                1,
                "kb_a".into(),
                0,
                CharacterTomBeliefRow {
                    carrier_entry_id: "kb_a".into(),
                    row_ordinal: 0,
                    belief: base.clone(),
                    carrier_recorded_at: None,
                },
            ),
            (
                1,
                "kb_a".into(),
                1,
                CharacterTomBeliefRow {
                    carrier_entry_id: "kb_a".into(),
                    row_ordinal: 1,
                    belief: base,
                    carrier_recorded_at: None,
                },
            ),
        ];
        let cursor = CharacterTomService::encode_cursor(1, "kb_a", 0);
        let decoded = CharacterTomService::decode_cursor(&Some(cursor)).unwrap();
        assert_eq!(decoded, Some((1, "kb_a".into(), 0)));
        let page = CharacterTomService::paginate(rows, decoded, 10);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].row_ordinal, 1);
    }
}
