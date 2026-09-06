//! v1.185 P2 — stored-owner summary CAS and referent-guarded delete.

use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::validation::validate_canonical_name;
use sqlx::SqlitePool;

use crate::character::{
    check_expected_revision, require_active_owned_character_tx, FieldPatch,
};
use crate::error::ActorContractConflict;
use crate::kb_store::{map_kb_store_to_local_db, KeyBlockRow};
use crate::LocalDbError;

/// Maximum UTF-8 byte length for `body.summary` (durable §11.5).
pub const ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES: usize = 65_536;

/// Patch surface for Character-scoped knowledge maintenance (handoff §2).
#[derive(Debug, Clone, Copy)]
pub struct ActorKnowledgePatch<'a> {
    pub canonical_name: Option<&'a str>,
    pub summary: FieldPatch<&'a str>,
}

fn knowledge_revision_conflict() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::KnowledgeRevisionConflict,
    }
}

fn knowledge_entry_not_mutable() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::KnowledgeEntryNotMutable,
    }
}

fn knowledge_entry_in_use() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::KnowledgeEntryInUse,
    }
}

fn knowledge_reference_state_invalid() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::KnowledgeReferenceStateInvalid,
    }
}

fn duplicate_actor_knowledge() -> LocalDbError {
    LocalDbError::ActorContractConflict {
        code: ActorContractConflict::DuplicateActorKnowledge,
    }
}

fn is_live_kb_status(status: &str) -> bool {
    !matches!(status, "deleted" | "merged" | "deprecated")
}

fn map_kb_actor_constraint<T>(result: Result<T, sqlx::Error>) -> Result<T, LocalDbError> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let sqlx::Error::Database(db) = &err {
                if db.is_unique_violation() {
                    let constraint = db.constraint().unwrap_or_default();
                    let message = db.message();
                    if constraint.contains("idx_kb_key_blocks_character_active_unique")
                        || constraint.contains("idx_kb_key_blocks_binding_active_unique")
                        || message.contains("idx_kb_key_blocks_character_active_unique")
                        || message.contains("idx_kb_key_blocks_binding_active_unique")
                        || message.contains(
                            "kb_key_blocks.character_id, kb_key_blocks.block_type, kb_key_blocks.canonical_name",
                        )
                        || message.contains(
                            "kb_key_blocks.actor_world_binding_id, kb_key_blocks.block_type, kb_key_blocks.canonical_name",
                        )
                    {
                        return Err(duplicate_actor_knowledge());
                    }
                }
            }
            Err(LocalDbError::from(err))
        }
    }
}

fn validate_summary_utf8_bytes(summary: &str) -> Result<(), LocalDbError> {
    if summary.len() > ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES {
        return Err(LocalDbError::ValidationError(format!(
            "summary must be at most {} UTF-8 bytes",
            ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES
        )));
    }
    Ok(())
}

fn module_value_nonempty(value: &serde_json::Value) -> bool {
    !value.is_null()
        && !(value.is_object() && value.as_object().is_some_and(|o| o.is_empty()))
        && !(value.is_array() && value.as_array().is_some_and(|a| a.is_empty()))
}

fn json_modules_has_authoritative_mind(modules_json: Option<&str>) -> Result<bool, LocalDbError> {
    let Some(raw) = modules_json else {
        return Ok(false);
    };
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| knowledge_reference_state_invalid())?;
    let obj = value
        .as_object()
        .ok_or_else(|| knowledge_reference_state_invalid())?;
    let mental_nonempty = obj.get("mental").is_some_and(module_value_nonempty);
    let belief_nonempty = obj.get("belief").is_some_and(module_value_nonempty);
    Ok(mental_nonempty || belief_nonempty)
}


fn json_value_contains_exact_id_scalar(value: &serde_json::Value, entry_id: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s == entry_id,
        serde_json::Value::Array(items) => items
            .iter()
            .any(|v| json_value_contains_exact_id_scalar(v, entry_id)),
        serde_json::Value::Object(map) => map
            .values()
            .any(|v| json_value_contains_exact_id_scalar(v, entry_id)),
        _ => false,
    }
}

fn parse_body_object(
    raw: Option<&str>,
) -> Result<serde_json::Map<String, serde_json::Value>, LocalDbError> {
    match raw {
        None => Ok(serde_json::Map::new()),
        Some(text) => {
            let value: serde_json::Value =
                serde_json::from_str(text).map_err(|_| knowledge_entry_not_mutable())?;
            value
                .as_object()
                .cloned()
                .ok_or_else(|| knowledge_entry_not_mutable())
        }
    }
}

fn current_summary_from_body(raw: Option<&str>) -> Result<Option<String>, LocalDbError> {
    let obj = parse_body_object(raw)?;
    match obj.get("summary") {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(serde_json::Value::Null) => Ok(None),
        _ => Err(knowledge_entry_not_mutable()),
    }
}

