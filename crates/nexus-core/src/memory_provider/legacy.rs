//! Provider-seam legacy read bridge (plan `v228-n2-nxm5-coreservice-reroute`,
//! Task 1).
//!
//! This is the **single** module that renders the legacy memory surfaces — the
//! Creator/Character bearer tables and the bearer files on disk — as
//! provider-native rows, so one [`MemoryStore`](super::storage::MemoryStore) read
//! path serves both the provider's own content and content that has not been
//! migrated yet (iteration compass D6: legacy tables/files stay write-idle).
//!
//! **Read-only.** Every statement here is a `SELECT` executed on the caller's
//! read transaction; no legacy table, file or schema is written or created. That
//! is what keeps a legacy surface byte-identical across a bridge read, and it is
//! why a projected row can never be updated or deleted through the provider: the
//! write paths resolve ids in the provider's own tables only.
//!
//! **Deterministic identity.** A projected row carries an external id derived
//! from `(space, source family, source primary key)` — never a fresh UUID and
//! never insertion order — so repeated reads and the P5 migration pass agree on
//! the identity of the same legacy object. When a provider-native row already
//! carries a projected id (a migrated object), the composing read path keeps the
//! provider's row and drops the projection, so the merged view never duplicates.
//!
//! **Owner selection.** A subject opts into projections by naming its legacy
//! bearer in its provider-native kind ([`CREATOR_KIND_NS`] /
//! [`CHARACTER_KIND_NS`]); every other subject projects nothing. Character rows
//! and files are additionally re-checked against the workspace's owning Creator,
//! because the legacy character tables are keyed by `character_id` alone.

use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{json, Value};
use sqlx::Sqlite;

use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::memory_io;
use nexus_creator_memory::MemoryBearerRef;

use super::policy;
use super::storage::{
    timestamp, CandidateRow, Placement, ProfileRow, RecordRow, Selection, StoreError, StoreResult,
    SubjectRow, SUBJECT_COLUMNS,
};

/// Subject namespace of the workspace's Creator bearer (`kind.ns`, an
/// `ExtensionMap` key: lowercase, `_`/`-` only).
pub const CREATOR_KIND_NS: &str = "nexus_creator";

/// Subject namespace of a Character bearer (its `kind_id` is the character id).
pub const CHARACTER_KIND_NS: &str = "nexus_character";

/// `provenance.captured_by` of every projected row: no authenticated principal
/// wrote it, the bridge projected it from the legacy bearer.
const LEGACY_CAPTURED_BY: &str = "nexus:legacy";

/// `profile_kind` of `SOUL.md` (§4.2's one required authored kind).
const SOUL_PROFILE_KIND: &str = "personality";

/// Record kind of a cached SOUL narrative: a derived cache, never one of the
/// nine authored kinds, so it sits beside the `nexus.` taxonomy under its own
/// name.
const SOUL_NARRATIVE_KIND: &str = "nexus.soul_narrative";

/// §6.7 advisory disposition recorded for a legacy pending-review row. The
/// provider never classified it; `fragment` is the shape the review pipeline
/// consumes and the `rationale_codes` entry names the legacy origin.
const PENDING_REVIEW_DISPOSITION: &str = "fragment";

/// The legacy source families. These names are part of the external-id contract
/// (P5's migration key is `(space, source family, source primary key)`), so they
/// name both the projected family and the id segment.
const FAMILY_MEMORY_PENDING_REVIEW: &str = "memory_pending_review";
const FAMILY_CHARACTER_PENDING_REVIEW: &str = "character_memory_pending_review";
const FAMILY_MEMORY_FRAGMENTS: &str = "memory_fragments";
const FAMILY_CHARACTER_FRAGMENTS: &str = "character_memory_fragments";
const FAMILY_MEMORY_NARRATIVES: &str = "memory_soul_narratives";
const FAMILY_CHARACTER_NARRATIVES: &str = "character_soul_narratives";
const FAMILY_MEMORY_LONG_TERM: &str = "memory_long_term";
const FAMILY_CHARACTER_LONG_TERM: &str = "character_long_term";
const FAMILY_RUN_CAPTURES: &str = "character_run_captures";

/// Record kind shared by a legacy fragment and by a promoted long-term memory of
/// the `custom` kind is *not* collapsed: fragments use `fact`, long-term files
/// keep their authored kind through [`policy::namespaced_record_kind`].
const FACT_KIND: &str = "fact";
/// Record kind of a `character_run_captures` provenance row.
const EVENT_KIND: &str = "event";
/// The external-id prefix. Provider-minted ids use `rec_`/`cnd_`, so a projected
/// id is distinguishable at a glance and can never collide with one.
const ID_PREFIX: &str = "lgc";
/// Encoded representation of an absent composite key component (`world_id` /
/// `actor_world_binding_id` is `NULL`).
const NULL_COMPONENT: &str = "\0";
/// Timestamp used when a bearer file's mtime cannot be read: a fixed epoch keeps
/// the projection deterministic instead of stamping the read time.
const EPOCH_TIMESTAMP: &str = "1970-01-01T00:00:00Z";

