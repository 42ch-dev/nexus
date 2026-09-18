//! Actor holder registry (v1.191 P1 T3).
//!
//! One service-managed holder per stored Creator or Character identity, in each
//! workspace database. The registry owns holder identity; the Actor tables own
//! lifecycle/name — the projected label is never copied into a second
//! authority here (`holder-governance.md` §2.1).
//!
//! # Identity stability
//!
//! The id is a pure function of the subject: `hld_` plus the full lowercase
//! 256-bit BLAKE3 digest of the UTF-8 bytes `nexus-holder-v1\0creator\0<creator_id>`
//! (or `nexus-holder-v1\0character\0<character_id>`). The same global Creator's
//! workspace materializations therefore resolve to the same id with no
//! cross-database transaction. A pre-existing id that resolves to another
//! subject — or a subject already bound to another id — is a hard integrity
//! error: holders are never reassigned and never re-minted on a read.
//!
//! # Migration staging
//!
//! SQLite has no BLAKE3 function and this crate forbids `unsafe`, so the
//! digest cannot be computed inside the SQL migration. The migration runner
//! ([`stage_holder_digests_in_tx`], called from
//! `crate::apply_fk_suspension_tx`) stages `<subject_kind, subject_id,
//! holder_entry_id>` rows on the migration connection inside the migration
//! transaction; the migration script backfills the registry from that staging
//! and aborts when it is absent or incomplete.
//!
//! # Scope
//!
//! Lifecycle provisioning (Creator/Character create, archive, rename, delete)
//! is not this module's job beyond these primitives: subject and holder must
//! commit in the caller's transaction before the identity is usable.

use std::collections::HashMap;

use sqlx::{Executor, Sqlite};

use crate::LocalDbError;

/// Reserved namespace for holder entry ids; never an ordinary narrative KE id.
pub const HOLDER_ENTRY_ID_PREFIX: &str = "hld_";

/// Domain separation for the Creator subject (`nexus-holder-v1\0creator\0`).
const HOLDER_ID_DOMAIN_CREATOR: &[u8] = b"nexus-holder-v1\0creator\0";

/// Domain separation for the Character subject (`nexus-holder-v1\0character\0`).
const HOLDER_ID_DOMAIN_CHARACTER: &[u8] = b"nexus-holder-v1\0character\0";

/// Version of the migration that creates the registry and adds the native
/// governance columns (migration `20260918000001_holder_registry.sql`).
///
/// The migration runner uses this to decide when to stage the subject digests.
pub const HOLDER_MIGRATION_VERSION: i64 = 20_260_918_000_001;

/// Connection-local table the runner fills with the subject digests before the
/// holder migration script runs. Created and dropped inside the migration
/// transaction, so it never survives into a migrated workspace.
pub(crate) const HOLDER_DIGEST_STAGING_TABLE: &str = "_v1191_holder_digest_staging";

/// The exact UTF-8 bytes hashed for a subject (§2.1).
fn holder_id_input(domain: &[u8], subject_id: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(domain.len() + subject_id.len());
    bytes.extend_from_slice(domain);
    bytes.extend_from_slice(subject_id.as_bytes());
    bytes
}

/// `hld_` + full lowercase BLAKE3 digest of the domain-separated subject bytes.
fn holder_entry_id(domain: &[u8], subject_id: &str) -> String {
    let digest = blake3::hash(&holder_id_input(domain, subject_id));
    format!("{HOLDER_ENTRY_ID_PREFIX}{}", digest.to_hex())
}

/// The stable holder id of a stored Creator (§2.1).
#[must_use]
pub fn creator_holder_entry_id(creator_id: &str) -> String {
    holder_entry_id(HOLDER_ID_DOMAIN_CREATOR, creator_id)
}

/// The stable holder id of a stored Character (§2.1).
#[must_use]
pub fn character_holder_entry_id(character_id: &str) -> String {
    holder_entry_id(HOLDER_ID_DOMAIN_CHARACTER, character_id)
}

/// The subject a holder is registered for. Exactly one subject column is
/// non-null in the registry (exclusive-subject CHECK).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HolderSubject {
    Creator(String),
    Character(String),
}

impl HolderSubject {
    /// The registry selector for this subject: `(kind, subject_id)`.
    fn selector(&self) -> (&'static str, &str) {
        match self {
            Self::Creator(id) => ("creator", id),
            Self::Character(id) => ("character", id),
        }
    }