fn apply_summary_patch_to_body_json(
    raw: Option<&str>,
    patch: FieldPatch<&str>,
) -> Result<Option<String>, LocalDbError> {
    let mut obj = parse_body_object(raw)?;
    match patch {
        FieldPatch::Keep => return Ok(raw.map(str::to_string)),
        FieldPatch::Clear => {
            obj.remove("summary");
        }
        FieldPatch::Set(summary) => {
            validate_summary_utf8_bytes(summary)?;
            obj.insert(
                "summary".into(),
                serde_json::Value::String(summary.to_string()),
            );
        }
    }
    if obj.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::Value::Object(obj).to_string()))
}

fn actor_knowledge_patch_is_no_op(
    row: &KeyBlockRow,
    patch: ActorKnowledgePatch<'_>,
) -> Result<bool, LocalDbError> {
    if let Some(name) = patch.canonical_name {
        if name != row.canonical_name {
            return Ok(false);
        }
    }
    match patch.summary {
        FieldPatch::Keep => {}
        FieldPatch::Clear => {
            if current_summary_from_body(row.body_json.as_deref())?.is_some() {
                return Ok(false);
            }
        }
        FieldPatch::Set(summary) => {
            let current = current_summary_from_body(row.body_json.as_deref())?;
            if current.as_deref() != Some(summary) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn json_value_has_exact_kb_entry_id_scalar(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => s.starts_with("kb_") && s.len() > 10,
        serde_json::Value::Array(items) => items
            .iter()
            .any(json_value_has_exact_kb_entry_id_scalar),
        serde_json::Value::Object(map) => map
            .values()
            .any(json_value_has_exact_kb_entry_id_scalar),
        _ => false,
    }
}

fn assert_target_row_module_referents(row: &KeyBlockRow, entry_id: &str) -> Result<(), LocalDbError> {
    let Some(raw) = row.modules_json.as_deref() else {
        return Ok(());
    };
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| knowledge_reference_state_invalid())?;
    if !value.is_object() {
        return Err(knowledge_reference_state_invalid());
    }
    if json_modules_has_authoritative_mind(Some(raw))? {
        return Err(knowledge_entry_in_use());
    }
    let obj = value.as_object().expect("object checked");
    for key in ["self", "observation", "other", "mental", "belief"] {
        if let Some(slot) = obj.get(key) {
            if json_value_has_exact_kb_entry_id_scalar(slot) {
                return Err(knowledge_entry_in_use());
            }
        }
    }
    if json_value_contains_exact_id_scalar(&value, entry_id) {
        return Err(knowledge_entry_in_use());
    }
    Ok(())
}

async fn load_key_block_row_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry_id: &str,
) -> Result<Option<KeyBlockRow>, LocalDbError> {
    let row = sqlx::query_as::<_, KeyBlockRow>(
        r"SELECT
            key_block_id, owner_kind, world_id, character_id,
            actor_world_binding_id, creator_only,
            block_type, canonical_name, status,
            revision, body_json, source_anchor_json, created_from_command_id,
            created_at, updated_at, source_work_id, source_chapter,
            source_provenance_kind, extensions_nexus_json, modules_json
        FROM kb_key_blocks
        WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row)
}

async fn row_in_character_read_scope_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: &KeyBlockRow,
    character_id: &str,
) -> Result<bool, LocalDbError> {
    match row.owner_kind.as_str() {
        "character" => Ok(row.character_id.as_deref() == Some(character_id)),
        "actor_world_binding" => {
            let binding_id = row.actor_world_binding_id.as_deref().ok_or_else(|| {
                LocalDbError::Sqlx(sqlx::Error::Protocol(
                    "malformed binding-owned knowledge row".into(),
                ))
            })?;
            let owner_character: Option<String> = sqlx::query_scalar(
                "SELECT character_id FROM actor_world_bindings WHERE binding_id = ?",
            )
            .bind(binding_id)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(owner_character.as_deref() == Some(character_id))
        }
        _ => Ok(false),
    }
}

/// Retained-read scope: Character-owned row or binding-owned row with P0 world provenance.
async fn row_in_character_retained_read_scope(
    pool: &SqlitePool,
    owner_creator_id: &str,
    row: &KeyBlockRow,
    character_id: &str,
) -> Result<bool, LocalDbError> {
    match row.owner_kind.as_str() {
        "character" => Ok(row.character_id.as_deref() == Some(character_id)),
        "actor_world_binding" => {
            let Some(binding_id) = row.actor_world_binding_id.as_deref() else {
                return Ok(false);
            };
            match crate::actor_world_binding::require_owned_binding_provenance_pool(
                pool,
                owner_creator_id,
                character_id,
                binding_id,
            )
            .await
            {
                Ok(()) => Ok(true),
                Err(LocalDbError::ActorNotFound { .. }) => Ok(false),
                Err(err) => Err(err),
            }
        }
        _ => Ok(false),
    }
}