/// The legacy bearer roots of one opened workspace: the user home the bearer
/// layout resolves from, and the Creator that owns it.
#[derive(Debug, Clone)]
pub(super) struct LegacyRoots {
    home: PathBuf,
    creator_id: String,
}

impl LegacyRoots {
    pub(super) fn new(home: PathBuf, creator_id: impl Into<String>) -> Self {
        Self {
            home,
            creator_id: creator_id.into(),
        }
    }

    fn bearer<'a>(&'a self, owner: &'a Owner) -> MemoryBearerRef<'a> {
        match owner {
            Owner::Creator => MemoryBearerRef::Creator(&self.creator_id),
            Owner::Character(character_id) => MemoryBearerRef::Character {
                owner_creator_id: &self.creator_id,
                character_id,
            },
        }
    }
}

/// The legacy bearer a provider subject projects.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Owner {
    Creator,
    Character(String),
}

impl Owner {
    const fn namespace(&self) -> &'static str {
        match self {
            Self::Creator => CREATOR_KIND_NS,
            Self::Character(_) => CHARACTER_KIND_NS,
        }
    }

    fn kind_id<'a>(&'a self, roots: &'a LegacyRoots) -> &'a str {
        match self {
            Self::Creator => &roots.creator_id,
            Self::Character(character_id) => character_id,
        }
    }
}

// ── Read-path entry points ───────────────────────────────────────────────

/// The subject's projected records, filtered by exactly the §6.13/§7.20
/// predicate the provider's own page uses ([`Selection`], provenance filter,
/// kind filter). Tombstoned rows are not a legacy state: a legacy object is
/// either present or absent.
pub(super) async fn records_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject: Option<&SubjectRow>,
    selection: &Selection,
    filter: Option<&[String]>,
    kinds: &[String],
) -> StoreResult<Vec<RecordRow>> {
    let Some(owner) = owner_of(tx, roots, subject).await? else {
        return Ok(Vec::new());
    };
    let subject_id = subject.map_or("", |subject| subject.subject_id.as_str());
    let mut rows = project_records(tx, roots, space_id, subject_id, &owner).await?;
    rows.retain(|row| {
        selection_admits(selection, row)
            && provenance_admits(filter, row)
            && kind_admits(kinds, row)
    });
    Ok(rows)
}

/// One projected record by its external id. The id names the source family, so
/// the owner and its provider subject are resolved before the projection runs;
/// an id that does not belong to this workspace (or to any legacy source) is
/// absent, never a disclosure of another bearer.
pub(super) async fn record_by_id_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    record_id: &str,
) -> StoreResult<Option<RecordRow>> {
    let Some((family, components)) = parse_id(record_id) else {
        return Ok(None);
    };
    let Some(owner) = id_owner(tx, roots, &family, &components).await? else {
        return Ok(None);
    };
    let Some(subject) = subject_of(tx, roots, space_id, &owner).await? else {
        return Ok(None);
    };
    let rows = project_records(tx, roots, space_id, &subject.subject_id, &owner).await?;
    Ok(rows.into_iter().find(|row| row.record_id == record_id))
}

/// The subject's projected pending-review rows, filtered by §6.7's status
/// predicate. A legacy queue entry is always `pending` and never expires, so a
/// non-pending request admits none of them.
pub(super) async fn candidates_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject: Option<&SubjectRow>,
    status: Option<&str>,
) -> StoreResult<Vec<CandidateRow>> {
    if status.is_some_and(|status| status != "pending") {
        return Ok(Vec::new());
    }
    let Some(owner) = owner_of(tx, roots, subject).await? else {
        return Ok(Vec::new());
    };
    let subject_id = subject.map_or("", |subject| subject.subject_id.as_str());
    candidates_for_owner(tx, roots, space_id, subject_id, &owner).await
}

/// One projected candidate by its external id (see [`record_by_id_in_tx`]).
pub(super) async fn candidate_by_id_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    candidate_id: &str,
) -> StoreResult<Option<CandidateRow>> {
    let Some((family, components)) = parse_id(candidate_id) else {
        return Ok(None);
    };
    let Some(owner) = id_owner(tx, roots, &family, &components).await? else {
        return Ok(None);
    };
    let Some(subject) = subject_of(tx, roots, space_id, &owner).await? else {
        return Ok(None);
    };
    let rows = candidates_for_owner(tx, roots, space_id, &subject.subject_id, &owner).await?;
    Ok(rows
        .into_iter()
        .find(|row| row.candidate_id == candidate_id))
}