    /// The stable holder id this subject must resolve to.
    fn expected_holder_entry_id(&self) -> String {
        match self {
            Self::Creator(id) => creator_holder_entry_id(id),
            Self::Character(id) => character_holder_entry_id(id),
        }
    }

    /// Human-readable subject for integrity errors.
    fn describe(&self) -> String {
        let (kind, id) = self.selector();
        format!("{kind}:{id}")
    }
}

/// A `knowledge_holders` row: holder identity plus its exclusive subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeHolder {
    pub holder_entry_id: String,
    pub creator_id: Option<String>,
    pub character_id: Option<String>,
    pub created_at: String,
}

impl KnowledgeHolder {
    /// The registered subject. `None` only for a row violating the registry's
    /// exclusive-subject CHECK (corrupt registry, not reachable through writes).
    #[must_use]
    pub fn subject(&self) -> Option<HolderSubject> {
        match (&self.creator_id, &self.character_id) {
            (Some(creator_id), None) => Some(HolderSubject::Creator(creator_id.clone())),
            (None, Some(character_id)) => Some(HolderSubject::Character(character_id.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct HolderRow {
    holder_entry_id: String,
    creator_id: Option<String>,
    character_id: Option<String>,
    created_at: String,
}

impl From<HolderRow> for KnowledgeHolder {
    fn from(row: HolderRow) -> Self {
        Self {
            holder_entry_id: row.holder_entry_id,
            creator_id: row.creator_id,
            character_id: row.character_id,
            created_at: row.created_at,
        }
    }
}

/// Read-only holder resolution (§2.2).
///
/// Accepts a pool or an in-flight transaction (`&mut *tx`), so a caller that
/// must resolve the registry row in the same transaction as its write can do
/// so without a second read path.
///
/// `Ok(None)` means the id is not registered here: the caller decides whether
/// that is a missing state (`holder_state_invalid`) or simply "no such holder".
/// Missing registry state is never repaired and never minted.
///
/// A row whose id does **not** re-derive from its registered subject is corrupt
/// registry state, not a holder (§2.1: "collision/mismatch is a hard integrity
/// error, never reassignment"), so it is an error rather than a returned row —
/// a digest that does not bind to its subject must never be admitted as
/// authority.
///
/// # Errors
///
/// Returns [`LocalDbError::ConstraintViolation`] (`holder_state_invalid`) for a
/// corrupt registry row and [`LocalDbError::Sqlx`] on database failure.
pub async fn resolve_holder<'e, E>(
    executor: E,
    holder_entry_id: &str,
) -> Result<Option<KnowledgeHolder>, LocalDbError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query_as::<_, HolderRow>(
        "SELECT holder_entry_id, creator_id, character_id, created_at \
         FROM knowledge_holders WHERE holder_entry_id = ?",
    )
    .bind(holder_entry_id)
    .fetch_optional(executor)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let holder = KnowledgeHolder::from(row);
    let Some(subject) = holder.subject() else {
        return Err(holder_state_invalid(&format!(
            "registry row {} has no exclusive Creator/Character subject",
            holder.holder_entry_id
        )));
    };
    let derived = subject.expected_holder_entry_id();
    if derived != holder.holder_entry_id {
        return Err(holder_state_invalid(&format!(
            "registry row {} does not re-derive from its subject {} (expected {derived})",
            holder.holder_entry_id,
            subject.describe()
        )));
    }
    Ok(Some(holder))
}

/// Corrupt-registry error (`holder_state_invalid`, durable §2.2) for a row that
/// cannot be admitted as holder authority.
fn holder_state_invalid(reason: &str) -> LocalDbError {
    LocalDbError::ConstraintViolation {
        table: "knowledge_holders".to_string(),
        constraint: format!("holder_state_invalid: {reason}"),
    }
}

/// Ensure the stable holder of `creator_id` exists in `tx` (§2.1).
///
/// Returns the holder id. The subject's own row must exist in the same
/// transaction (registry FK), so Creator materialization commits both together.
///
/// # Errors
///
/// Returns [`LocalDbError::ConstraintViolation`] when the subject is empty,
/// when the subject is already bound to another holder id, or when the derived
/// id is already bound to another subject (never reassignment), and
/// [`LocalDbError::Sqlx`] on database failure.
pub async fn ensure_creator_holder_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    creator_id: &str,
) -> Result<String, LocalDbError> {
    ensure_holder_in_tx(tx, &HolderSubject::Creator(creator_id.to_string())).await
}

/// Ensure the stable holder of `character_id` exists in `tx` (§2.1).
///
/// # Errors
///
/// Same as [`ensure_creator_holder_in_tx`].
pub async fn ensure_character_holder_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    character_id: &str,
) -> Result<String, LocalDbError> {
    ensure_holder_in_tx(tx, &HolderSubject::Character(character_id.to_string())).await
}

fn holder_integrity_error(constraint: String) -> LocalDbError {
    LocalDbError::ConstraintViolation {
        table: "knowledge_holders".to_string(),
        constraint,
    }
}

async fn ensure_holder_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    subject: &HolderSubject,
) -> Result<String, LocalDbError> {
    let (kind, subject_id) = subject.selector();
    if subject_id.is_empty() {
        return Err(holder_integrity_error(format!(
            "{kind} id must be nonempty"
        )));
    }
    let expected = subject.expected_holder_entry_id();

    // 1. The subject must not already resolve to another id.
    let subject_column = match kind {
        "creator" => "creator_id",
        _ => "character_id",
    };
    let by_subject: Option<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT holder_entry_id FROM knowledge_holders WHERE {subject_column} = ?"
    )))
    .bind(subject_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((stored,)) = by_subject {
        if stored == expected {
            return Ok(expected);
        }
        return Err(holder_integrity_error(format!(
            "{} is bound to holder {stored}, not the derived {expected}; \
             refusing reassignment",
            subject.describe()
        )));
    }

    // 2. The derived id must not already belong to another subject.
    let by_id: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT creator_id, character_id FROM knowledge_holders WHERE holder_entry_id = ?",
    )
    .bind(&expected)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((creator_id, character_id)) = by_id {
        let bound = match (creator_id, character_id) {
            (Some(id), None) => format!("creator:{id}"),
            (None, Some(id)) => format!("character:{id}"),
            _ => "an invalid subject".to_string(),
        };
        return Err(holder_integrity_error(format!(
            "holder {expected} is already bound to {bound}; \
             refusing to reassign it to {}",
            subject.describe()
        )));
    }

    // 3. Register. `BEGIN IMMEDIATE` callers serialize the check-then-insert.
    let (creator_id, character_id) = match subject {
        HolderSubject::Creator(id) => (Some(id.clone()), None),
        HolderSubject::Character(id) => (None, Some(id.clone())),
    };
    sqlx::query(
        "INSERT INTO knowledge_holders (holder_entry_id, creator_id, character_id, created_at) \
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind(&expected)
    .bind(creator_id)
    .bind(character_id)
    .execute(&mut **tx)
    .await?;
    Ok(expected)
}

