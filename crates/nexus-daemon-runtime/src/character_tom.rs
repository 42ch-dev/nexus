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
    pub async fn list(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        query: CharacterTomListQuery,
    ) -> Result<CharacterTomPage, NexusApiError> {
        self.admit_viewer(caller_creator_id, viewer_character_id, &query.world_id, &query.binding_id)
            .await?;
        let cursor = Self::decode_cursor(&query.cursor)?;
        let carriers = self.authorized_carriers(viewer_character_id, &query.binding_id).await?;
        let mut keyed = Vec::new();
        for carrier in carriers {
            let recorded_at = self.latest_carrier_recorded_at(&carrier.entry_id).await?;
            for (ordinal, belief) in carrier.parse_belief_rows().into_iter().enumerate() {
                if validate_character_tom_belief_row(&belief, viewer_character_id).is_err() {
                    continue;
                }
                let order = belief.order.unwrap_or(0);
                keyed.push((
                    order,
                    carrier.entry_id.clone(),
                    u32::try_from(ordinal).unwrap_or(0),
                    CharacterTomBeliefRow {
                        carrier_entry_id: carrier.entry_id.clone(),
                        row_ordinal: u32::try_from(ordinal).unwrap_or(0),
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
            self.require_active_subject_binding(subject, &input.world_id)
                .await?;
        }
        let carrier = self
            .require_admitted_carrier(
                viewer_character_id,
                &input.binding_id,
                &input.carrier_entry_id,
            )
            .await?;
        let mut modules = carrier.modules.clone().unwrap_or_else(|| json!({}));
        append_belief_row(&mut modules, &input.belief);
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

    async fn admit_viewer(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> Result<(), NexusApiError> {
        self.views
            .require_owned_character(caller_creator_id, viewer_character_id)
            .await?;
        self.views.require_owned_world(caller_creator_id, world_id).await?;
        self.views
            .require_active_binding(viewer_character_id, binding_id, world_id)
            .await?;
        Ok(())
    }

    async fn require_active_subject_binding(
        &self,
        subject_character_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        let row: Option<String> = sqlx::query_scalar(
            r"SELECT binding_id FROM actor_world_bindings
              WHERE character_id = ? AND world_id = ? AND status = 'active' LIMIT 1",
        )
        .bind(subject_character_id)
        .bind(world_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        if row.is_some() {
            Ok(())
        } else {
            Err(not_found("character_world_binding", subject_character_id))
        }
    }

    async fn authorized_carriers(
        &self,
        viewer_character_id: &str,
        binding_id: &str,
    ) -> Result<Vec<KnowledgeEntryRecord>, NexusApiError> {
        let mut out = self
            .store
            .list_by_owner_complete(&KnowledgeOwnerRef::character(viewer_character_id))
            .await
            .map_err(map_kb_store)?;
        out.extend(
            self.store
                .list_by_owner_complete(&KnowledgeOwnerRef::actor_world_binding(binding_id))
                .await
                .map_err(map_kb_store)?,
        );
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
            .map_err(|_| not_found("carrier_entry", carrier_entry_id))?;
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

    async fn latest_carrier_recorded_at(
        &self,
        carrier_entry_id: &str,
    ) -> Result<Option<String>, NexusApiError> {
        let row: Option<String> = sqlx::query_scalar(
            r"SELECT occurred_at FROM mind_states
              WHERE holder_entry_id = ?
              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(carrier_entry_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(NexusApiError::from)?;
        Ok(row)
    }
}

fn append_belief_row(modules: &mut Value, row: &BeliefPropositionRaw) {
    let obj = modules.as_object_mut().expect("modules object");
    let belief = obj.entry("belief").or_insert_with(|| json!([]));
    if !belief.is_array() {
        *belief = json!([]);
    }
    belief
        .as_array_mut()
        .expect("belief array")
        .push(serde_json::to_value(row).expect("belief row serializes"));
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