/// The subject's projected profiles. `SOUL.md` is one authored document (§6.8),
/// so it projects as the `personality` profile and nothing else — a narrative or
/// cache document is never fabricated as an authored profile.
pub(super) async fn profiles_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject: Option<&SubjectRow>,
) -> StoreResult<Vec<ProfileRow>> {
    let Some(owner) = owner_of(tx, roots, subject).await? else {
        return Ok(Vec::new());
    };
    let subject_id = subject.map_or("", |subject| subject.subject_id.as_str());
    let Some(body) = soul_document(roots, &owner)? else {
        return Ok(Vec::new());
    };
    let document = json!({ "format": "markdown", "body": body });
    Ok(vec![ProfileRow {
        space_id: space_id.to_string(),
        subject_id: subject_id.to_string(),
        profile_kind: SOUL_PROFILE_KIND.to_string(),
        document_json: serialize(&document, "legacy profile document")?,
        updated_by: LEGACY_CAPTURED_BY.to_string(),
        revision: 1,
        created_at: soul_timestamp(roots, &owner),
        updated_at: soul_timestamp(roots, &owner),
    }])
}

// ── Owner resolution ────────────────────────────────────────────────────

/// Which legacy bearer this provider subject stands for, if any.
async fn owner_of(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    subject: Option<&SubjectRow>,
) -> StoreResult<Option<Owner>> {
    let Some(subject) = subject else {
        return Ok(None);
    };
    let (Some(namespace), Some(kind_id)) = (subject.kind_ns.as_deref(), subject.kind_id.as_deref())
    else {
        return Ok(None);
    };
    match namespace {
        CREATOR_KIND_NS if kind_id == roots.creator_id => Ok(Some(Owner::Creator)),
        CHARACTER_KIND_NS if character_owned(tx, roots, kind_id).await? => {
            Ok(Some(Owner::Character(kind_id.to_string())))
        }
        _ => Ok(None),
    }
}

/// The owner an external id names, or `None` when the id is foreign to this
/// workspace. Character families whose primary key does not carry the character
/// id resolve it from the source row itself.
async fn id_owner(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    family: &str,
    components: &[String],
) -> StoreResult<Option<Owner>> {
    match family {
        FAMILY_MEMORY_PENDING_REVIEW | FAMILY_MEMORY_FRAGMENTS | FAMILY_MEMORY_NARRATIVES => {
            Ok(Some(Owner::Creator))
        }
        FAMILY_MEMORY_LONG_TERM => Ok(components
            .first()
            .filter(|creator_id| *creator_id == &roots.creator_id)
            .map(|_| Owner::Creator)),
        FAMILY_CHARACTER_LONG_TERM | FAMILY_CHARACTER_NARRATIVES => {
            let Some(character_id) = components.first() else {
                return Ok(None);
            };
            Ok(character_owned(tx, roots, character_id)
                .await?
                .then(|| Owner::Character(character_id.clone())))
        }
        FAMILY_CHARACTER_FRAGMENTS => {
            character_owner_of(
                tx,
                roots,
                "character_memory_fragments",
                "fragment_id",
                components,
            )
            .await
        }
        FAMILY_RUN_CAPTURES => {
            character_owner_of(
                tx,
                roots,
                "character_run_captures",
                "operation_id",
                components,
            )
            .await
        }
        FAMILY_CHARACTER_PENDING_REVIEW => {
            character_owner_of(
                tx,
                roots,
                "character_memory_pending_review",
                "pending_id",
                components,
            )
            .await
        }
        _ => Ok(None),
    }
}

/// Resolve the owning Character of a row addressed by its own primary key, then
/// re-check it against this workspace's Creator. `table`/`column` are literals
/// from the fixed family table above, never caller text.
async fn character_owner_of(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    table: &str,
    column: &str,
    components: &[String],
) -> StoreResult<Option<Owner>> {
    let Some(primary_key) = components.first() else {
        return Ok(None);
    };
    let sql = format!("SELECT character_id FROM {table} WHERE {column} = ?");
    let character_id: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(primary_key)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(character_id) = character_id else {
        return Ok(None);
    };
    Ok(character_owned(tx, roots, &character_id)
        .await?
        .then_some(Owner::Character(character_id)))
}

/// The legacy character tables are keyed by `character_id` alone, so ownership
/// is re-checked here: a character of another Creator never projects.
async fn character_owned(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    character_id: &str,
) -> StoreResult<bool> {
    let owned: Option<String> = sqlx::query_scalar(
        "SELECT character_id FROM characters WHERE character_id = ? AND owner_creator_id = ?",
    )
    .bind(character_id)
    .bind(&roots.creator_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(owned.is_some())
}

/// The provider subject standing for a legacy owner, if the caller has ensured
/// one. A projected row names its subject's own id, so grants and coverage keep
/// applying to exactly the row a merged read returns.
async fn subject_of(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    owner: &Owner,
) -> StoreResult<Option<SubjectRow>> {
    let sql = format!(
        "SELECT {SUBJECT_COLUMNS} FROM memory_subjects \
         WHERE space_id = ? AND kind_ns = ? AND kind_id = ? ORDER BY subject_id LIMIT 1"
    );
    sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(owner.namespace())
        .bind(owner.kind_id(roots))
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::from)
}

// ── Predicate parity with the provider's own page ───────────────────────

