//! Character `ToM` L1/L2 record and bounded query (v1.184 P4 Task 2).
//!
//! Composes P1 [`ActorKnowledgeViewService`] admission, P2 stored-owner checks,
//! and the Task 1 atomic carrier CAS + derivative `MindState` seam. Record and
//! query make zero provider calls.

use crate::actor_knowledge_view::ActorKnowledgeViewService;
use crate::api::errors::NexusApiError;
use nexus_contracts::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_knowledge::world_kb::knowledge_entry::{
    validate_character_tom_belief_row, BeliefPropositionRaw, KnowledgeEntryRecord,
    KnowledgeOwnerRef,
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
/// `MAX_BELIEF_ROWS_PER_CARRIER` as `i64` for compile-time SQL bind args.
// `MAX_BELIEF_ROWS_PER_CARRIER` is a compile-time literal (200); the i64
// constant mirrors it exactly so SQL bind args never cast from usize.
const MAX_BELIEF_ROWS_PER_CARRIER_I64: i64 = 200;

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
    /// Optional order filter (`1` = L1, `2` = L2) so MCA can fill each slot
    /// with an independent bounded fetch (QC fix round 1, F-003). The public
    /// list route always passes `None`.
    pub order: Option<i64>,
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

/// Admitted carrier probe row: typed base columns only, never `modules_json`,
/// so the pre-parse bound holds before any serde materialization.
#[derive(sqlx::FromRow, Debug, Clone)]
pub struct ProbeCarrier {
    pub key_block_id: String,
    pub revision: Option<i64>,
    pub status: String,
    pub character_id: Option<String>,
    pub actor_world_binding_id: Option<String>,
}

/// Stored `modules_json` violation category, used to classify a carrier that
/// fails the probe-ok predicate with the matching fail-closed error.
#[derive(Debug, Clone, Copy)]
enum ViolationKind {
    InvalidJson,
    Malformed,
    Oversized,
}

/// Reusable Character `ToM` composer (record + query).
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
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when `raw` is outside `1..=100`.
    pub fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
        ActorKnowledgeViewService::resolve_limit(raw)
    }

    /// Decode opaque `(order, carrier_entry_id, row_ordinal)` cursor.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when the cursor is malformed.
    pub fn decode_cursor(
        cursor: &Option<String>,
    ) -> Result<Option<(i64, String, u32)>, NexusApiError> {
        match cursor {
            None => Ok(None),
            Some(raw) => {
                let rest = raw.strip_prefix(CURSOR_PREFIX).ok_or_else(invalid_cursor)?;
                let parts: Vec<&str> = rest.split(CURSOR_SEP).collect();
                if parts.len() != 3
                    || parts[0].is_empty()
                    || parts[1].is_empty()
                    || parts[2].is_empty()
                {
                    return Err(invalid_cursor());
                }
                let order = parts[0].parse::<i64>().map_err(|_| invalid_cursor())?;
                let ordinal = parts[2].parse::<u32>().map_err(|_| invalid_cursor())?;
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
    #[must_use]
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
            rows.retain(|(o, c, ord, _)| {
                (*o, c.as_str(), *ord) > (order, carrier.as_str(), ordinal)
            });
        }
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = rows.len() > limit_us;
        rows.truncate(limit_us);
        let items: Vec<CharacterTomBeliefRow> =
            rows.into_iter().map(|(_, _, _, row)| row).collect();
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

    /// List Character `ToM` rows from authorized carriers only.
    ///
    /// # Errors
    ///
    /// Returns an admission/validation `NexusApiError` on unauthorized or invalid input.
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
        self.admit_viewer(
            caller_creator_id,
            viewer_character_id,
            &query.world_id,
            &query.binding_id,
        )
        .await?;
        let cursor = Self::decode_cursor(&query.cursor)?;
        let order_filter = query.order;
        // DB-side pre-parse bounds (fix round 3): carrier counts, belief-array
        // lengths, and modules JSON validity are enforced with compile-time SQL
        // before any `modules_json` text is parsed or materialized into records.
        let mut admitted_ids: Vec<String> = Vec::new();
        for owner in [
            KnowledgeOwnerRef::character(viewer_character_id),
            KnowledgeOwnerRef::actor_world_binding(&query.binding_id),
        ] {
            self.probe_scope_violations(&owner).await?;
            let admitted = self.probe_scope_carriers(&owner).await?;
            admitted_ids.extend(admitted.into_iter().map(|c| c.key_block_id));
        }
        // The timestamp lookup is constrained to exactly this concrete admitted
        // carrier-id snapshot — never an owner-scope rescan — so carriers
        // inserted/changed after the probe cannot enter the result (fix round 4).
        let recorded = self.carrier_recorded_at_map(&admitted_ids).await?;
        // Materialize exactly the probe-admitted id snapshot — never a second
        // owner-scope rescan (QC fix round 1, F-001). Status/ownership drift,
        // invalid `modules_json`, and oversized belief arrays discovered here
        // fail closed instead of entering or silently dropping rows.
        let carriers = self
            .materialize_admitted_carriers(&admitted_ids, viewer_character_id, &query.binding_id)
            .await?;
        let mut keyed = Vec::new();
        for (entry_id, modules) in carriers {
            let recorded_at = recorded.get(&entry_id).cloned().flatten();
            let rows = carrier_belief_elements(modules.as_ref())?;
            for (ordinal, element) in rows.iter().enumerate() {
                let Ok(belief) = serde_json::from_value::<BeliefPropositionRaw>(element.clone())
                else {
                    continue; // malformed legacy element: skip, keep physical ordinal
                };
                if validate_character_tom_belief_row(&belief, viewer_character_id).is_err() {
                    continue;
                }
                let order = belief.order.unwrap_or(0);
                if let Some(want) = order_filter {
                    if order != want {
                        continue;
                    }
                }
                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| corpus_exceeded("belief rows per carrier"))?;
                keyed.push((
                    order,
                    entry_id.clone(),
                    ordinal,
                    CharacterTomBeliefRow {
                        carrier_entry_id: entry_id.clone(),
                        row_ordinal: ordinal,
                        belief,
                        carrier_recorded_at: recorded_at.clone(),
                    },
                ));
            }
        }
        Ok(Self::paginate(keyed, cursor, query.limit))
    }

    /// Record one L1/L2 belief on an authorized carrier (atomic CAS + `MindState`).
    ///
    /// # Errors
    ///
    /// Returns `invalid_input`/`not_found`/`conflict` on bad input, stale
    /// scopes, or CAS/ownership drift.
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
            .map_err(|e| map_kb_validation(&e))?;
        if input.belief.order == Some(2) {
            let subject = input.belief.holder.as_deref().unwrap_or_default();
            self.require_active_subject_binding(caller_creator_id, subject, &input.world_id)
                .await?;
        }
        // Pre-parse carrier probe (fix round 3): the stored `modules_json`
        // value is type/length/validity checked via compile-time SQL before any
        // `get_knowledge_entry` / serde parse. Invalid text, a present non-array
        // `belief`, or an oversized array fail closed without mutation.
        let probe = self
            .probe_carrier(&input.carrier_entry_id)
            .await?
            .ok_or_else(|| not_found("carrier_entry", &input.carrier_entry_id))?;
        if matches!(probe.status.as_str(), "deleted" | "merged" | "deprecated") {
            return Err(not_found("carrier_entry", &input.carrier_entry_id));
        }
        match &probe.character_id {
            Some(id) if id == viewer_character_id => {}
            _ => match &probe.actor_world_binding_id {
                Some(id) if id == &input.binding_id => {}
                _ if probe.character_id.is_none() && probe.actor_world_binding_id.is_none() => {
                    return Err(NexusApiError::BadRequest {
                        code: "invalid_input".into(),
                        message: "World-owned KnowledgeEntry cannot be a Character ToM carrier"
                            .into(),
                    });
                }
                _ => return Err(not_found("carrier_entry", &input.carrier_entry_id)),
            },
        }
        // The CAS bump is a checked `+ 1` on this i64 revision; only the exact
        // `i64::MAX` input can overflow it, so reject that single value
        // deterministically (equality form also satisfies
        // clippy::absurd_extreme_comparisons).
        if input.expected_revision == i64::MAX {
            return Err(NexusApiError::BadRequest {
                code: "invalid_input".into(),
                message: "expected_revision exceeds the CAS increment domain".into(),
            });
        }
        let carrier = self
            .require_admitted_carrier(
                viewer_character_id,
                &input.binding_id,
                &input.carrier_entry_id,
            )
            .await?;
        let mut modules = carrier.modules.clone().unwrap_or_else(|| json!({}));
        append_belief_row(&mut modules, &input.belief)?;
        let modules_str = serde_json::to_string(&modules).map_err(|e| internal_wire(&e))?;
        let mind_state_id = format!("ms_{}", uuid::Uuid::new_v4().simple());
        let mind_state_wire =
            build_derivative_mind_state_wire(&input.carrier_entry_id, &mind_state_id, &input)?;
        let mut tx = self.pool.begin().await.map_err(NexusApiError::from)?;
        // PR #240 finding 3: the pre-transaction admission can go stale before
        // commit. Revalidate the complete live scope — active owned viewer
        // Character, active owned World, active selected binding, and the L2
        // subject's own active Character + binding — inside the same
        // transaction as the CAS, so a removed/deactivated binding or
        // lifecycle flip rolls back instead of committing under a stale
        // viewpoint.
        let l2_subject = if input.belief.order == Some(2) {
            Some(input.belief.holder.as_deref().unwrap_or_default())
        } else {
            None
        };
        Self::revalidate_live_scope_in_tx(
            &mut tx,
            caller_creator_id,
            viewer_character_id,
            &input.world_id,
            &input.binding_id,
            l2_subject,
        )
        .await?;
        let new_revision = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
            &mut tx,
            &input.carrier_entry_id,
            input.expected_revision,
            &modules_str,
            &mind_state_wire,
            viewer_character_id,
            &input.binding_id,
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

    /// In-transaction revalidation of the complete live record scope
    /// (PR #240 finding 3). Runs inside the CAS transaction; any drift
    /// (inactive/foreign/missing Character, World, binding, or L2 subject)
    /// returns an error so the caller drops the transaction uncommitted.
    async fn revalidate_live_scope_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
        l2_subject: Option<&str>,
    ) -> Result<(), NexusApiError> {
        let chr = sqlx::query!(
            r#"SELECT status AS "status!" FROM characters
               WHERE character_id = ? AND owner_creator_id = ?"#,
            viewer_character_id,
            caller_creator_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(NexusApiError::from)?;
        match chr {
            Some(row) if row.status == "active" => {}
            Some(row) => {
                return Err(NexusApiError::ConflictCoded {
                    code: "character_inactive".into(),
                    message: format!("character {viewer_character_id} is {}", row.status),
                })
            }
            None => return Err(not_found("character", viewer_character_id)),
        }
        let world = sqlx::query!(
            r#"SELECT owner_creator_id AS "owner_creator_id!", status AS "status!"
               FROM narrative_worlds WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(NexusApiError::from)?;
        match world {
            Some(row) if row.owner_creator_id == caller_creator_id && row.status == "active" => {}
            Some(row) if row.owner_creator_id == caller_creator_id => {
                return Err(NexusApiError::ConflictCoded {
                    code: "world_inactive".into(),
                    message: format!("world {world_id} is {}", row.status),
                })
            }
            _ => return Err(not_found("world", world_id)),
        }
        let binding = sqlx::query!(
            r#"SELECT character_id AS "character_id!", world_id AS "world_id!",
                      status AS "status!"
               FROM actor_world_bindings WHERE binding_id = ?"#,
            binding_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(NexusApiError::from)?;
        match binding {
            Some(row)
                if row.character_id == viewer_character_id
                    && row.world_id == world_id
                    && row.status == "active" => {}
            _ => return Err(not_found("actor_world_binding", binding_id)),
        }
        if let Some(subject) = l2_subject {
            let subj = sqlx::query!(
                r#"SELECT status AS "status!" FROM characters
                   WHERE character_id = ? AND owner_creator_id = ?"#,
                subject,
                caller_creator_id
            )
            .fetch_optional(&mut **tx)
            .await
            .map_err(NexusApiError::from)?;
            match subj {
                Some(row) if row.status == "active" => {}
                Some(row) => {
                    return Err(NexusApiError::ConflictCoded {
                        code: "character_inactive".into(),
                        message: format!("character {subject} is {}", row.status),
                    })
                }
                None => return Err(not_found("character", subject)),
            }
            let subject_binding = sqlx::query_scalar!(
                r#"SELECT binding_id AS "binding_id!" FROM actor_world_bindings
                   WHERE character_id = ? AND world_id = ? AND status = 'active' LIMIT 1"#,
                subject,
                world_id
            )
            .fetch_optional(&mut **tx)
            .await
            .map_err(NexusApiError::from)?;
            if subject_binding.is_none() {
                return Err(not_found("character_world_binding", subject));
            }
        }
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

    /// Materialize exactly the probe-admitted carrier ids (QC fix round 1,
    /// F-001) in probe-admission order. Each row is revalidated at
    /// materialization: still live, still owned by the admitted
    /// Character/binding, `modules_json` still valid JSON, belief array still
    /// within the row cap. Any drift fails closed with
    /// `carrier_scope_drifted` / `carrier_modules_invalid_json` /
    /// `carrier_modules_malformed` / `view_incomplete` — never a silent
    /// omission, never an unadmitted carrier.
    async fn materialize_admitted_carriers(
        &self,
        admitted_ids: &[String],
        viewer_character_id: &str,
        binding_id: &str,
    ) -> Result<Vec<(String, Option<Value>)>, NexusApiError> {
        #[derive(sqlx::FromRow)]
        struct AdmittedCarrierRow {
            key_block_id: String,
            status: String,
            character_id: Option<String>,
            actor_world_binding_id: Option<String>,
            modules_json: Option<String>,
        }
        if admitted_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids_json = serde_json::to_string(admitted_ids).map_err(|e| internal_wire(&e))?;
        let rows = sqlx::query_as!(
            AdmittedCarrierRow,
            r#"SELECT key_block_id AS "key_block_id!", status AS "status!",
                      character_id, actor_world_binding_id, modules_json
               FROM kb_key_blocks WHERE key_block_id IN (SELECT value FROM json_each(?))"#,
            ids_json
        )
        .fetch_all(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        let mut by_id: std::collections::HashMap<String, AdmittedCarrierRow> = rows
            .into_iter()
            .map(|row| (row.key_block_id.clone(), row))
            .collect();
        let mut out = Vec::with_capacity(admitted_ids.len());
        for id in admitted_ids {
            let row = by_id.remove(id).ok_or_else(|| carrier_scope_drifted(id))?;
            if matches!(row.status.as_str(), "deleted" | "merged" | "deprecated") {
                return Err(carrier_scope_drifted(id));
            }
            let owner_ok = row.character_id.as_deref() == Some(viewer_character_id)
                || row.actor_world_binding_id.as_deref() == Some(binding_id);
            if !owner_ok {
                return Err(carrier_scope_drifted(id));
            }
            let modules = match &row.modules_json {
                None => None,
                Some(text) => {
                    Some(serde_json::from_str::<Value>(text).map_err(|_| invalid_modules_json())?)
                }
            };
            // Re-check the per-carrier row cap on the materialized array: a
            // carrier admitted by the probe but appended before this read must
            // still fail closed instead of materializing oversized work.
            if carrier_belief_elements(modules.as_ref())?.len() > MAX_BELIEF_ROWS_PER_CARRIER {
                return Err(corpus_exceeded("belief rows per carrier"));
            }
            out.push((id.clone(), modules));
        }
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
                other => map_kb_store(&other),
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

    /// Latest derivative `MindState` `occurred_at` per carrier in the concrete
    /// admitted carrier-id set (fix round 4).
    ///
    /// The query constrains `holder_entry_id` to the exact admitted id snapshot
    /// (a single `json_each(?)` bind of the id array) and uses an anti-join to
    /// return at most one row per id — the latest by `(created_at,
    /// mind_state_id)`. It never rescans owner scope and never scans rows for
    /// carriers outside the admitted snapshot, so the total work is bounded by
    /// the admitted carrier cap (`<= 2 * MAX_CARRIERS_PER_SCOPE`) at the SQL
    /// boundary.
    async fn carrier_recorded_at_map(
        &self,
        admitted_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Option<String>>, NexusApiError> {
        #[derive(sqlx::FromRow)]
        struct LatestDerivative {
            carrier_entry_id: String,
            occurred_at: Option<String>,
        }
        if admitted_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let ids_json = serde_json::to_string(admitted_ids).map_err(|e| internal_wire(&e))?;
        let rows = sqlx::query_as!(
            LatestDerivative,
            r#"SELECT m.holder_entry_id AS "carrier_entry_id!", m.occurred_at
               FROM mind_states m
               WHERE m.holder_entry_id IN (SELECT value FROM json_each(?))
                 AND NOT EXISTS (
                   SELECT 1 FROM mind_states m2
                   WHERE m2.holder_entry_id = m.holder_entry_id
                     AND (m2.created_at > m.created_at
                          OR (m2.created_at = m.created_at
                              AND m2.mind_state_id > m.mind_state_id))
                 )"#,
            ids_json
        )
        .fetch_all(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        // Exactly one latest derivative row per admitted id, matching the
        // `json_each` id snapshot — never a scope rescan.
        Ok(rows
            .into_iter()
            .map(|row| (row.carrier_entry_id, row.occurred_at))
            .collect())
    }

    /// Admitted carrier probe row (typed base columns only — never selects or
    /// parses `modules_json`, so the pre-parse bound holds).
    ///
    /// A carrier is "probe-ok" when its persisted `modules_json` is either
    /// NULL (legacy absent modules), or valid JSON whose `$.belief` path is
    /// absent (treated as zero rows) or an array of at most
    /// `MAX_BELIEF_ROWS_PER_CARRIER` elements. Non-NULL invalid JSON text,
    /// a present non-array `belief`, or an oversized array are classified by
    /// [`Self::probe_scope_violations`] / [`Self::probe_carrier_violation`]
    /// and fail closed before any materialization.
    async fn probe_scope_carriers(
        &self,
        owner: &KnowledgeOwnerRef,
    ) -> Result<Vec<ProbeCarrier>, NexusApiError> {
        let rows: Vec<ProbeCarrier> = match owner {
            KnowledgeOwnerRef::Character(id) => sqlx::query_as!(
                ProbeCarrier,
                r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
                          character_id, actor_world_binding_id
                   FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       modules_json IS NULL
                       OR (
                         json_valid(modules_json)
                         AND (
                           json_type(modules_json, '$.belief') IS NULL
                           OR json_type(modules_json, '$.belief') = 'array'
                         )
                         AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                       )
                     )
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT 201"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_all(&self.pool)
            .await
            .map_err(NexusApiError::from)?,
            KnowledgeOwnerRef::ActorWorldBinding(id) => sqlx::query_as!(
                ProbeCarrier,
                r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
                          character_id, actor_world_binding_id
                   FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       modules_json IS NULL
                       OR (
                         json_valid(modules_json)
                         AND (
                           json_type(modules_json, '$.belief') IS NULL
                           OR json_type(modules_json, '$.belief') = 'array'
                         )
                         AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                       )
                     )
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT 201"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
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
        Ok(rows)
    }

    /// Detect non-probe-ok carriers in an owner scope and map them to the
    /// matching fail-closed error, before any modules parse. Returns `Ok(())`
    /// when the whole admitted scope is probe-ok.
    async fn probe_scope_violations(&self, owner: &KnowledgeOwnerRef) -> Result<(), NexusApiError> {
        let invalid = self
            .scope_violation_count(owner, ViolationKind::InvalidJson)
            .await?;
        if invalid > 0 {
            return Err(invalid_modules_json());
        }
        let malformed = self
            .scope_violation_count(owner, ViolationKind::Malformed)
            .await?;
        if malformed > 0 {
            return Err(modules_malformed());
        }
        let oversized = self
            .scope_violation_count(owner, ViolationKind::Oversized)
            .await?;
        if oversized > 0 {
            return Err(corpus_exceeded("belief rows per carrier"));
        }
        Ok(())
    }

    /// One typed `COUNT(*)` over the scope + a violation predicate. Every
    /// statement is a literal compile-time `query_scalar!` (the JSON functions
    /// live in the WHERE, never in the SELECT), so schema drift is caught at
    /// compile time.
    async fn scope_violation_count(
        &self,
        owner: &KnowledgeOwnerRef,
        kind: ViolationKind,
    ) -> Result<i64, NexusApiError> {
        match (owner, kind) {
            (KnowledgeOwnerRef::Character(id), ViolationKind::InvalidJson) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
            (KnowledgeOwnerRef::Character(id), ViolationKind::Malformed) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
            (KnowledgeOwnerRef::Character(id), ViolationKind::Oversized) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::InvalidJson) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                    id
                )
                .fetch_one(&self.pool)
                .await
                .map_err(NexusApiError::from)
            }
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::Malformed) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                    id
                )
                .fetch_one(&self.pool)
                .await
                .map_err(NexusApiError::from)
            }
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::Oversized) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                    id,
                    MAX_BELIEF_ROWS_PER_CARRIER_I64
                )
                .fetch_one(&self.pool)
                .await
                .map_err(NexusApiError::from)
            }
            (KnowledgeOwnerRef::World(_), _) => Err(NexusApiError::Internal {
                code: "CHARACTER_TOM_SCOPE_INVALID".into(),
                message: "ToM carrier scope is never World-owned".into(),
            }),
        }
    }

    /// Single-carrier probe for the record path. Returns the typed admitted
    /// carrier row when probe-ok, or `None` when the carrier is absent.
    /// Non-probe-ok carriers are classified and rejected here — before any
    /// `get_knowledge_entry` / `modules_json` parse.
    async fn probe_carrier(
        &self,
        carrier_entry_id: &str,
    ) -> Result<Option<ProbeCarrier>, NexusApiError> {
        let row = sqlx::query_as!(
            ProbeCarrier,
            r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
                      character_id, actor_world_binding_id
               FROM kb_key_blocks
               WHERE key_block_id = ?
                 AND (
                   modules_json IS NULL
                   OR (
                     json_valid(modules_json)
                     AND (
                       json_type(modules_json, '$.belief') IS NULL
                       OR json_type(modules_json, '$.belief') = 'array'
                     )
                     AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                   )
                 )"#,
            carrier_entry_id,
            MAX_BELIEF_ROWS_PER_CARRIER_I64
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        if row.is_some() {
            return Ok(row);
        }
        // No probe-ok row: classify the violation or report the carrier as
        // absent. Each check is a separate typed COUNT query.
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::InvalidJson)
            .await?
            > 0
        {
            return Err(invalid_modules_json());
        }
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::Malformed)
            .await?
            > 0
        {
            return Err(modules_malformed());
        }
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::Oversized)
            .await?
            > 0
        {
            return Err(corpus_exceeded("belief rows per carrier"));
        }
        Ok(None)
    }

    async fn carrier_violation_count(
        &self,
        carrier_entry_id: &str,
        kind: ViolationKind,
    ) -> Result<i64, NexusApiError> {
        match kind {
            ViolationKind::InvalidJson => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                carrier_entry_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
            ViolationKind::Malformed => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                carrier_entry_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
            ViolationKind::Oversized => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                carrier_entry_id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_one(&self.pool)
            .await
            .map_err(NexusApiError::from),
        }
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
    obj.get("belief").map_or_else(
        || Ok(&[][..]),
        |value| {
            value
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(modules_malformed)
        },
    )
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

/// A probe-admitted carrier changed status or ownership before
/// materialization (QC fix round 1, F-001): refuse the inconsistent snapshot.
fn carrier_scope_drifted(id: &str) -> NexusApiError {
    NexusApiError::ConflictCoded {
        code: "carrier_scope_drifted".into(),
        message: format!(
            "admitted carrier {id} changed status or ownership before materialization;              refusing an inconsistent ToM snapshot"
        ),
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
    let rows = belief.as_array_mut().expect("checked is_array above");
    // QC fix round 1 (F-002): never append the row that would exceed the
    // per-carrier cap — a carrier at the cap rejects here, before any CAS or
    // MindState write, so the corpus can never become un-listable.
    if rows.len() >= MAX_BELIEF_ROWS_PER_CARRIER {
        return Err(corpus_exceeded("belief rows per carrier"));
    }
    rows.push(serde_json::to_value(row).map_err(|e| internal_wire(&e))?);
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
            "belief": serde_json::to_value(&input.belief).map_err(|e| internal_wire(&e))?
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
///
/// # Errors
///
/// Returns `invalid_input` when the wire record is malformed.
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
        event_id: req.event_id.as_ref().map(newtype_wire_string),
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

fn internal_wire(err: &serde_json::Error) -> NexusApiError {
    NexusApiError::Internal {
        code: "CHARACTER_TOM_WIRE_INVALID".into(),
        message: err.to_string(),
    }
}

fn map_kb_validation(err: &nexus_knowledge::world_kb::KbError) -> NexusApiError {
    NexusApiError::BadRequest {
        code: "invalid_input".into(),
        message: err.to_string(),
    }
}

fn map_kb_store(err: &KbStoreError) -> NexusApiError {
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

    /// PR #240 finding 3: the in-transaction revalidation must reject any
    /// live-scope drift (deactivated binding, archived subject) so the CAS
    /// transaction rolls back instead of committing under a stale viewpoint.
    // Drift matrix: sequential binding/subject deactivation cases; one contract.
    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn revalidate_live_scope_in_tx_rejects_binding_and_subject_drift() {
        let (_tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        let owner = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let world = "wld_scope";
        nexus_local_db::ensure_creator_row(&pool, owner, "Owner")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world)
        .bind(owner)
        .bind(world)
        .bind(world)
        .execute(&pool)
        .await
        .unwrap();
        for chr in [
            "chr_a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
            "chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
        ] {
            sqlx::query(
                "INSERT INTO characters \
                 (character_id, owner_creator_id, display_name, status, image_uri, persona_json, created_at, updated_at) \
                 VALUES (?, ?, ?, 'active', NULL, '{}', datetime('now'), datetime('now'))",
            )
            .bind(chr)
            .bind(owner)
            .bind(chr)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO actor_world_bindings \
                 (binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at) \
                 VALUES (?, ?, ?, 'active', NULL, datetime('now'), datetime('now'))",
            )
            .bind(if chr.contains("a1") { "awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3".to_string() } else { "awb_d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4".to_string() })
            .bind(chr)
            .bind(world)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Control: fully live scope validates.
        let mut tx = pool.begin().await.unwrap();
        CharacterTomService::revalidate_live_scope_in_tx(
            &mut tx,
            owner,
            "chr_a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
            world,
            "awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
            Some("chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
        )
        .await
        .expect("live scope must validate");
        tx.rollback().await.unwrap();

        // Binding deactivated mid-flight -> NotFound, transaction unusable.
        sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = 'awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3'")
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let err = CharacterTomService::revalidate_live_scope_in_tx(
            &mut tx,
            owner,
            "chr_a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
            world,
            "awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
            Some("chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
        )
        .await
        .expect_err("deactivated binding must fail in-tx");
        assert!(
            matches!(err, NexusApiError::NotFound(_)),
            "unexpected: {err}"
        );
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE actor_world_bindings SET status = 'active' WHERE binding_id = 'awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3'")
            .execute(&pool)
            .await
            .unwrap();

        // L2 subject archived mid-flight -> 409 character_inactive.
        sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = 'chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2'")
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let err = CharacterTomService::revalidate_live_scope_in_tx(
            &mut tx,
            owner,
            "chr_a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
            world,
            "awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
            Some("chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
        )
        .await
        .expect_err("archived L2 subject must fail in-tx");
        match err {
            NexusApiError::ConflictCoded { code, .. } => {
                assert_eq!(code, "character_inactive");
            }
            other => panic!("unexpected: {other}"),
        }
        tx.rollback().await.unwrap();

        // L2 subject's own binding removed -> NotFound.
        sqlx::query("UPDATE characters SET status = 'active' WHERE character_id = 'chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE actor_world_bindings SET status = 'inactive' WHERE binding_id = 'awb_d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4'")
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let err = CharacterTomService::revalidate_live_scope_in_tx(
            &mut tx,
            owner,
            "chr_a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
            world,
            "awb_c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
            Some("chr_b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
        )
        .await
        .expect_err("subject without active binding to the world must fail in-tx");
        assert!(
            matches!(err, NexusApiError::NotFound(_)),
            "unexpected: {err}"
        );
        tx.rollback().await.unwrap();
    }
}