async fn character_owned_for_read(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
) -> Result<bool, LocalDbError> {
    let owned: Option<String> = sqlx::query_scalar(
        "SELECT owner_creator_id FROM characters WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_optional(pool)
    .await?;
    Ok(matches!(owned.as_deref(), Some(stored) if stored == owner_creator_id))
}

/// Stored-owner scoped knowledge detail read (retained archive reads allowed).
pub async fn get_actor_knowledge_entry(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    entry_id: &str,
) -> Result<Option<KnowledgeEntryRecord>, LocalDbError> {
    if !character_owned_for_read(pool, owner_creator_id, character_id).await? {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, KeyBlockRow>(
        r"SELECT
            key_block_id, owner_kind, world_id, character_id,
            actor_world_binding_id, creator_only,
            block_type, canonical_name, status,
            revision, body_json, source_anchor_json, created_from_command_id,
            created_at, updated_at, source_work_id, source_chapter,
            source_provenance_kind, extensions_nexus_json, modules_json
        FROM kb_key_blocks
        WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if !row_in_character_retained_read_scope(pool, owner_creator_id, &row, character_id)
        .await?
    {
        return Ok(None);
    }
    row.to_record().map(Some).map_err(map_kb_store_to_local_db)
}

async fn prepare_actor_knowledge_write_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    owner_creator_id: &str,
    character_id: &str,
    entry_id: &str,
    expected_revision: i64,
) -> Result<KeyBlockRow, LocalDbError> {
    check_expected_revision(expected_revision)?;
    require_active_owned_character_tx(tx, owner_creator_id, character_id).await?;
    let row = load_key_block_row_tx(tx, entry_id)
        .await?
        .ok_or_else(|| LocalDbError::ActorNotFound {
            resource: "knowledge_entry",
            id: entry_id.to_string(),
        })?;
    if !row_in_character_read_scope_tx(tx, &row, character_id).await? {
        return Err(LocalDbError::ActorNotFound {
            resource: "knowledge_entry",
            id: entry_id.to_string(),
        });
    }
    if row.owner_kind == "actor_world_binding" {
        crate::actor_world_binding::require_valid_provenance_tx(
            tx,
            owner_creator_id,
            character_id,
            row.actor_world_binding_id.as_deref(),
        )
        .await?;
    }
    if !is_live_kb_status(&row.status) {
        return Err(knowledge_entry_not_mutable());
    }
    let stored_revision = row.revision.unwrap_or(0);
    if stored_revision != expected_revision {
        return Err(knowledge_revision_conflict());
    }
    Ok(row)
}

async fn assert_no_protected_referents_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry_id: &str,
    row: &KeyBlockRow,
) -> Result<(), LocalDbError> {
    let other_modules_invalid: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_key_blocks
            WHERE key_block_id != ?
              AND modules_json IS NOT NULL
              AND json_valid(modules_json) = 0
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let timeline_modules_invalid: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM narrative_timeline_events
            WHERE modules_json IS NOT NULL
              AND json_valid(modules_json) = 0
        ) as "exists!""#
    )
    .fetch_one(&mut **tx)
    .await?;

    let timeline_participants_invalid: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM narrative_timeline_events
            WHERE affected_key_block_ids_json IS NOT NULL
              AND (
                json_valid(affected_key_block_ids_json) = 0
                OR json_type(affected_key_block_ids_json) != 'array'
              )
        ) as "exists!""#
    )
    .fetch_one(&mut **tx)
    .await?;

    if other_modules_invalid != 0
        || timeline_modules_invalid != 0
        || timeline_participants_invalid != 0
    {
        return Err(knowledge_reference_state_invalid());
    }

    let world_sheet: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM actor_world_bindings WHERE world_sheet_entry_id = ?
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let anchors: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_source_anchors WHERE key_block_id = ?
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let relationships: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_relationships
            WHERE source_entity_id = ? OR target_entity_id = ?
        ) as "exists!""#,
        entry_id,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let mind_states: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM mind_states WHERE holder_entry_id = ?
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let other_modules_ref: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM kb_key_blocks
            WHERE key_block_id != ?
              AND modules_json IS NOT NULL
              AND json_valid(modules_json) = 1
              AND EXISTS (
                SELECT 1 FROM json_tree(modules_json)
                WHERE type = 'text' AND value = ?
              )
        ) as "exists!""#,
        entry_id,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let timeline_modules_ref: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM narrative_timeline_events
            WHERE modules_json IS NOT NULL
              AND json_valid(modules_json) = 1
              AND EXISTS (
                SELECT 1 FROM json_tree(modules_json)
                WHERE type = 'text' AND value = ?
              )
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let timeline_participants_ref: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM narrative_timeline_events
            WHERE affected_key_block_ids_json IS NOT NULL
              AND json_valid(affected_key_block_ids_json) = 1
              AND json_type(affected_key_block_ids_json) = 'array'
              AND EXISTS (
                SELECT 1 FROM json_each(affected_key_block_ids_json)
                WHERE value = ?
              )
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let findings: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM world_findings WHERE target_entry_id = ?
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    let compute_sessions: i64 = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM compute_sessions WHERE entry_id = ?
        ) as "exists!""#,
        entry_id
    )
    .fetch_one(&mut **tx)
    .await?;

    if world_sheet != 0
        || anchors != 0
        || relationships != 0
        || mind_states != 0
        || other_modules_ref != 0
        || timeline_modules_ref != 0
        || timeline_participants_ref != 0
        || findings != 0
        || compute_sessions != 0
    {
        return Err(knowledge_entry_in_use());
    }

    assert_target_row_module_referents(row, entry_id)?;
    Ok(())
}