/// The Rust rendering of [`super::storage`]'s `record_predicate` selection
/// term, so a projected row is admitted or excluded by exactly the rule the
/// SQL page applies to a provider row.
fn selection_admits(selection: &Selection, row: &RecordRow) -> bool {
    let shared = row.scope_visibility == Placement::SHARED;
    match selection {
        Selection::SubjectAll => true,
        Selection::SharedOnly => shared,
        Selection::SelectedContexts {
            context_ids,
            include_shared,
        } => {
            if context_ids.is_empty() {
                return *include_shared && shared;
            }
            (*include_shared && shared)
                || row
                    .scope_context_id
                    .as_deref()
                    .is_some_and(|context| context_ids.iter().any(|named| named == context))
        }
    }
}

/// The `source_context_ids` filter term of `record_predicate`.
fn provenance_admits(filter: Option<&[String]>, row: &RecordRow) -> bool {
    let Some(context_ids) = filter else {
        return true;
    };
    if context_ids.is_empty() {
        return false;
    }
    serde_json::from_str::<Value>(&row.provenance_json).is_ok_and(|provenance| {
        provenance["source_context_ids"]
            .as_array()
            .is_some_and(|sources| {
                sources.iter().any(|source| {
                    source
                        .as_str()
                        .is_some_and(|source| context_ids.iter().any(|named| named == source))
                })
            })
    })
}

/// The `kind IN (...)` term of `record_predicate`.
fn kind_admits(kinds: &[String], row: &RecordRow) -> bool {
    kinds.is_empty() || kinds.iter().any(|kind| kind == &row.kind)
}

// ── Record projections ──────────────────────────────────────────────────

async fn project_records(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
    owner: &Owner,
) -> StoreResult<Vec<RecordRow>> {
    let mut rows = match owner {
        Owner::Creator => {
            let mut rows = creator_fragments(tx, roots, space_id, subject_id).await?;
            rows.extend(creator_narratives(tx, roots, space_id, subject_id).await?);
            rows
        }
        Owner::Character(character_id) => {
            let mut rows = character_fragments(tx, space_id, subject_id, character_id).await?;
            rows.extend(character_narratives(tx, space_id, subject_id, character_id).await?);
            rows.extend(character_run_captures(tx, space_id, subject_id, character_id).await?);
            rows
        }
    };
    rows.extend(long_term_records(roots, space_id, subject_id, owner)?);
    Ok(rows)
}

#[derive(sqlx::FromRow)]
struct CreatorFragmentRow {
    fragment_id: String,
    session_id: String,
    keywords: String,
    summary: String,
    created_at: String,
    ttl: Option<String>,
    world_id: Option<String>,
}