/// Stage every stored Creator/Character subject with its derived holder id for
/// the holder migration (§2.1, §5.3).
///
/// Called by the migration runner inside the migration transaction, before the
/// migration script executes. Collision detection here is the same integrity
/// rule as [`ensure_creator_holder_in_tx`]: two subjects may never share a
/// holder id.
///
/// # Errors
///
/// Returns [`LocalDbError::ConstraintViolation`] on a subject-id collision and
/// [`LocalDbError::Sqlx`] on database failure.
pub(crate) async fn stage_holder_digests_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
) -> Result<(), LocalDbError> {
    // SAFETY: TEMP table DDL with a fixed name; the migration script reads the
    // same name (and aborts when this staging is missing).
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TEMP TABLE IF NOT EXISTS {HOLDER_DIGEST_STAGING_TABLE} (\
             subject_kind TEXT NOT NULL CHECK (subject_kind IN ('creator', 'character')), \
             subject_id TEXT NOT NULL, \
             holder_entry_id TEXT NOT NULL, \
             PRIMARY KEY (subject_kind, subject_id), \
             UNIQUE (holder_entry_id))"
    )))
    .execute(&mut **tx)
    .await?;

    let creators: Vec<(String,)> =
        sqlx::query_as("SELECT creator_id FROM creators ORDER BY creator_id")
            .fetch_all(&mut **tx)
            .await?;
    let characters: Vec<(String,)> =
        sqlx::query_as("SELECT character_id FROM characters ORDER BY character_id")
            .fetch_all(&mut **tx)
            .await?;

    let mut staged: Vec<(&'static str, String, String)> =
        Vec::with_capacity(creators.len() + characters.len());
    let mut owners: HashMap<String, String> = HashMap::with_capacity(staged.capacity());
    for (kind, rows) in [("creator", creators), ("character", characters)] {
        for (subject_id,) in rows {
            let holder_entry_id = match kind {
                "creator" => creator_holder_entry_id(&subject_id),
                _ => character_holder_entry_id(&subject_id),
            };
            if let Some(previous) =
                owners.insert(holder_entry_id.clone(), format!("{kind}:{subject_id}"))
            {
                return Err(holder_integrity_error(format!(
                    "holder id {holder_entry_id} collides between {previous} and {kind}:{subject_id}"
                )));
            }
            staged.push((kind, subject_id, holder_entry_id));
        }
    }

    for (kind, subject_id, holder_entry_id) in staged {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {HOLDER_DIGEST_STAGING_TABLE} (subject_kind, subject_id, holder_entry_id) \
             VALUES (?, ?, ?)"
        )))
        .bind(kind)
        .bind(&subject_id)
        .bind(&holder_entry_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    /// Pinned digest of `nexus-holder-v1\0creator\0ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`
    /// per the §2.1 recipe (BLAKE3, full 256-bit lowercase hex).
    const PINNED_CREATOR_HOLDER: &str =
        "hld_c07e80d881ab00dd1b73c5d4ae74ca20ffb9b39702cf310ce43b759fcf5753a7";
    /// Pinned digest of `nexus-holder-v1\0character\0ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`.
    const PINNED_CHARACTER_HOLDER: &str =
        "hld_bb4d1d30cd1166a7a1c1a85ef1fea6de3f3844ce455f1aee266a201086b0121e";

    /// Fresh migrated pool in a tempdir (same pattern as `creators::tests`).
    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    /// §2.1 byte recipe, NUL separators included.
    #[test]
    fn holder_id_input_is_the_frozen_domain_separated_bytes() {
        assert_eq!(
            holder_id_input(HOLDER_ID_DOMAIN_CREATOR, "ctr_a"),
            b"nexus-holder-v1\0creator\0ctr_a".to_vec()
        );
        assert_eq!(
            holder_id_input(HOLDER_ID_DOMAIN_CHARACTER, "chr_a"),
            b"nexus-holder-v1\0character\0chr_a".to_vec()
        );
    }

    /// Regression pin: the id is `hld_` + the full lowercase 256-bit digest,
    /// stable across processes, and domain-separated between subject kinds.
    #[test]
    fn holder_entry_id_is_stable_and_domain_separated() {
        let creator = creator_holder_entry_id("ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(creator, PINNED_CREATOR_HOLDER);
        assert_eq!(
            character_holder_entry_id("ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            PINNED_CHARACTER_HOLDER
        );
        // Domain separation on the *same* subject string.
        assert_ne!(
            creator_holder_entry_id("subject_same"),
            character_holder_entry_id("subject_same")
        );
        // Deterministic: a second call (a second process) is identical.
        assert_eq!(
            creator,
            creator_holder_entry_id("ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert_ne!(
            creator,
            creator_holder_entry_id("ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }

    #[tokio::test]
    async fn ensure_registers_once_and_resolves_the_subject() {
        let (pool, _dir) = fresh_pool().await;
        crate::ensure_creator_row(&pool, "ctr_holder_a", "Holder A")
            .await
            .unwrap();

        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        let created = ensure_creator_holder_in_tx(&mut tx, "ctr_holder_a")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(created, creator_holder_entry_id("ctr_holder_a"));

        // Idempotent: the same subject resolves the same id, no second row.
        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        let again = ensure_creator_holder_in_tx(&mut tx, "ctr_holder_a")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(again, created);

        let resolved = resolve_holder(&pool, &created).await.unwrap().unwrap();
        assert_eq!(
            resolved.subject(),
            Some(HolderSubject::Creator("ctr_holder_a".to_string()))
        );
        assert!(resolved.creator_id.is_some() && resolved.character_id.is_none());

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_holders")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "one holder per subject");
    }

    #[tokio::test]
    async fn subject_bound_to_another_id_is_a_hard_error() {
        let (pool, _dir) = fresh_pool().await;
        crate::ensure_creator_row(&pool, "ctr_holder_b", "Holder B")
            .await
            .unwrap();
        // A foreign/hand-written registry row binds the subject to another id.
        sqlx::query(
            "INSERT INTO knowledge_holders (holder_entry_id, creator_id, created_at) \
             VALUES ('hld_foreign', 'ctr_holder_b', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        let err = ensure_creator_holder_in_tx(&mut tx, "ctr_holder_b")
            .await
            .unwrap_err();
        let _ = tx.rollback().await;
        assert!(
            matches!(&err, LocalDbError::ConstraintViolation { table, constraint }
                if table == "knowledge_holders" && constraint.contains("refusing reassignment")),
            "expected a hard integrity error, got {err:?}"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_holders")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "no row may be added");
    }

    #[tokio::test]
    async fn id_bound_to_another_subject_is_a_hard_error() {
        let (pool, _dir) = fresh_pool().await;
        crate::ensure_creator_row(&pool, "ctr_holder_c", "Holder C")
            .await
            .unwrap();
        crate::ensure_creator_row(&pool, "ctr_holder_d", "Holder D")
            .await
            .unwrap();
        // The id that `ctr_holder_c` must derive is already bound to another subject.
        sqlx::query(
            "INSERT INTO knowledge_holders (holder_entry_id, creator_id, created_at) \
             VALUES (?, 'ctr_holder_d', datetime('now'))",
        )
        .bind(creator_holder_entry_id("ctr_holder_c"))
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        let err = ensure_creator_holder_in_tx(&mut tx, "ctr_holder_c")
            .await
            .unwrap_err();
        let _ = tx.rollback().await;
        assert!(
            matches!(&err, LocalDbError::ConstraintViolation { constraint, .. }
                if constraint.contains("already bound to creator:ctr_holder_d")),
            "expected a collision error naming the bound subject, got {err:?}"
        );
    }

    /// A registry row whose id does not re-derive from its subject is corrupt
    /// state (§2.1): resolution must refuse it (`holder_state_invalid`) rather
    /// than hand back an id that its own subject does not bind to.
    #[tokio::test]
    async fn corrupt_registry_row_is_not_resolved_as_a_holder() {
        let (pool, _dir) = fresh_pool().await;
        crate::ensure_creator_row(&pool, "ctr_corrupt", "Corrupt")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO knowledge_holders (holder_entry_id, creator_id, created_at) \
             VALUES ('hld_corrupt', 'ctr_corrupt', datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = resolve_holder(&pool, "hld_corrupt").await.unwrap_err();
        assert!(
            matches!(&err, LocalDbError::ConstraintViolation { table, constraint }
                if table == "knowledge_holders"
                    && constraint.contains("holder_state_invalid")
                    && constraint.contains("does not re-derive")),
            "expected holder_state_invalid for a non-deriving row, got {err:?}"
        );

        // A correctly registered holder resolves; an unregistered id is absent.
        crate::ensure_creator_row(&pool, "ctr_derived", "Derived")
            .await
            .unwrap();
        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        let derived = ensure_creator_holder_in_tx(&mut tx, "ctr_derived")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let resolved = resolve_holder(&pool, &derived).await.unwrap().unwrap();
        assert_eq!(
            resolved.subject(),
            Some(HolderSubject::Creator("ctr_derived".to_string()))
        );
        assert!(resolve_holder(&pool, "hld_absent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn empty_subject_and_unknown_holder_are_refused() {
        let (pool, _dir) = fresh_pool().await;
        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        assert!(matches!(
            ensure_creator_holder_in_tx(&mut tx, "").await,
            Err(LocalDbError::ConstraintViolation { .. })
        ));
        let _ = tx.rollback().await;
        assert!(resolve_holder(&pool, "hld_missing")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn staging_covers_creators_and_characters() {
        let (pool, _dir) = fresh_pool().await;
        crate::ensure_creator_row(&pool, "ctr_staged", "Staged")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO characters (character_id, owner_creator_id, display_name, status, \
             created_at, updated_at) \
             VALUES ('chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'ctr_staged', 'Staged', 'archived', \
                     '2026-09-18T00:00:00Z', '2026-09-18T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut tx = crate::begin_immediate(&pool).await.unwrap();
        stage_holder_digests_in_tx(&mut tx).await.unwrap();
        let staged: Vec<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT subject_kind, subject_id, holder_entry_id FROM {HOLDER_DIGEST_STAGING_TABLE} \
             ORDER BY subject_kind"
        )))
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        let _ = tx.rollback().await;

        assert_eq!(
            staged,
            vec![
                (
                    "character".to_string(),
                    "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    character_holder_entry_id("chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                ),
                (
                    "creator".to_string(),
                    "ctr_staged".to_string(),
                    creator_holder_entry_id("ctr_staged")
                ),
            ],
            "archived subjects are staged too"
        );
    }
}