pub async fn update_actor_knowledge_entry(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    entry_id: &str,
    expected_revision: i64,
    patch: ActorKnowledgePatch<'_>,
) -> Result<KnowledgeEntryRecord, LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        let row = prepare_actor_knowledge_write_tx(
            &mut tx,
            owner_creator_id,
            character_id,
            entry_id,
            expected_revision,
        )
        .await?;
        if actor_knowledge_patch_is_no_op(&row, patch)? {
            return row.to_record().map_err(map_kb_store_to_local_db);
        }

        let canonical_name = if let Some(name) = patch.canonical_name {
            validate_canonical_name(name)
                .map_err(|e| LocalDbError::ValidationError(e.to_string()))?;
            name.to_string()
        } else {
            row.canonical_name.clone()
        };

        let body_json = match patch.summary {
            FieldPatch::Keep => row.body_json.clone(),
            _ => apply_summary_patch_to_body_json(row.body_json.as_deref(), patch.summary)?,
        };

        let now = chrono::Utc::now().to_rfc3339();
        let new_revision = expected_revision + 1;
        let updated = map_kb_actor_constraint(
            sqlx::query(
                r"UPDATE kb_key_blocks
                   SET canonical_name = ?, body_json = ?, revision = ?, updated_at = ?
                   WHERE key_block_id = ?
                     AND COALESCE(revision, 0) = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       (owner_kind = 'character' AND character_id = ?)
                       OR (
                         owner_kind = 'actor_world_binding'
                         AND actor_world_binding_id IN (
                           SELECT binding_id FROM actor_world_bindings
                           WHERE character_id = ? AND status = 'active'
                         )
                       )
                     )",
            )
            .bind(&canonical_name)
            .bind(&body_json)
            .bind(new_revision)
            .bind(&now)
            .bind(entry_id)
            .bind(expected_revision)
            .bind(character_id)
            .bind(character_id)
            .execute(&mut *tx)
            .await,
        )?
        .rows_affected();
        if updated == 0 {
            return Err(knowledge_revision_conflict());
        }
        load_key_block_row_tx(&mut tx, entry_id)
            .await?
            .ok_or_else(|| LocalDbError::ActorNotFound {
                resource: "knowledge_entry",
                id: entry_id.to_string(),
            })?
            .to_record()
            .map_err(map_kb_store_to_local_db)
    }
    .await;
    match result {
        Ok(record) => {
            tx.commit().await?;
            Ok(record)
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

pub async fn delete_actor_knowledge_entry(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    entry_id: &str,
    expected_revision: i64,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        let row = prepare_actor_knowledge_write_tx(
            &mut tx,
            owner_creator_id,
            character_id,
            entry_id,
            expected_revision,
        )
        .await?;
        assert_no_protected_referents_tx(&mut tx, entry_id, &row).await?;
        let deleted = map_kb_actor_constraint(
            sqlx::query(
                r"DELETE FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND COALESCE(revision, 0) = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       (owner_kind = 'character' AND character_id = ?)
                       OR (
                         owner_kind = 'actor_world_binding'
                         AND actor_world_binding_id IN (
                           SELECT binding_id FROM actor_world_bindings
                           WHERE character_id = ? AND status = 'active'
                         )
                       )
                     )",
            )
            .bind(entry_id)
            .bind(expected_revision)
            .bind(character_id)
            .bind(character_id)
            .execute(&mut *tx)
            .await,
        )?
        .rows_affected();
        if deleted == 0 {
            return Err(knowledge_revision_conflict());
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            tx.commit().await?;
            Ok(())
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}