async fn creator_fragments(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Vec<RecordRow>> {
    let rows = sqlx::query_as::<_, CreatorFragmentRow>(
        "SELECT fragment_id, session_id, keywords, summary, created_at, ttl, world_id \
         FROM memory_fragments WHERE creator_id = ? ORDER BY created_at DESC, fragment_id",
    )
    .bind(&roots.creator_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let CreatorFragmentRow {
                fragment_id,
                session_id,
                keywords,
                summary,
                created_at,
                ttl,
                world_id,
            } = row;
            let created_at = normalize_timestamp(&created_at);
            let record_id = external_id(FAMILY_MEMORY_FRAGMENTS, &[fragment_id.as_str()]);
            let content = json!({
                "text": summary,
                "structured": { "nexus_legacy": {
                    "family": FAMILY_MEMORY_FRAGMENTS,
                    "fragment_id": fragment_id,
                    "session_id": session_id,
                    "keywords": keywords_json(&keywords),
                    "ttl": ttl,
                    "world_id": world_id,
                }},
            });
            record_row(
                space_id,
                subject_id,
                RecordDraft {
                    record_id,
                    kind: FACT_KIND.to_string(),
                    content,
                    visibility: Placement::SHARED,
                    context_id: None,
                    source_context_ids: world_id.into_iter().collect(),
                    occurred_at: None,
                    created_at: created_at.clone(),
                    updated_at: created_at,
                    revision: 1,
                },
            )
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct CharacterFragmentRow {
    fragment_id: String,
    session_id: String,
    actor_world_binding_id: Option<String>,
    keywords: String,
    summary: String,
    created_at: String,
    ttl: Option<String>,
    revision: i64,
}

async fn character_fragments(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
    character_id: &str,
) -> StoreResult<Vec<RecordRow>> {
    let rows = sqlx::query_as::<_, CharacterFragmentRow>(
        "SELECT fragment_id, session_id, actor_world_binding_id, keywords, summary, \
         created_at, ttl, revision FROM character_memory_fragments \
         WHERE character_id = ? ORDER BY created_at DESC, fragment_id",
    )
    .bind(character_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let CharacterFragmentRow {
                fragment_id,
                session_id,
                actor_world_binding_id,
                keywords,
                summary,
                created_at,
                ttl,
                revision,
            } = row;
            let created_at = normalize_timestamp(&created_at);
            let record_id = external_id(FAMILY_CHARACTER_FRAGMENTS, &[fragment_id.as_str()]);
            let visibility = binding_visibility(actor_world_binding_id.as_deref());
            let context_id = actor_world_binding_id.clone();
            let content = json!({
                "text": summary,
                "structured": { "nexus_legacy": {
                    "family": FAMILY_CHARACTER_FRAGMENTS,
                    "fragment_id": fragment_id,
                    "session_id": session_id,
                    "actor_world_binding_id": actor_world_binding_id,
                    "keywords": keywords_json(&keywords),
                    "ttl": ttl,
                }},
            });
            record_row(
                space_id,
                subject_id,
                RecordDraft {
                    record_id,
                    kind: FACT_KIND.to_string(),
                    content,
                    visibility,
                    source_context_ids: context_id.clone().into_iter().collect(),
                    context_id,
                    occurred_at: None,
                    created_at: created_at.clone(),
                    updated_at: created_at,
                    // Legacy revision 0 is the provider's revision 1, so the
                    // 0→1 promotion step the DTOs expose survives the bridge.
                    revision: revision.saturating_add(1),
                },
            )
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct NarrativeRow {
    scope_id: Option<String>,
    narrative: Option<String>,
    generated_at: Option<String>,
    fragment_count_at_generation: i64,
    max_fragment_created_at_at_generation: Option<String>,
    distinct_keyword_count_cache: i64,
    stats_fingerprint: Option<String>,
    created_at: String,
    updated_at: String,
}

async fn creator_narratives(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Vec<RecordRow>> {
    let rows = sqlx::query_as::<_, NarrativeRow>(
        "SELECT world_id AS scope_id, narrative, generated_at, fragment_count_at_generation, \
         max_fragment_created_at_at_generation, distinct_keyword_count_cache, stats_fingerprint, \
         created_at, updated_at FROM memory_soul_narratives \
         WHERE creator_id = ? ORDER BY created_at DESC, world_id",
    )
    .bind(&roots.creator_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let scope_id = row.scope_id.clone();
            narrative_row(
                space_id,
                subject_id,
                FAMILY_MEMORY_NARRATIVES,
                &[scope_id.as_deref().unwrap_or(NULL_COMPONENT)],
                row,
                Placement::SHARED,
                scope_id.clone().into_iter().collect(),
            )
        })
        .collect()
}

async fn character_narratives(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
    character_id: &str,
) -> StoreResult<Vec<RecordRow>> {
    let rows = sqlx::query_as::<_, NarrativeRow>(
        "SELECT actor_world_binding_id AS scope_id, narrative, generated_at, \
         fragment_count_at_generation, max_fragment_created_at_at_generation, \
         distinct_keyword_count_cache, stats_fingerprint, created_at, updated_at \
         FROM character_soul_narratives WHERE character_id = ? \
         ORDER BY created_at DESC, actor_world_binding_id",
    )
    .bind(character_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let scope_id = row.scope_id.clone();
            narrative_row(
                space_id,
                subject_id,
                FAMILY_CHARACTER_NARRATIVES,
                &[character_id, scope_id.as_deref().unwrap_or(NULL_COMPONENT)],
                row,
                binding_visibility(scope_id.as_deref()),
                scope_id.clone().into_iter().collect(),
            )
        })
        .collect()
}

fn narrative_row(
    space_id: &str,
    subject_id: &str,
    family: &str,
    key_components: &[&str],
    row: NarrativeRow,
    visibility: &'static str,
    source_context_ids: Vec<String>,
) -> StoreResult<RecordRow> {
    let NarrativeRow {
        scope_id,
        narrative,
        generated_at,
        fragment_count_at_generation,
        max_fragment_created_at_at_generation,
        distinct_keyword_count_cache,
        stats_fingerprint,
        created_at,
        updated_at,
    } = row;
    let created_at = normalize_timestamp(&created_at);
    let updated_at = normalize_timestamp(&updated_at);
    let content = json!({
        "text": narrative.unwrap_or_default(),
        "structured": { "nexus_legacy": {
            "family": family,
            "scope_id": scope_id,
            "generated_at": generated_at,
            "fragment_count_at_generation": fragment_count_at_generation,
            "max_fragment_created_at_at_generation": max_fragment_created_at_at_generation,
            "distinct_keyword_count_cache": distinct_keyword_count_cache,
            "stats_fingerprint": stats_fingerprint,
        }},
    });
    record_row(
        space_id,
        subject_id,
        RecordDraft {
            record_id: external_id(family, key_components),
            kind: SOUL_NARRATIVE_KIND.to_string(),
            content,
            visibility,
            context_id: scope_id,
            source_context_ids,
            occurred_at: None,
            created_at,
            updated_at,
            revision: 1,
        },
    )
}

#[derive(sqlx::FromRow)]
struct RunCaptureRow {
    operation_id: String,
    session_id: String,
    binding_id: String,
    lifecycle_epoch: i64,
    pending_id: String,
    captured_at: String,
}

/// A receipt-only capture carries no session text, so its record has an empty
/// `text` and the complete tuple in `structured` — the receipt is preserved
/// without inventing a review outcome.
async fn character_run_captures(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
    character_id: &str,
) -> StoreResult<Vec<RecordRow>> {
    let rows = sqlx::query_as::<_, RunCaptureRow>(
        "SELECT operation_id, session_id, binding_id, lifecycle_epoch, pending_id, captured_at \
         FROM character_run_captures WHERE character_id = ? \
         ORDER BY captured_at DESC, operation_id",
    )
    .bind(character_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let RunCaptureRow {
                operation_id,
                session_id,
                binding_id,
                lifecycle_epoch,
                pending_id,
                captured_at,
            } = row;
            let captured_at = normalize_timestamp(&captured_at);
            let record_id = external_id(FAMILY_RUN_CAPTURES, &[operation_id.as_str()]);
            let context_id = Some(binding_id.clone());
            let content = json!({
                "text": "",
                "structured": { "nexus_legacy": {
                    "family": FAMILY_RUN_CAPTURES,
                    "operation_id": operation_id,
                    "session_id": session_id,
                    "binding_id": binding_id,
                    "lifecycle_epoch": lifecycle_epoch,
                    "pending_id": pending_id,
                }},
            });
            record_row(
                space_id,
                subject_id,
                RecordDraft {
                    record_id,
                    kind: EVENT_KIND.to_string(),
                    content,
                    visibility: Placement::CONTEXT,
                    source_context_ids: context_id.clone().into_iter().collect(),
                    context_id,
                    occurred_at: Some(captured_at.clone()),
                    created_at: captured_at.clone(),
                    updated_at: captured_at,
                    revision: 1,
                },
            )
        })
        .collect()
}

/// Bearer memory files project as shared records: a long-term memory is the
/// bearer's own retained fact, and its authored kind survives the D-7 mapping.
fn long_term_records(
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
    owner: &Owner,
) -> StoreResult<Vec<RecordRow>> {
    let bearer = roots.bearer(owner);
    let slugs =
        memory_io::list_memories(&roots.home, bearer).map_err(|error| legacy_error(&error))?;
    let (family, owner_key) = match owner {
        Owner::Creator => (FAMILY_MEMORY_LONG_TERM, roots.creator_id.as_str()),
        Owner::Character(character_id) => (FAMILY_CHARACTER_LONG_TERM, character_id.as_str()),
    };
    let mut rows = Vec::with_capacity(slugs.len());
    for slug in slugs {
        let memory = memory_io::load_memory(&roots.home, bearer, &slug)
            .map_err(|error| legacy_error(&error))?;
        let kind = policy::namespaced_record_kind(&memory.frontmatter.memory_kind);
        let updated_at = normalize_timestamp(&memory.frontmatter.updated_at);
        let record_id = external_id(family, &[owner_key, slug.as_str()]);
        let content = json!({
            "text": memory.body,
            "structured": { "nexus_legacy": {
                "family": family,
                "slug": slug,
                "memory_id": memory.frontmatter.memory_id,
                "memory_kind": memory.frontmatter.memory_kind,
                "nexus_memory_version": memory.frontmatter.nexus_memory_version,
                "updated_at": memory.frontmatter.updated_at,
                "source_session_ids": memory.frontmatter.source_session_ids,
            }},
        });
        rows.push(record_row(
            space_id,
            subject_id,
            RecordDraft {
                record_id,
                kind,
                content,
                visibility: Placement::SHARED,
                context_id: None,
                source_context_ids: Vec::new(),
                occurred_at: None,
                created_at: updated_at.clone(),
                updated_at,
                revision: 1,
            },
        )?);
    }
    Ok(rows)
}

// ── Candidate projections ───────────────────────────────────────────────

async fn candidates_for_owner(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
    owner: &Owner,
) -> StoreResult<Vec<CandidateRow>> {
    match owner {
        Owner::Creator => creator_pending_reviews(tx, roots, space_id, subject_id).await,
        Owner::Character(character_id) => {
            character_pending_reviews(tx, space_id, subject_id, character_id).await
        }
    }
}

#[derive(sqlx::FromRow)]
struct CreatorPendingRow {
    pending_id: String,
    session_id: String,
    world_id: Option<String>,
    task_kind: String,
    raw_digest: String,
    created_at: String,
}

async fn creator_pending_reviews(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Vec<CandidateRow>> {
    let rows = sqlx::query_as::<_, CreatorPendingRow>(
        "SELECT pending_id, session_id, world_id, task_kind, raw_digest, created_at \
         FROM memory_pending_review WHERE creator_id = ? ORDER BY created_at, pending_id",
    )
    .bind(&roots.creator_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let CreatorPendingRow {
                pending_id,
                session_id,
                world_id,
                task_kind,
                raw_digest,
                created_at,
            } = row;
            let candidate_id = external_id(FAMILY_MEMORY_PENDING_REVIEW, &[pending_id.as_str()]);
            let source_context_ids: Vec<String> = world_id.clone().into_iter().collect();
            let content = json!({
                "text": "",
                "structured": { "nexus_legacy": {
                    "family": FAMILY_MEMORY_PENDING_REVIEW,
                    "pending_id": pending_id,
                    "session_id": session_id,
                    "world_id": world_id,
                    "task_kind": task_kind,
                    "raw_digest": raw_digest,
                }},
            });
            candidate_row(
                candidate_id,
                space_id,
                subject_id,
                &content,
                Placement::SHARED,
                None,
                &source_context_ids,
                normalize_timestamp(&created_at),
            )
        })
        .collect()
}

#[derive(sqlx::FromRow)]
struct CharacterPendingRow {
    pending_id: String,
    session_id: String,
    actor_world_binding_id: Option<String>,
    task_kind: String,
    raw_digest: String,
    source_operation_id: Option<String>,
    created_at: String,
}

async fn character_pending_reviews(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
    character_id: &str,
) -> StoreResult<Vec<CandidateRow>> {
    let rows = sqlx::query_as::<_, CharacterPendingRow>(
        "SELECT pending_id, session_id, actor_world_binding_id, task_kind, raw_digest, \
         source_operation_id, created_at FROM character_memory_pending_review \
         WHERE character_id = ? ORDER BY created_at, pending_id",
    )
    .bind(character_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|row| {
            let CharacterPendingRow {
                pending_id,
                session_id,
                actor_world_binding_id,
                task_kind,
                raw_digest,
                source_operation_id,
                created_at,
            } = row;
            let candidate_id = external_id(FAMILY_CHARACTER_PENDING_REVIEW, &[pending_id.as_str()]);
            let visibility = binding_visibility(actor_world_binding_id.as_deref());
            let context_id = actor_world_binding_id.clone();
            let content = json!({
                "text": "",
                "structured": { "nexus_legacy": {
                    "family": FAMILY_CHARACTER_PENDING_REVIEW,
                    "pending_id": pending_id,
                    "session_id": session_id,
                    "actor_world_binding_id": actor_world_binding_id,
                    "task_kind": task_kind,
                    "raw_digest": raw_digest,
                    "source_operation_id": source_operation_id,
                }},
            });
            candidate_row(
                candidate_id,
                space_id,
                subject_id,
                &content,
                visibility,
                context_id.clone(),
                context_id.as_slice(),
                normalize_timestamp(&created_at),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn candidate_row(
    candidate_id: String,
    space_id: &str,
    subject_id: &str,
    content: &Value,
    visibility: &'static str,
    context_id: Option<String>,
    source_context_ids: &[String],
    created_at: String,
) -> StoreResult<CandidateRow> {
    let classification = json!({
        "disposition": PENDING_REVIEW_DISPOSITION,
        "rationale_codes": ["legacy_pending_review"],
    });
    Ok(CandidateRow {
        candidate_id,
        space_id: space_id.to_string(),
        subject_id: subject_id.to_string(),
        capture_visibility: visibility.to_string(),
        capture_context_id: context_id,
        kind: EVENT_KIND.to_string(),
        content_json: Some(serialize(content, "legacy candidate content")?),
        provenance_json: serialize(
            &provenance(source_context_ids, &created_at),
            "legacy candidate provenance",
        )?,
        classification_json: serialize(&classification, "legacy candidate classification")?,
        status: "pending".to_string(),
        consumed_by_json: None,
        content_digest: None,
        occurred_at: None,
        expires_at: None,
        revision: 1,
        created_at,
    })
}

// ── Bearer files ────────────────────────────────────────────────────────

/// The `SOUL.md` bytes of a bearer, or `None` when the file is absent (an
/// honest-empty profile, never an error).
fn soul_document(roots: &LegacyRoots, owner: &Owner) -> StoreResult<Option<String>> {
    let path = roots.bearer(owner).soul_path(&roots.home);
    match std::fs::read_to_string(&path) {
        Ok(body) => Ok(Some(body)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StoreError::Corrupt(format!(
            "legacy SOUL document {}: {error}",
            path.display()
        ))),
    }
}

/// `SOUL.md`'s mtime as the profile timestamps; a fixed epoch when the file
/// system cannot report one, so repeated reads stay deterministic.
fn soul_timestamp(roots: &LegacyRoots, owner: &Owner) -> String {
    std::fs::metadata(roots.bearer(owner).soul_path(&roots.home))
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|offset| DateTime::from_timestamp(i64::try_from(offset.as_secs()).ok()?, 0))
        .map_or_else(|| EPOCH_TIMESTAMP.to_string(), timestamp)
}

fn legacy_error(error: &MemoryError) -> StoreError {
    StoreError::Corrupt(format!("legacy bearer file: {error}"))
}

// ── Projection helpers ──────────────────────────────────────────────────

struct RecordDraft {
    record_id: String,
    kind: String,
    content: Value,
    visibility: &'static str,
    context_id: Option<String>,
    source_context_ids: Vec<String>,
    occurred_at: Option<String>,
    created_at: String,
    updated_at: String,
    revision: i64,
}

fn record_row(space_id: &str, subject_id: &str, draft: RecordDraft) -> StoreResult<RecordRow> {
    let RecordDraft {
        record_id,
        kind,
        content,
        visibility,
        context_id,
        source_context_ids,
        occurred_at,
        created_at,
        updated_at,
        revision,
    } = draft;
    Ok(RecordRow {
        record_id,
        space_id: space_id.to_string(),
        subject_id: subject_id.to_string(),
        kind,
        content_json: serialize(&content, "legacy record content")?,
        scope_visibility: visibility.to_string(),
        scope_context_id: context_id,
        provenance_json: serialize(
            &provenance(&source_context_ids, &created_at),
            "legacy record provenance",
        )?,
        import_ref_json: None,
        occurred_at,
        tombstone_json: None,
        revision,
        created_at,
        updated_at,
    })
}

/// The §6.6 provenance every projected row carries: no client claim, no source
/// anchor, the bridge as the capturing principal.
fn provenance(source_context_ids: &[String], captured_at: &str) -> Value {
    json!({
        "source_context_ids": source_context_ids,
        "anchors": [],
        "captured_by": LEGACY_CAPTURED_BY,
        "captured_at": captured_at,
    })
}

const fn binding_visibility(binding_id: Option<&str>) -> &'static str {
    match binding_id {
        Some(_) => Placement::CONTEXT,
        None => Placement::SHARED,
    }
}

/// E4 tolerates a malformed `keywords` column as empty (its own fragment
/// projection does), so the bridge reports the same value it would.
fn keywords_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// The provider's single timestamp rendering, so a merged page orders provider
/// and legacy rows on one comparable format. Legacy SQLite columns carry
/// `YYYY-MM-DD HH:MM:SS` and bearer frontmatter carries RFC 3339; a value in
/// neither form is kept verbatim rather than replaced by an invented time.
fn normalize_timestamp(raw: &str) -> String {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return timestamp(parsed.with_timezone(&Utc));
    }
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"]
        .into_iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
        .map_or_else(|| raw.to_string(), |naive| timestamp(naive.and_utc()))
}

fn serialize(value: &Value, what: &str) -> StoreResult<String> {
    serde_json::to_string(value).map_err(|error| StoreError::Corrupt(format!("{what}: {error}")))
}

// ── External identity ───────────────────────────────────────────────────

/// `lgc:<family>:<component>[:<component>…]`, each component percent-escaped so
/// the id is both reversible and safe to store as one opaque string.
fn external_id(family: &str, components: &[&str]) -> String {
    let mut id = format!("{ID_PREFIX}:{family}");
    for component in components {
        id.push(':');
        id.push_str(&encode_component(component));
    }
    id
}

fn parse_id(id: &str) -> Option<(String, Vec<String>)> {
    let rest = id.strip_prefix(ID_PREFIX)?.strip_prefix(':')?;
    let mut parts = rest.split(':');
    let family = parts.next()?.to_string();
    let components = parts.map(decode_component).collect::<Option<Vec<_>>>()?;
    Some((family, components))
}

const HEX_DIGITS: [u8; 16] = *b"0123456789ABCDEF";

fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0F)]));
        }
    }
    encoded
}

fn decode_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_id_is_stable_and_reversible() {
        let first = external_id("memory_fragments", &["frag_abc"]);
        let second = external_id("memory_fragments", &["frag_abc"]);
        assert_eq!(first, second, "ids never depend on read order or time");
        assert_eq!(
            parse_id(&first),
            Some(("memory_fragments".to_string(), vec!["frag_abc".to_string()]))
        );
    }

    #[test]
    fn external_id_escapes_separators_and_null_components() {
        // A slug may contain ':' (only '/', '\\', '..' and control chars are
        // rejected), so the separator must be escaped for the id to parse back.
        let id = external_id(
            "character_long_term",
            &["chr_ab", "a:b/../c", NULL_COMPONENT],
        );
        assert!(!id.contains("a:b"), "a separator in a key is escaped: {id}");
        assert_eq!(
            parse_id(&id),
            Some((
                "character_long_term".to_string(),
                vec![
                    "chr_ab".to_string(),
                    "a:b/../c".to_string(),
                    NULL_COMPONENT.to_string(),
                ]
            ))
        );
        assert_eq!(parse_id("rec_00000000"), None);
        assert_eq!(parse_id("lgc:family:%zz"), None);
    }

    #[test]
    fn timestamps_normalize_to_one_rendering() {
        assert_eq!(
            normalize_timestamp("2026-11-02 08:05:00"),
            "2026-11-02T08:05:00Z"
        );
        assert_eq!(
            normalize_timestamp("2026-11-02T08:05:00.123+02:00"),
            "2026-11-02T06:05:00Z"
        );
        assert_eq!(normalize_timestamp("not a time"), "not a time");
    }
}
