//! Provider-private storage and transaction layer for `LocalMemoryProvider`.
//!
//! Every write runs inside the core writer protocol: the pool is opened through
//! [`init_guarded_pool`], so its connections carry the writer scalar functions
//! and the `core_writer_registration` row the `guard_memory_*` triggers demand.
//! A raw pool (no registration, no scalar functions) is fenced with
//! `WRITER_FENCED` at the schema boundary — this layer adds no second admission
//! path, and the provider never opens its own unguarded handle.
//!
//! Invariants that must be one transaction with the effect live here rather than
//! in the facade, because a value read on a pooled connection before the write
//! transaction can be superseded between the read and the write:
//!
//! * **Admission + effect** (§7.3/§11, MEM-R017). Each mutating entry point takes
//!   an [`Admission`] and opens its transaction with `BEGIN IMMEDIATE`, then
//!   evaluates authorization, the dedup probe and the freeze/fence lifecycle
//!   gates *inside* that transaction. A freeze therefore either commits before
//!   the op begins (which then sees it and refuses) or after the op commits —
//!   never in between. The authorization clock is read *after* the lock is held
//!   ([`begin_admission`]), so a grant that expires while the op waits behind
//!   another writer cannot authorize it.
//! * **Dedup + receipt** (§7.0.5, MEM-R008/R021). The `memory_receipts` primary
//!   key IS the namespace `(space_id, requester_principal_id, op,
//!   idempotency_key)`; a same-namespace replay returns the stored receipt
//!   without re-executing, and a same-namespace different-`payload_digest`
//!   attempt is `IDEMPOTENCY_CONFLICT`. The probe runs after current
//!   authorization but before every fresh-effect rule — issuance
//!   non-amplification (§7.9), the CAS, freeze and fence preconditions — so a
//!   replay never re-evaluates the constraints of the receipt it returns.
//! * **Revision CAS** (MEM-R007). `expected_revision` is compared and `revision`
//!   incremented in the same `UPDATE ... WHERE revision = ?` statement.
//! * **Receipt retention** (§6.10/MEM-R021). A receipt body is served for
//!   `receipt_retention_seconds`, then *removed from the row*: the content-free
//!   tombstone — stored namespace, digest, id and terminal status — is all that
//!   answers, so a replay and `memory.receipts.get` fail `RECEIPT_EXPIRED`
//!   (terminal, `details.last_known_status`) instead of silently re-executing.
//!   Past `dedup_retention_seconds` the tombstone row itself is dropped and the
//!   namespace is free again, on read and replay alike — a tombstone is not an
//!   eternal answer. Every mutating entry point and every provider open first
//!   reclaims expired rows in a transaction of its own
//!   ([`MemoryStore::reclaim_expired_receipts`]) — all namespaces, keyless
//!   receipts included — so eviction is committed whatever the operation then
//!   does: a replay refusal, an authorization failure or any other early return
//!   drops only the effect transaction. A sweep failure is logged and never
//!   fails or rolls back the operation it precedes. `memory.receipts.get` stays
//!   read-only (MEM-R011) and only reports what the sweep left; the provider
//!   clock never runs backwards ([`MemoryStore::now`]), so a body seen past its
//!   retention window is never served again whatever the wall clock does next.
//! * **Error class** (§14.2/§14.3). A failure raised before `COMMIT` is a clean
//!   no-effect [`StoreError::Database`]/[`StoreError::Corrupt`]; a failure at or
//!   after `COMMIT` is [`StoreError::CommitUnknown`], which the facade reports as
//!   `OUTCOME_UNCERTAIN`.
//!
//! `memory.subjects.ensure` is the sole idempotency-key exemption (§7.0.5): its
//! receipt carries a NULL key, which the partial unique index excludes from the
//! dedup namespace, so two ensures never collide and never replay each other.
//!
//! The content tables are provider-owned (`memory_candidates`, `memory_records`,
//! `memory_profiles`) and carry the same control metadata from their first row:
//! a `revision` CAS surface, a provider-minted identity that is never reused,
//! and the writer admission above. Authorization over content hangs off a row's
//! placement, so [`Coverage`] resolves those requirements inside the effect
//! transaction instead of trusting a value read before it.

use std::path::Path;
#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicI64, Ordering};
#[cfg(feature = "test-hooks")]
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use spoke_operations::spoke_schemas::MemoryGrant;
use spoke_operations::{
    grant_covers, is_freeze_allowlisted, validate_grant_issuance, GrantRequirement, IssuedGrant,
    SpokeResult,
};
use sqlx::SqlitePool;
use uuid::Uuid;

use nexus_local_db::writer_protocol::{init_guarded_pool, GuardedPool};
use nexus_local_db::{begin_immediate, LocalDbError};

/// §6.10 receipt ids are provider-assigned opaque strings.
fn mint_receipt_id() -> String {
    format!("rcp_{}", Uuid::new_v4().simple())
}

/// The one canonical timestamp rendering.
///
/// Every timestamp column this module writes and every stored expiry is compared
/// as text, so the rendering must be fixed: RFC 3339 UTC, whole seconds, `Z`.
pub(super) fn timestamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug)]
pub(super) enum StoreError {
    /// Something the caller can see and correct (missing row, stale revision…).
    NotFound(String),
    /// Absence of grant coverage for the op (§6.5/§7.9).
    NotAuthorized(String),
    /// A frozen space refuses an op outside the §8.2 allowlist (§7.3/§8.2).
    SpaceFrozen(String),
    /// A fenced or archived subject refuses writes (§7.5/§7.7/§7.9), and so does
    /// a placement into one of its fenced contexts (§7.7/MEM-R024).
    SubjectFenced {
        subject_id: String,
        status: String,
        context_id: Option<String>,
    },
    RevisionConflict {
        current: i64,
    },
    IdempotencyConflict,
    /// The receipt body is past `receipt_retention_seconds`; the dedup tombstone
    /// still knows how the op ended (§6.10/MEM-R021, §7.28).
    ReceiptExpired {
        last_known_status: String,
    },
    /// The candidate left the §8.1 `pending` state: consumption is terminal.
    CandidateConsumed(String),
    /// A forbidden lifecycle transition (§7.7/§7.10).
    StatusTransition(String),
    /// The transaction may have committed (§14.3). Never reported as a clean
    /// no-effect failure.
    CommitUnknown(String),
    /// A stored row this provider's own writer could not have produced.
    Corrupt(String),
    Database(LocalDbError),
}

impl From<LocalDbError> for StoreError {
    fn from(value: LocalDbError) -> Self {
        Self::Database(value)
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(LocalDbError::Sqlx(value))
    }
}

type StoreResult<T> = Result<T, StoreError>;

/// Test-only commit fault, injected by the §14.2/§14.3 proof; compiled out of a
/// production build (`test-hooks`).
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_NONE: u8 = 0;
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_BEFORE: u8 = 1;
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_AFTER: u8 = 2;

/// What admits a mutating op (§6.5): one requirement per capability the op
/// needs, and every one of them must be covered by the grant set.
///
/// Two ops derive their requirements from state the request does not name —
/// candidate consumption and record writes name only an id, while the coverage
/// they need hangs off that row's immutable placement. Those resolve the
/// requirement *inside* the effect transaction, so authorization still sees
/// exactly the row the write will touch.
pub(super) enum Coverage<'a> {
    /// Requirements known from the request; empty = admitted by another rule.
    Requirements(&'a [GrantRequirement<'a>]),
    /// Candidate-scoped: coverage of the candidate's `capture_scope` (§6.7).
    Candidate {
        candidate_id: &'a str,
        capabilities: &'a [&'a str],
    },
    /// Record-scoped: coverage of the record's current placement, plus — for
    /// `memory.records.promote` — of the requested target placement (§7.21).
    Record {
        record_id: &'a str,
        capabilities: &'a [&'a str],
        target: Option<Placement<'a>>,
    },
}

pub(super) struct Admission<'a> {
    pub op: &'a str,
    pub space_id: &'a str,
    pub principal_id: &'a str,
    /// §6.5 coverage, evaluated inside the effect transaction.
    pub coverage: Coverage<'a>,
    /// §7.9 non-amplification input, evaluated over the same grant set.
    pub issuance: Option<&'a IssuedGrant<'a>>,
    /// Subject whose `fenced`/`archived` status rejects the write (§7.5/§7.9).
    pub subject_gate: Option<&'a str>,
    /// Placement the op writes content into, checked against that subject's
    /// `fenced_contexts` in the same transaction (§7.7/MEM-R024). Row-scoped ops
    /// resolve the same check from the row they are about to write.
    pub placement_gate: Option<Placement<'a>>,
    /// Whether the §8.2 freeze gate applies. The space lifecycle ops define
    /// their own admission (§7.3/§7.4) and set this to false.
    pub freeze_gated: bool,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SpaceRow {
    pub space_id: String,
    pub display_name: Option<String>,
    pub authority_host_id: String,
    pub authority_generation: i64,
    pub status: String,
    pub revision: i64,
    pub created_at: String,
    pub pending_receipts: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SubjectRow {
    pub space_id: String,
    pub subject_id: String,
    pub kind_ns: Option<String>,
    pub kind_id: Option<String>,
    pub status: String,
    pub fenced_contexts_json: String,
    pub revision: i64,
    pub created_at: String,
}

impl SubjectRow {
    pub(super) fn fenced_contexts(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.fenced_contexts_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_subjects.fenced_contexts_json for {}: {e}",
                self.subject_id
            ))
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct GrantRow {
    pub grant_id: String,
    pub space_id: String,
    pub subject_id: Option<String>,
    pub principal_id: String,
    pub capabilities_json: String,
    pub context_ids_json: String,
    pub shared_access: i64,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
}

impl GrantRow {
    pub(super) fn capabilities(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.capabilities_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_grants.capabilities_json for {}: {e}",
                self.grant_id
            ))
        })
    }

    pub(super) fn context_ids(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.context_ids_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_grants.context_ids_json for {}: {e}",
                self.grant_id
            ))
        })
    }

    /// The §6.5 wire projection the protocol's own evaluators take.
    fn wire(&self) -> StoreResult<MemoryGrant> {
        super::grant_wire(self).map_err(|error| StoreError::Corrupt(error.message()))
    }
}

/// A committed memory receipt as stored (§6.10). `idempotency_key` is `None`
/// only for the `memory.subjects.ensure` exemption. A settled row whose payloads
/// are both `None` is the content-free tombstone left by body eviction.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct ReceiptRow {
    pub space_id: String,
    pub requester_principal_id: String,
    pub op: String,
    pub idempotency_key: Option<String>,
    pub receipt_id: String,
    pub payload_digest: String,
    pub status: String,
    pub result_json: Option<String>,
    pub error_json: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Stable operation identity handed to every mutating entry point.
pub(super) struct OpContext<'a> {
    pub space_id: &'a str,
    pub requester_principal_id: &'a str,
    pub op: &'a str,
    /// `None` for the `memory.subjects.ensure` exemption only (§7.0.5).
    pub idempotency_key: Option<&'a str>,
    pub payload_digest: &'a str,
}

/// A grant row as the caller issues it (§7.9).
pub(super) struct NewGrant<'a> {
    pub grant_id: &'a str,
    pub space_id: &'a str,
    pub subject_id: Option<&'a str>,
    pub principal_id: &'a str,
    pub capabilities: &'a [String],
    pub context_ids: &'a [String],
    pub shared_access: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// §7.7 fence target.
pub(super) enum FenceTarget<'a> {
    Subject,
    Context(&'a str),
}

/// §7.7 fence action.
#[derive(Debug, Clone, Copy)]
pub(super) enum FenceAction {
    Fence,
    Unfence,
    Archive,
}

/// One §7.7 fence request: what is fenced, the action, and the CAS precondition.
pub(super) struct Fence<'a> {
    pub subject_id: &'a str,
    pub target: FenceTarget<'a>,
    pub action: FenceAction,
    pub expected_revision: i64,
}

pub(super) struct MemoryStore {
    pool: SqlitePool,
    /// Holds this provider's writer admission for the provider's lifetime.
    _guarded: GuardedPool,
    /// Latest provider-clock reading in microseconds since the epoch. A floor,
    /// so retention verdicts cannot be re-derived downwards (§6.10).
    clock_floor: AtomicI64,
    #[cfg(feature = "test-hooks")]
    commit_fault: AtomicU8,
    /// Test-only offset applied to the provider clock (§6.10 retention).
    #[cfg(feature = "test-hooks")]
    receipt_clock_skew: AtomicI64,
    /// Test-only rendezvous a `memory.receipts.get` waits at once it holds its
    /// connection, so a test can move the clock while the read is queued.
    #[cfg(feature = "test-hooks")]
    receipt_read_barrier: std::sync::Mutex<Option<Arc<tokio::sync::Barrier>>>,
}

const SPACE_COLUMNS: &str = "s.space_id, s.display_name, s.authority_host_id, \
     s.authority_generation, s.status, s.revision, s.created_at, \
     (SELECT COUNT(*) FROM memory_receipts r WHERE r.space_id = s.space_id \
      AND r.status = 'pending') AS pending_receipts";

const SUBJECT_COLUMNS: &str = "space_id, subject_id, kind_ns, kind_id, status, \
     fenced_contexts_json, revision, created_at";

const GRANT_COLUMNS: &str = "grant_id, space_id, subject_id, principal_id, \
     capabilities_json, context_ids_json, shared_access, expires_at, revoked_at, \
     revision, created_at";

const RECEIPT_COLUMNS: &str = "space_id, requester_principal_id, op, idempotency_key, \
     receipt_id, payload_digest, status, result_json, error_json, created_at, completed_at";

/// The visibility predicate of §7.2, shared by the page and its marker so both
/// read exactly the same set.
const VISIBLE_SPACE_PREDICATE: &str = "EXISTS ( \
         SELECT 1 FROM memory_grants g \
         WHERE g.space_id = s.space_id AND g.principal_id = ? \
           AND g.revoked_at IS NULL \
           AND (g.expires_at IS NULL OR g.expires_at > ?) \
     )";

impl MemoryStore {
    /// Open an effect transaction and read the authorization clock **after** the
    /// write lock is held.
    ///
    /// `BEGIN IMMEDIATE` can park behind another writer for as long as its
    /// `busy_timeout` allows, so a clock read by the caller beforehand would let a
    /// grant that expires during that wait still authorize the write (§6.5). Every
    /// mutating entry point therefore takes its `now` from here.
    ///
    /// Receipt reclamation runs *before* this transaction, in one of its own:
    /// the caller may still refuse the operation or return early, and dropping
    /// its transaction must not resurrect the bodies the sweep just removed.
    async fn begin_admission(
        &self,
    ) -> StoreResult<(sqlx::Transaction<'_, sqlx::Sqlite>, DateTime<Utc>)> {
        self.reclaim_expired_receipts().await;
        let tx = begin_immediate(&self.pool).await?;
        Ok((tx, self.now()))
    }

    /// §6.10/MEM-R021 reclamation in a transaction of its own, committed before
    /// the operation begins — autonomous of how that operation ends.
    ///
    /// Eviction therefore outlives a replay refusal, an authorization failure,
    /// any other early return, and a caller whose own `COMMIT` fails. It is
    /// best-effort in the opposite direction too: a sweep that cannot run is
    /// logged and never fails or rolls back the operation it precedes — retention
    /// is not part of that operation's contract.
    async fn reclaim_expired_receipts(&self) {
        let mut tx = match begin_immediate(&self.pool).await {
            Ok(tx) => tx,
            Err(error) => {
                tracing::warn!(?error, "memory receipt retention sweep skipped");
                return;
            }
        };
        if let Err(error) = receipt_retention_sweep(&mut tx, self.now()).await {
            tracing::warn!(?error, "memory receipt retention sweep failed");
            return;
        }
        if let Err(error) = tx.commit().await {
            tracing::warn!(?error, "memory receipt retention sweep did not commit");
        }
    }

    /// The provider clock: retention windows (§6.10) are evaluated against it,
    /// and it is the one place a test may move time.
    ///
    /// It never runs backwards: the latest reading is a floor for every later
    /// one, so a wall-clock correction (or a test clock move) cannot re-open a
    /// window that already closed. A receipt body reported past its window stays
    /// reported — retention is one-way, not a projection of the current clock.
    fn now(&self) -> DateTime<Utc> {
        #[cfg(feature = "test-hooks")]
        let observed =
            Utc::now() + chrono::Duration::seconds(self.receipt_clock_skew.load(Ordering::SeqCst));
        #[cfg(not(feature = "test-hooks"))]
        let observed = Utc::now();
        let micros = observed.timestamp_micros();
        let floor = self.clock_floor.fetch_max(micros, Ordering::SeqCst);
        DateTime::from_timestamp_micros(floor.max(micros)).unwrap_or(observed)
    }

    pub(super) async fn open(db_path: &Path, creator_id: &str) -> StoreResult<Self> {
        let guarded = init_guarded_pool(db_path, creator_id).await?;
        let pool = guarded.clone_pool();
        Ok(Self {
            pool,
            _guarded: guarded,
            clock_floor: AtomicI64::new(0),
            #[cfg(feature = "test-hooks")]
            commit_fault: AtomicU8::new(COMMIT_FAULT_NONE),
            #[cfg(feature = "test-hooks")]
            receipt_clock_skew: AtomicI64::new(0),
            #[cfg(feature = "test-hooks")]
            receipt_read_barrier: std::sync::Mutex::new(None),
        })
    }

    /// §6.10/MEM-R021 proof seam: move the provider clock forward so the receipt
    /// retention windows elapse. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn advance_receipt_clock(&self, seconds: i64) {
        self.receipt_clock_skew.fetch_add(seconds, Ordering::SeqCst);
    }

    /// §7.28 proof seam: park the next `memory.receipts.get` after it holds its
    /// connection, so a test can move the clock while the read is queued.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn park_next_receipt_read(&self, barrier: Arc<tokio::sync::Barrier>) {
        *self.receipt_read_barrier.lock().expect("receipt read gate") = Some(barrier);
    }

    #[cfg(feature = "test-hooks")]
    async fn receipt_read_gate(&self) {
        let parked = {
            let mut gate = self.receipt_read_barrier.lock().expect("receipt read gate");
            gate.take()
        };
        if let Some(barrier) = parked {
            barrier.wait().await;
        }
    }

    /// Force the next committing transaction to fail at the commit boundary: a
    /// `false` fault fails before `COMMIT` (no effect), a `true` fault fails after
    /// it (effect committed, response lost). §14.2/§14.3 proof seam only.
    #[cfg(feature = "test-hooks")]
    pub(super) fn inject_commit_fault(&self, after_commit: bool) {
        self.commit_fault.store(
            if after_commit {
                COMMIT_FAULT_AFTER
            } else {
                COMMIT_FAULT_BEFORE
            },
            Ordering::SeqCst,
        );
    }

    /// Operator provisioning (§7.0.6): the local space and the first `admin`
    /// grant exist before any protocol op can be called.
    ///
    /// Both inserts are `INSERT OR IGNORE` keyed on their deterministic ids, so
    /// a re-open neither duplicates nor resets provisioning state.
    pub(super) async fn provision(
        &self,
        space_id: &str,
        authority_host_id: &str,
        creator_id: &str,
        bootstrap_grant_id: &str,
        display_name: Option<&str>,
        now: DateTime<Utc>,
    ) -> StoreResult<()> {
        let at = timestamp(now);
        // §6.10 retention is durable even for a deployment that only reads:
        // opening the provider reclaims what has aged out, and does so in its
        // own committed transaction so a provisioning failure cannot put an
        // already-evicted body back either.
        self.reclaim_expired_receipts().await;
        let mut tx = begin_immediate(&self.pool).await?;
        sqlx::query(
            "INSERT OR IGNORE INTO memory_spaces \
             (space_id, display_name, authority_host_id, authority_generation, status, revision, created_at) \
             VALUES (?, ?, ?, 1, 'active', 1, ?)",
        )
        .bind(space_id)
        .bind(display_name)
        .bind(authority_host_id)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO memory_grants \
             (grant_id, space_id, subject_id, principal_id, capabilities_json, context_ids_json, \
              shared_access, expires_at, revoked_at, revision, created_at) \
             VALUES (?, ?, NULL, ?, '[\"admin\"]', '[]', 0, NULL, NULL, 1, ?)",
        )
        .bind(bootstrap_grant_id)
        .bind(space_id)
        .bind(creator_id)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        tx.commit()
            .await
            .map_err(|e| StoreError::CommitUnknown(e.to_string()))?;
        Ok(())
    }

    pub(super) async fn find_space(&self, space_id: &str) -> StoreResult<Option<SpaceRow>> {
        let sql = format!("SELECT {SPACE_COLUMNS} FROM memory_spaces s WHERE s.space_id = ?");
        Ok(sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// §7.28 `memory.receipts.get`: one stored receipt under **current**
    /// authorization (MEM-R018/R021).
    ///
    /// Disclosure admits the receipt's own requester while they are still a live
    /// participant of the space, or a space-wide `admin` grant — a revoked
    /// principal reads nothing, not even their own receipt. Current participation
    /// is required *before* the id is looked up, so a non-participant gets one
    /// refusal whether or not the receipt exists. Read-only (MEM-R011): this
    /// writes nothing — retained rows are reclaimed by mutating entry points —
    /// so an evicted body is reported, never repaired here. Its verdict is
    /// one-way all the same: [`Self::now`] never runs backwards, so a body this
    /// read reports past its retention window is not served again by a later
    /// read after a clock correction, and a tombstone past the dedup window
    /// answers `NOT_FOUND` without needing a sweep to remove it first.
    pub(super) async fn receipt(
        &self,
        principal_id: &str,
        space_id: &str,
        receipt_id: &str,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        #[cfg(feature = "test-hooks")]
        self.receipt_read_gate().await;
        // §6.5: the authorization clock is read once the connection is held, so a
        // grant that expires while this read waits in the pool queue cannot
        // authorize it.
        let now = self.now();
        space_in_tx(&mut tx, space_id).await?;
        let grant_rows = grants_in_tx(&mut tx, space_id).await?;
        let at = timestamp(now);
        let participant = grant_rows.iter().any(|grant| {
            grant.principal_id == principal_id
                && grant.revoked_at.is_none()
                && grant
                    .expires_at
                    .as_deref()
                    .is_none_or(|expires_at| expires_at > at.as_str())
        });
        // MEM-R018: the refusal is identical for an unknown id and for an
        // existing receipt this principal may not read, so receipt existence is
        // not enumerable without current participation.
        let refusal = || {
            StoreError::NotAuthorized(format!(
                "principal {principal_id} holds no current authorization for receipt {receipt_id}"
            ))
        };
        if !participant {
            return Err(refusal());
        }
        let sql = format!(
            "SELECT {RECEIPT_COLUMNS} FROM memory_receipts WHERE receipt_id = ? AND space_id = ?"
        );
        let row = sqlx::query_as::<_, ReceiptRow>(sqlx::AssertSqlSafe(sql))
            .bind(receipt_id)
            .bind(space_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("receipt {receipt_id}")))?;
        let admin = GrantRequirement {
            capability: "admin",
            subject_id: None,
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        };
        let requester = row.requester_principal_id == principal_id;
        let was_admin = covered(
            &wire_grants(&grant_rows)?,
            principal_id,
            space_id,
            now,
            &admin,
        );
        if !requester && !was_admin {
            return Err(refusal());
        }
        match retention(&row, now)? {
            Retention::Retained => {
                tx.commit().await?;
                Ok(row)
            }
            Retention::Evicted => Err(StoreError::ReceiptExpired {
                last_known_status: row.status,
            }),
            // Past the dedup window even the tombstone is gone.
            Retention::Expired => Err(StoreError::NotFound(format!("receipt {receipt_id}"))),
        }
    }

    pub(super) async fn find_subject(
        &self,
        space_id: &str,
        subject_id: &str,
    ) -> StoreResult<Option<SubjectRow>> {
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM memory_subjects WHERE space_id = ? AND subject_id = ?"
        );
        Ok(sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// The visible spaces AND their invalidation marker from one consistent read
    /// (§7.0.7, MEM-R026). The marker is the sum of the visible revisions, which
    /// only grows while the set is unchanged and moves when a space drifts in or
    /// out of visibility.
    pub(super) async fn visible_spaces(
        &self,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<(Vec<SpaceRow>, String)> {
        let mut tx = self.pool.begin().await?;
        let rows_sql = format!(
            "SELECT {SPACE_COLUMNS} FROM memory_spaces s \
             WHERE {VISIBLE_SPACE_PREDICATE} \
             ORDER BY s.space_id"
        );
        let rows = sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(rows_sql))
            .bind(principal_id)
            .bind(timestamp(now))
            .fetch_all(&mut *tx)
            .await?;
        let marker_sql = format!(
            "SELECT COALESCE(SUM(s.revision), 0) FROM memory_spaces s \
             WHERE {VISIBLE_SPACE_PREDICATE}"
        );
        let (marker,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(marker_sql))
            .bind(principal_id)
            .bind(timestamp(now))
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok((rows, marker.to_string()))
    }

    /// §7.3/§7.4 freeze-state change. `frozen`/`active` no-ops succeed without
    /// bumping the revision (§7.0.4).
    pub(super) async fn set_space_status(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        expected_revision: i64,
        status: &str,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let current = require_space_revision(&mut tx, ctx.space_id, expected_revision).await?;
        let revision = if current.status == status {
            current.revision
        } else {
            let updated = sqlx::query(
                "UPDATE memory_spaces SET status = ?, revision = revision + 1 \
                 WHERE space_id = ? AND revision = ?",
            )
            .bind(status)
            .bind(ctx.space_id)
            .bind(current.revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict {
                    current: current.revision,
                });
            }
            current.revision + 1
        };
        let pending = pending_receipts(&mut tx, ctx.space_id).await?;
        let result = json!({
            "status": status,
            "pending_receipts": pending,
            "revision": revision,
        });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.5 get-or-create. `kind` is creation-only metadata: an existing subject
    /// is returned unchanged whatever kind the caller now names. A fenced or
    /// archived subject rejects the write in the same transaction (§7.5/§7.7).
    pub(super) async fn ensure_subject(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        subject_id: &str,
        kind: Option<(&str, &str)>,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let existing = subject_in_tx(&mut tx, ctx.space_id, subject_id).await?;
        let (created, status, revision) = if let Some(row) = existing {
            (false, row.status, row.revision)
        } else {
            sqlx::query(
                "INSERT INTO memory_subjects \
                 (space_id, subject_id, kind_ns, kind_id, status, fenced_contexts_json, revision, created_at) \
                 VALUES (?, ?, ?, ?, 'active', '[]', 1, ?)",
            )
            .bind(ctx.space_id)
            .bind(subject_id)
            .bind(kind.map(|(ns, _)| ns))
            .bind(kind.map(|(_, id)| id))
            .bind(timestamp(now))
            .execute(&mut *tx)
            .await?;
            (true, "active".to_string(), 1)
        };
        let result = json!({
            "subject_id": subject_id,
            "created": created,
            "status": status,
            "revision": revision,
        });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.7 fence state machine. Only the listed transitions are admitted; the
    /// "already in the requested state" arms are no-ops that keep the revision
    /// (§7.0.4), and `unfence` of a non-fenced target is
    /// `INVALID_STATUS_TRANSITION` as §7.7 names explicitly.
    pub(super) async fn fence_subject(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        fence: Fence<'_>,
    ) -> StoreResult<ReceiptRow> {
        let Fence {
            subject_id,
            target,
            action,
            expected_revision,
        } = fence;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let current = subject_in_tx(&mut tx, ctx.space_id, subject_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("subject {subject_id}")))?;
        if current.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: current.revision,
            });
        }
        if current.status == "archived" {
            return Err(StoreError::StatusTransition(format!(
                "subject {subject_id} is archived (terminal)"
            )));
        }
        let (status, fenced_contexts, changed) =
            next_fence_state(&current, &target, action, subject_id)?;
        let revision = if changed {
            let contexts_json = serde_json::to_string(&fenced_contexts)
                .map_err(|e| StoreError::Corrupt(format!("fenced contexts: {e}")))?;
            let updated = sqlx::query(
                "UPDATE memory_subjects SET status = ?, fenced_contexts_json = ?, \
                 revision = revision + 1 WHERE space_id = ? AND subject_id = ? AND revision = ?",
            )
            .bind(&status)
            .bind(&contexts_json)
            .bind(ctx.space_id)
            .bind(subject_id)
            .bind(current.revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict {
                    current: current.revision,
                });
            }
            current.revision + 1
        } else {
            current.revision
        };
        let result = json!({
            "subject_id": subject_id,
            "status": status,
            "fenced_contexts": fenced_contexts,
            "revision": revision,
        });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    pub(super) async fn grants(
        &self,
        space_id: &str,
        subject_id: Option<&str>,
        principal_id: Option<&str>,
    ) -> StoreResult<Vec<GrantRow>> {
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM memory_grants \
             WHERE space_id = ? AND (? IS NULL OR subject_id = ?) \
             AND (? IS NULL OR principal_id = ?) \
             ORDER BY grant_id"
        );
        Ok(sqlx::query_as::<_, GrantRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .bind(subject_id)
            .bind(principal_id)
            .bind(principal_id)
            .fetch_all(&self.pool)
            .await?)
    }

    /// One filtered grant page AND its invalidation marker from the same read
    /// (§7.0.7, MEM-R026): a grant write that changes the marker between pages
    /// invalidates the cursor. Revisions only grow, so the sum over the space
    /// moves on every insert or revoke.
    pub(super) async fn grants_page(
        &self,
        space_id: &str,
        subject_id: Option<&str>,
        principal_id: Option<&str>,
    ) -> StoreResult<(Vec<GrantRow>, String)> {
        let mut tx = self.pool.begin().await?;
        let rows_sql = format!(
            "SELECT {GRANT_COLUMNS} FROM memory_grants \
             WHERE space_id = ? AND (? IS NULL OR subject_id = ?) \
             AND (? IS NULL OR principal_id = ?) \
             ORDER BY grant_id"
        );
        let rows = sqlx::query_as::<_, GrantRow>(sqlx::AssertSqlSafe(rows_sql))
            .bind(space_id)
            .bind(subject_id)
            .bind(subject_id)
            .bind(principal_id)
            .bind(principal_id)
            .fetch_all(&mut *tx)
            .await?;
        let (marker,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(revision), 0) FROM memory_grants WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((rows, marker.to_string()))
    }

    /// §7.9 issuance. Current-coverage authorization, the dedup probe, the
    /// non-amplification check, the subject lifecycle gate and the insert are one
    /// transaction (§7.3/§11), in that order.
    pub(super) async fn insert_grant(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        new_grant: &NewGrant<'_>,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        // §7.0.5/MEM-R021: handing back a committed receipt is a disclosure, so
        // it needs current coverage — but it never re-runs the issuance rules.
        // An applicable admin grant acquired after the original issuance can
        // tighten the §7.9 expiry bound, and the retry must still return its
        // receipt rather than `NOT_AUTHORIZED`.
        let grants = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        authorize_issuance(&grants, admission, now)?;
        gate(&mut tx, admission).await?;
        let capabilities = serde_json::to_string(new_grant.capabilities)
            .map_err(|e| StoreError::Corrupt(format!("grant capabilities: {e}")))?;
        let context_ids = serde_json::to_string(new_grant.context_ids)
            .map_err(|e| StoreError::Corrupt(format!("grant contexts: {e}")))?;
        sqlx::query(
            "INSERT INTO memory_grants \
             (grant_id, space_id, subject_id, principal_id, capabilities_json, context_ids_json, \
              shared_access, expires_at, revoked_at, revision, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?)",
        )
        .bind(new_grant.grant_id)
        .bind(new_grant.space_id)
        .bind(new_grant.subject_id)
        .bind(new_grant.principal_id)
        .bind(&capabilities)
        .bind(&context_ids)
        .bind(i64::from(new_grant.shared_access))
        .bind(new_grant.expires_at.map(timestamp))
        .bind(timestamp(now))
        .execute(&mut *tx)
        .await?;
        let result = json!({
            "grant_id": new_grant.grant_id,
            "revision": 1,
        });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.10 terminal revoke. A repeated revoke is a no-op success: the grant is
    /// already terminal, and §7.0.4 forbids inventing a revision bump for it.
    pub(super) async fn revoke_grant(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        grant_id: &str,
        expected_revision: i64,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let current: Option<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT revision, space_id, revoked_at FROM memory_grants WHERE grant_id = ?",
        )
        .bind(grant_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (revision, space_id, revoked_at) =
            current.ok_or_else(|| StoreError::NotFound(format!("grant {grant_id}")))?;
        if space_id != ctx.space_id {
            return Err(StoreError::NotFound(format!("grant {grant_id}")));
        }
        if revision != expected_revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        let revision = if revoked_at.is_some() {
            revision
        } else {
            let updated = sqlx::query(
                "UPDATE memory_grants SET revoked_at = ?, revision = revision + 1 \
                 WHERE grant_id = ? AND revision = ?",
            )
            .bind(timestamp(now))
            .bind(grant_id)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict { current: revision });
            }
            revision + 1
        };
        let result = json!({
            "grant_id": grant_id,
            "revoked": true,
            "revision": revision,
        });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// Write the receipt as the last statement of the effect transaction, then
    /// commit. A `COMMIT` failure leaves the outcome unknown (§14.3).
    async fn commit_receipt(
        &self,
        tx: sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &OpContext<'_>,
        result: &Value,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        // Candidate consumption writes the receipt id into the audit shell
        // before this runs (§6.7 `consumed_by`), so those ops pass their own.
        self.commit_receipt_with(tx, ctx, mint_receipt_id(), result, now)
            .await
    }

    /// [`Self::commit_receipt`] for ops that must know the receipt id before the
    /// effect rows that reference it are written.
    async fn commit_receipt_with(
        &self,
        mut tx: sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &OpContext<'_>,
        receipt_id: String,
        result: &Value,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let at = timestamp(now);
        let result_json = serde_json::to_string(result)
            .map_err(|e| StoreError::Corrupt(format!("receipt result: {e}")))?;
        sqlx::query(
            "INSERT INTO memory_receipts \
             (space_id, requester_principal_id, op, idempotency_key, receipt_id, payload_digest, \
              status, result_json, error_json, created_at, completed_at) \
             VALUES (?, ?, ?, ?, ?, ?, 'committed', ?, NULL, ?, ?)",
        )
        .bind(ctx.space_id)
        .bind(ctx.requester_principal_id)
        .bind(ctx.op)
        .bind(ctx.idempotency_key)
        .bind(&receipt_id)
        .bind(ctx.payload_digest)
        .bind(&result_json)
        .bind(&at)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        #[cfg(feature = "test-hooks")]
        let fault = self.commit_fault.swap(COMMIT_FAULT_NONE, Ordering::SeqCst);
        #[cfg(feature = "test-hooks")]
        if fault == COMMIT_FAULT_BEFORE {
            return Err(StoreError::CommitUnknown(
                "injected failure before COMMIT".to_string(),
            ));
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::CommitUnknown(format!("COMMIT outcome unknown: {e}")))?;
        #[cfg(feature = "test-hooks")]
        if fault == COMMIT_FAULT_AFTER {
            return Err(StoreError::CommitUnknown(
                "injected failure after COMMIT".to_string(),
            ));
        }
        Ok(ReceiptRow {
            space_id: ctx.space_id.to_string(),
            requester_principal_id: ctx.requester_principal_id.to_string(),
            op: ctx.op.to_string(),
            idempotency_key: ctx.idempotency_key.map(str::to_string),
            receipt_id,
            payload_digest: ctx.payload_digest.to_string(),
            status: "committed".to_string(),
            result_json: Some(result_json),
            error_json: None,
            created_at: at.clone(),
            completed_at: Some(at),
        })
    }
}

/// §6.5 coverage, evaluated inside the effect transaction: the grant set cannot
/// change between this check and the write.
///
/// Runs *before* the dedup probe, because handing back an already committed
/// receipt is itself a disclosure that current coverage must still admit
/// (§7.0.5). §7.9 non-amplification is a fresh-effect rule and lives in
/// [`authorize_issuance`], behind the probe.
async fn authorize(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    admission: &Admission<'_>,
    now: DateTime<Utc>,
) -> StoreResult<Vec<GrantRow>> {
    space_in_tx(tx, admission.space_id).await?;
    let rows = grants_in_tx(tx, admission.space_id).await?;
    let grants = wire_grants(&rows)?;
    let check = |requirement: &GrantRequirement<'_>| -> StoreResult<()> {
        if covered(
            &grants,
            admission.principal_id,
            admission.space_id,
            now,
            requirement,
        ) {
            return Ok(());
        }
        Err(StoreError::NotAuthorized(format!(
            "principal {} holds no grant covering {} in space {}",
            admission.principal_id, requirement.capability, admission.space_id
        )))
    };
    match &admission.coverage {
        Coverage::Requirements(requirements) => {
            for requirement in *requirements {
                check(requirement)?;
            }
        }
        Coverage::Candidate {
            candidate_id,
            capabilities,
        } => {
            let candidate = candidate_in_tx(tx, admission.space_id, candidate_id)
                .await?
                .ok_or_else(|| StoreError::NotFound(format!("candidate {candidate_id}")))?;
            for capability in *capabilities {
                check(&candidate.requirement(capability))?;
            }
        }
        Coverage::Record {
            record_id,
            capabilities,
            target,
        } => {
            let record = record_in_tx(tx, admission.space_id, record_id)
                .await?
                .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
            for capability in *capabilities {
                check(&record.requirement(capability, false))?;
                if let Some(target) = target {
                    check(&GrantRequirement {
                        capability,
                        subject_id: Some(record.subject_id.as_str()),
                        context_ids: target.context_id.map(str::to_string).into_iter().collect(),
                        reads_shared: target.visibility == Placement::SHARED,
                        subject_all: false,
                    })?;
                }
            }
        }
    }
    Ok(rows)
}

/// §7.9 non-amplification over the same grant set the coverage check read. Only
/// a fresh issuance can amplify granter coverage, so this runs on the fresh
/// effect path and never on a replay (§7.0.5/MEM-R021).
fn authorize_issuance(
    rows: &[GrantRow],
    admission: &Admission<'_>,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let Some(issued) = admission.issuance else {
        return Ok(());
    };
    let mut grants = Vec::with_capacity(rows.len());
    for row in rows {
        grants.push(row.wire()?);
    }
    if let SpokeResult::Reject(reject) = validate_grant_issuance(
        &grants,
        admission.principal_id,
        admission.space_id,
        now,
        issued,
    ) {
        return Err(StoreError::NotAuthorized(reject.message));
    }
    Ok(())
}

/// Lifecycle gates that apply only to a fresh effect — never to a replay
/// (§7.0.5/MEM-R021): the space freeze allowlist (§8.2) and the subject
/// `fenced`/`archived` write gate, including its fenced-context set
/// (§7.5/§7.7/§7.9/MEM-R024).
async fn gate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    admission: &Admission<'_>,
) -> StoreResult<()> {
    if admission.freeze_gated {
        let space = space_in_tx(tx, admission.space_id).await?;
        if space.status == "frozen" && !is_freeze_allowlisted(admission.op) {
            return Err(StoreError::SpaceFrozen(format!(
                "space {} is frozen; {} is outside the maintenance allowlist",
                space.space_id, admission.op
            )));
        }
    }
    if let Some(subject_id) = admission.subject_gate {
        if let Some(subject) = subject_in_tx(tx, admission.space_id, subject_id).await? {
            require_write_allowed(&subject, admission.placement_gate.as_slice())?;
        }
    }
    Ok(())
}

/// §7.7/MEM-R024: a `fenced`/`archived` subject rejects the write, and so does a
/// placement into any context it has fenced. Sibling contexts and shared
/// placement stay usable — a context fence never collaterally fences the
/// subject.
fn require_write_allowed(subject: &SubjectRow, placements: &[Placement<'_>]) -> StoreResult<()> {
    if subject.status != "active" {
        return Err(StoreError::SubjectFenced {
            subject_id: subject.subject_id.clone(),
            status: subject.status.clone(),
            context_id: None,
        });
    }
    let fenced_contexts = subject.fenced_contexts()?;
    for placement in placements {
        let Some(context_id) = placement.context_id else {
            continue;
        };
        if fenced_contexts.iter().any(|known| known == context_id) {
            return Err(StoreError::SubjectFenced {
                subject_id: subject.subject_id.clone(),
                status: subject.status.clone(),
                context_id: Some(context_id.to_string()),
            });
        }
    }
    Ok(())
}

/// The §7.7 transition outcome: resulting status, fenced-context set, and whether
/// anything changed (a genuine no-op keeps the revision, §7.0.4).
fn next_fence_state(
    current: &SubjectRow,
    target: &FenceTarget<'_>,
    action: FenceAction,
    subject_id: &str,
) -> StoreResult<(String, Vec<String>, bool)> {
    let mut fenced_contexts = current.fenced_contexts()?;
    let mut status = current.status.clone();
    let mut changed = false;
    match (target, action) {
        // Already fenced is a no-op success (§7.0.4).
        (FenceTarget::Subject, FenceAction::Fence) => {
            if current.status == "active" {
                status = "fenced".to_string();
                changed = true;
            }
        }
        (FenceTarget::Subject, FenceAction::Unfence) => {
            if current.status != "fenced" {
                return Err(StoreError::StatusTransition(format!(
                    "subject {subject_id} is not fenced"
                )));
            }
            status = "active".to_string();
            changed = true;
        }
        (FenceTarget::Subject, FenceAction::Archive) => {
            status = "archived".to_string();
            changed = true;
        }
        (FenceTarget::Context(context_id), FenceAction::Fence) => {
            if !fenced_contexts.iter().any(|known| known == context_id) {
                fenced_contexts.push((*context_id).to_string());
                changed = true;
            }
        }
        (FenceTarget::Context(context_id), FenceAction::Unfence) => {
            let before = fenced_contexts.len();
            fenced_contexts.retain(|known| known != context_id);
            if fenced_contexts.len() == before {
                return Err(StoreError::StatusTransition(format!(
                    "context {context_id} is not fenced on subject {subject_id}"
                )));
            }
            changed = true;
        }
        // §7.7: archive on a context target is INVALID_INPUT, refused by the
        // request validator before this layer.
        (FenceTarget::Context(_), FenceAction::Archive) => {
            return Err(StoreError::StatusTransition(
                "archive is not valid on a context target".to_string(),
            ))
        }
    }
    Ok((status, fenced_contexts, changed))
}

/// §6.10/MEM-R021 retention state of one stored receipt at the provider clock.
enum Retention {
    /// Inside `receipt_retention_seconds`: the body is served as stored.
    Retained,
    /// Body evicted, dedup tombstone alive: never re-execute, never serve.
    Evicted,
    /// Past `dedup_retention_seconds`: the namespace is free again.
    Expired,
}

/// Which retention window the receipt is in at `now` (§6.10/MEM-R021).
///
/// The dedup window is decided first: past it the namespace is free again even
/// when the row is still there, so a tombstone left by an earlier sweep (or by a
/// body-less settled row) never answers `RECEIPT_EXPIRED` forever. Inside that
/// window a settled row with no payload left is a tombstone — the body was
/// evicted from the row, so it is never served again whatever the clock now
/// says — and a row that still carries its body is `Retained` until
/// `receipt_retention_seconds` and `Evicted` after.
fn retention(row: &ReceiptRow, now: DateTime<Utc>) -> StoreResult<Retention> {
    let created = chrono::DateTime::parse_from_rfc3339(&row.created_at)
        .map_err(|e| StoreError::Corrupt(format!("memory_receipts.created_at: {e}")))?
        .with_timezone(&Utc);
    let age = now.signed_duration_since(created).num_seconds();
    let receipt_window =
        i64::try_from(super::policy::RECEIPT_RETENTION_SECONDS).unwrap_or(i64::MAX);
    let dedup_window = i64::try_from(super::policy::DEDUP_RETENTION_SECONDS).unwrap_or(i64::MAX);
    Ok(if age > dedup_window {
        Retention::Expired
    } else if row.status != "pending" && row.result_json.is_none() && row.error_json.is_none() {
        Retention::Evicted
    } else if age <= receipt_window {
        Retention::Retained
    } else {
        Retention::Evicted
    })
}

/// `window` seconds before `now` in the stored [`timestamp`] rendering, so the
/// sweep's range predicate and [`retention`]'s age comparison cut on the same
/// second.
fn retention_cutoff(now: DateTime<Utc>, window: u64) -> String {
    let seconds = i64::try_from(window).unwrap_or(i64::MAX);
    timestamp(now - chrono::Duration::seconds(seconds))
}

/// §6.10/MEM-R021 reclamation in one transaction.
///
/// Evicts every body older than `receipt_retention_seconds` across all dedup
/// namespaces and the keyless `memory.subjects.ensure` receipts too, then drops
/// every tombstone older than `dedup_retention_seconds`. Eviction is a one-way
/// rewrite — the payload is discarded, so a later clock move cannot resurrect
/// it. [`MemoryStore::reclaim_expired_receipts`] owns the transaction and its
/// commit; this function only writes the reclamation into it.
async fn receipt_retention_sweep(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    sqlx::query(
        "UPDATE memory_receipts SET result_json = NULL, error_json = NULL \
         WHERE created_at < ? AND (result_json IS NOT NULL OR error_json IS NOT NULL)",
    )
    .bind(retention_cutoff(
        now,
        super::policy::RECEIPT_RETENTION_SECONDS,
    ))
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM memory_receipts WHERE created_at < ?")
        .bind(retention_cutoff(
            now,
            super::policy::DEDUP_RETENTION_SECONDS,
        ))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Probe the dedup namespace inside the caller's transaction.
///
/// A keyless op (the `memory.subjects.ensure` exemption) binds NULL, which
/// SQLite never matches, so it never replays and always records a fresh
/// receipt.
///
/// A hit is judged by payload first and retention second: the namespace binds
/// the key to its `payload_digest` (§7.0.5), so a different payload is
/// `IDEMPOTENCY_CONFLICT` at any age. A true replay is only `RECEIPT_EXPIRED`
/// (terminal, never re-executed) once its body is gone, and a namespace past the
/// dedup window is cleared here so the key starts over (§6.10/MEM-R021).
async fn probe(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ctx: &OpContext<'_>,
    now: DateTime<Utc>,
) -> StoreResult<Option<ReceiptRow>> {
    let sql = format!(
        "SELECT {RECEIPT_COLUMNS} FROM memory_receipts \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?"
    );
    let existing = sqlx::query_as::<_, ReceiptRow>(sqlx::AssertSqlSafe(sql))
        .bind(ctx.space_id)
        .bind(ctx.requester_principal_id)
        .bind(ctx.op)
        .bind(ctx.idempotency_key)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = existing else {
        return Ok(None);
    };
    let same_payload = row.payload_digest == ctx.payload_digest;
    match retention(&row, now)? {
        Retention::Retained if same_payload => Ok(Some(row)),
        Retention::Retained | Retention::Evicted => {
            if same_payload {
                Err(StoreError::ReceiptExpired {
                    last_known_status: row.status,
                })
            } else {
                Err(StoreError::IdempotencyConflict)
            }
        }
        Retention::Expired => {
            // The sweep in `begin_admission` normally cleared it already; the
            // delete stays here so a probe can never inherit a key the partial
            // unique index still holds.
            sqlx::query("DELETE FROM memory_receipts WHERE receipt_id = ?")
                .bind(&row.receipt_id)
                .execute(&mut **tx)
                .await?;
            Ok(None)
        }
    }
}

async fn space_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
) -> StoreResult<SpaceRow> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM memory_spaces s WHERE s.space_id = ?");
    sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| StoreError::NotFound(format!("space {space_id}")))
}

async fn grants_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
) -> StoreResult<Vec<GrantRow>> {
    let sql =
        format!("SELECT {GRANT_COLUMNS} FROM memory_grants WHERE space_id = ? ORDER BY grant_id");
    Ok(sqlx::query_as::<_, GrantRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .fetch_all(&mut **tx)
        .await?)
}

async fn subject_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Option<SubjectRow>> {
    let sql = format!(
        "SELECT {SUBJECT_COLUMNS} FROM memory_subjects WHERE space_id = ? AND subject_id = ?"
    );
    Ok(sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(subject_id)
        .fetch_optional(&mut **tx)
        .await?)
}

async fn require_space_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    expected_revision: i64,
) -> StoreResult<SpaceRow> {
    let row = space_in_tx(tx, space_id).await?;
    if row.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            current: row.revision,
        });
    }
    Ok(row)
}

async fn pending_receipts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
) -> StoreResult<u64> {
    let (pending,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_receipts WHERE space_id = ? AND status = 'pending'",
    )
    .bind(space_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(u64::try_from(pending).unwrap_or(0))
}

// ── The content families (§6.6/§6.7/§6.8, §7.11–§7.24) ──────────────────
//
// The content tables carry the same control metadata as the round-A control
// tables: `revision` as the CAS surface (MEM-R007), a stable provider-minted
// identity that is never reused, and the provider's writer admission on every
// mutation. Reads are one transaction so the page and its invalidation marker
// observe the same snapshot, and authorization filters rows *before* the page
// is cut (MEM-R006).

const CANDIDATE_COLUMNS: &str = "candidate_id, space_id, subject_id, capture_visibility, \
     capture_context_id, kind, content_json, provenance_json, classification_json, status, \
     consumed_by_json, content_digest, occurred_at, expires_at, revision, created_at";

const RECORD_COLUMNS: &str = "record_id, space_id, subject_id, kind, content_json, \
     scope_visibility, scope_context_id, provenance_json, import_ref_json, occurred_at, \
     tombstone_json, revision, created_at, updated_at";

const PROFILE_COLUMNS: &str = "space_id, subject_id, profile_kind, document_json, updated_by, \
     revision, created_at, updated_at";

/// One placement (§6.6/§6.7 `scope` / `capture_scope`) as the tables store it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placement<'a> {
    pub visibility: &'a str,
    pub context_id: Option<&'a str>,
}

impl Placement<'_> {
    pub(super) const SHARED: &'static str = "shared";
    pub(super) const CONTEXT: &'static str = "context";
}

/// The §6.12 read selection, as the read path needs it.
pub(super) enum Selection {
    SubjectAll,
    SharedOnly,
    SelectedContexts {
        context_ids: Vec<String>,
        include_shared: bool,
    },
}

impl Selection {
    /// Coverage the *whole* read needs (§6.5): every named context, shared access
    /// when shared rows are in scope, and unrestricted coverage for `subject_all`.
    fn base_requirement<'a>(
        &'a self,
        capability: &'a str,
        subject_id: &'a str,
    ) -> GrantRequirement<'a> {
        match self {
            Self::SubjectAll => GrantRequirement {
                capability,
                subject_id: Some(subject_id),
                context_ids: Vec::new(),
                reads_shared: false,
                subject_all: true,
            },
            Self::SharedOnly => GrantRequirement {
                capability,
                subject_id: Some(subject_id),
                context_ids: Vec::new(),
                reads_shared: true,
                subject_all: false,
            },
            Self::SelectedContexts {
                context_ids,
                include_shared,
            } => GrantRequirement {
                capability,
                subject_id: Some(subject_id),
                context_ids: context_ids.clone(),
                reads_shared: *include_shared,
                subject_all: false,
            },
        }
    }

    /// Does the selection include a row with this placement (§6.12)?
    fn admits(&self, placement: &Placement<'_>) -> bool {
        match self {
            Self::SubjectAll => true,
            Self::SharedOnly => placement.visibility == Placement::SHARED,
            Self::SelectedContexts {
                context_ids,
                include_shared,
            } => placement.context_id.map_or_else(
                || placement.visibility == Placement::SHARED && *include_shared,
                |context_id| context_ids.iter().any(|known| known == context_id),
            ),
        }
    }
}

/// Which read op is asking for records (§7.20 vs §7.22): the two differ in the
/// capability they demand and in whether §7.7's archive exclusion applies.
#[derive(Debug, Clone, Copy)]
pub(super) enum RecordRead {
    List,
    Recall,
}

impl RecordRead {
    /// §7.20 accepts either read capability; §7.22 requires `recall` itself.
    const fn capabilities(self) -> &'static [&'static str] {
        match self {
            Self::List => &["records_read", "recall"],
            Self::Recall => &["recall"],
        }
    }

    /// §7.7: `archived` excludes the subject's records from recall only.
    const fn excludes_archived(self) -> bool {
        matches!(self, Self::Recall)
    }
}

/// MEM-R026 invalidation marker for a content page: a digest over everything
/// that determines what the page holds — each visible row's identity and
/// revision, every grant revision in the space, and the subject's lifecycle
/// status. A bare revision sum can stay equal while one visible row is swapped
/// for another, and cannot see a grant change that leaves the rows unchanged.
fn snapshot_marker<'a>(
    rows: impl Iterator<Item = (&'a str, i64)>,
    grants: &[GrantRow],
    subject_status: Option<&str>,
) -> StoreResult<String> {
    let mut visible: Vec<(String, i64)> = rows
        .map(|(record_id, revision)| (record_id.to_string(), revision))
        .collect();
    visible.sort();
    let mut grant_revisions: Vec<(String, i64)> = grants
        .iter()
        .map(|grant| (grant.grant_id.clone(), grant.revision))
        .collect();
    grant_revisions.sort();
    let parts = json!({
        "rows": visible,
        "grants": grant_revisions,
        "subject": subject_status,
    });
    match spoke_operations::jcs_sha256_hex(&parts) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(StoreError::Corrupt(reject.message)),
    }
}

/// A pending/consumed review candidate as stored (§6.7).
#[derive(Debug, sqlx::FromRow)]
pub(super) struct CandidateRow {
    pub candidate_id: String,
    pub space_id: String,
    pub subject_id: String,
    pub capture_visibility: String,
    pub capture_context_id: Option<String>,
    pub kind: String,
    pub content_json: Option<String>,
    pub provenance_json: String,
    pub classification_json: String,
    pub status: String,
    pub consumed_by_json: Option<String>,
    pub content_digest: Option<String>,
    pub occurred_at: Option<String>,
    pub expires_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
}

impl CandidateRow {
    pub(super) fn scope(&self) -> Placement<'_> {
        Placement {
            visibility: self.capture_visibility.as_str(),
            context_id: self.capture_context_id.as_deref(),
        }
    }

    /// §6.7: an expired *pending* candidate leaves the lists without being
    /// consumed; a consumed shell keeps its audit visibility.
    fn expired(&self, now: DateTime<Utc>) -> bool {
        self.status == "pending"
            && self
                .expires_at
                .as_deref()
                .is_some_and(|expires_at| expires_at <= timestamp(now).as_str())
    }

    fn requirement<'a>(&'a self, capability: &'a str) -> GrantRequirement<'a> {
        let scope = self.scope();
        GrantRequirement {
            capability,
            subject_id: Some(self.subject_id.as_str()),
            context_ids: scope.context_id.map(str::to_string).into_iter().collect(),
            reads_shared: scope.visibility == Placement::SHARED,
            subject_all: false,
        }
    }
}

/// A retained record as stored (§6.6).
#[derive(Debug, sqlx::FromRow)]
pub(super) struct RecordRow {
    pub record_id: String,
    pub space_id: String,
    pub subject_id: String,
    pub kind: String,
    pub content_json: String,
    pub scope_visibility: String,
    pub scope_context_id: Option<String>,
    pub provenance_json: String,
    pub import_ref_json: Option<String>,
    pub occurred_at: Option<String>,
    pub tombstone_json: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl RecordRow {
    pub(super) fn scope(&self) -> Placement<'_> {
        Placement {
            visibility: self.scope_visibility.as_str(),
            context_id: self.scope_context_id.as_deref(),
        }
    }

    pub(super) const fn tombstoned(&self) -> bool {
        self.tombstone_json.is_some()
    }

    fn requirement<'a>(&'a self, capability: &'a str, subject_all: bool) -> GrantRequirement<'a> {
        let scope = self.scope();
        GrantRequirement {
            capability,
            subject_id: Some(self.subject_id.as_str()),
            context_ids: scope.context_id.map(str::to_string).into_iter().collect(),
            reads_shared: scope.visibility == Placement::SHARED,
            subject_all,
        }
    }
}

/// A trusted authored subject document as stored (§6.8).
#[derive(Debug, sqlx::FromRow)]
pub(super) struct ProfileRow {
    pub space_id: String,
    pub subject_id: String,
    pub profile_kind: String,
    pub document_json: String,
    pub updated_by: String,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One candidate as `memory.capture` materializes it (§7.11).
pub(super) struct NewCandidate<'a> {
    pub candidate_id: &'a str,
    pub subject_id: &'a str,
    pub placement: Placement<'a>,
    pub kind: &'a str,
    pub content: &'a Value,
    pub provenance: &'a Value,
    pub classification: &'a Value,
    pub occurred_at: Option<&'a str>,
    pub expires_at: Option<&'a str>,
}

/// The §7.15 review decision, reduced to what the write path needs.
pub(super) enum ReviewDecision<'a> {
    Promote {
        kind: Option<&'a str>,
        content: Option<&'a Value>,
    },
    Fragment {
        items: &'a [(String, Value)],
    },
    Drop,
}

impl ReviewDecision<'_> {
    const fn outcome(&self) -> &'static str {
        match self {
            Self::Promote { .. } => "promoted",
            Self::Fragment { .. } => "fragmented",
            Self::Drop => "dropped",
        }
    }
}

fn serialize(value: &Value, what: &str) -> StoreResult<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Corrupt(format!("{what}: {e}")))
}

/// §6.5 coverage over an already-read and already-wired grant set.
fn covered(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirement: &GrantRequirement<'_>,
) -> bool {
    grants
        .iter()
        .any(|grant| grant_covers(grant, principal_id, space_id, now, requirement))
}

fn wire_grants(rows: &[GrantRow]) -> StoreResult<Vec<MemoryGrant>> {
    rows.iter().map(GrantRow::wire).collect()
}

/// §7.5/§7.7/§7.9 write gate for a subject named by a *stored row* rather than
/// the request, including the placement(s) that write touches (MEM-R024).
async fn require_subject_writable(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
    placements: &[Placement<'_>],
) -> StoreResult<()> {
    if let Some(subject) = subject_in_tx(tx, space_id, subject_id).await? {
        require_write_allowed(&subject, placements)?;
    }
    Ok(())
}

async fn candidate_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    candidate_id: &str,
) -> StoreResult<Option<CandidateRow>> {
    let sql = format!(
        "SELECT {CANDIDATE_COLUMNS} FROM memory_candidates \
         WHERE space_id = ? AND candidate_id = ?"
    );
    Ok(sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(candidate_id)
        .fetch_optional(&mut **tx)
        .await?)
}

async fn record_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    record_id: &str,
) -> StoreResult<Option<RecordRow>> {
    let sql =
        format!("SELECT {RECORD_COLUMNS} FROM memory_records WHERE space_id = ? AND record_id = ?");
    Ok(sqlx::query_as::<_, RecordRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(record_id)
        .fetch_optional(&mut **tx)
        .await?)
}

/// §7.14/§7.15 shared consumption prelude: load the shell, gate its subject and
/// its immutable capture placement, then check that it is still pending and at
/// the expected revision.
async fn consumable_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    candidate_id: &str,
    expected_revision: i64,
) -> StoreResult<CandidateRow> {
    let candidate = candidate_in_tx(tx, space_id, candidate_id)
        .await?
        .ok_or_else(|| StoreError::NotFound(format!("candidate {candidate_id}")))?;
    require_subject_writable(
        tx,
        space_id,
        candidate.subject_id.as_str(),
        &[candidate.scope()],
    )
    .await?;
    if candidate.status == "consumed" {
        return Err(StoreError::CandidateConsumed(format!(
            "candidate {candidate_id} was already consumed"
        )));
    }
    if candidate.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            current: candidate.revision,
        });
    }
    Ok(candidate)
}

/// §6.7/§8.1 consumption: scrub the body, stamp `content_digest`, record the
/// outcome and bump the revision in one statement guarded by the CAS.
#[allow(clippy::too_many_arguments)]
async fn consume_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    candidate: &CandidateRow,
    expected_revision: i64,
    consumed_by: &Value,
    content_digest: &str,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let updated = sqlx::query(
        "UPDATE memory_candidates \
         SET status = 'consumed', content_json = NULL, content_digest = ?, consumed_by_json = ?, \
             revision = revision + 1 \
         WHERE space_id = ? AND candidate_id = ? AND revision = ? AND status = 'pending'",
    )
    .bind(content_digest)
    .bind(serialize(consumed_by, "candidate consumption")?)
    .bind(candidate.space_id.as_str())
    .bind(candidate.candidate_id.as_str())
    .bind(expected_revision)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(StoreError::RevisionConflict {
            current: candidate.revision,
        });
    }
    let _ = now;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_record(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
    placement: Placement<'_>,
    kind: &str,
    content: &Value,
    provenance: &Value,
    occurred_at: Option<&str>,
    record_id: &str,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let at = timestamp(now);
    sqlx::query(
        "INSERT INTO memory_records \
         (record_id, space_id, subject_id, kind, content_json, scope_visibility, scope_context_id, \
          provenance_json, import_ref_json, occurred_at, tombstone_json, revision, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, NULL, 1, ?, ?)",
    )
    .bind(record_id)
    .bind(space_id)
    .bind(subject_id)
    .bind(kind)
    .bind(serialize(content, "record content")?)
    .bind(placement.visibility)
    .bind(placement.context_id)
    .bind(serialize(provenance, "record provenance")?)
    .bind(occurred_at)
    .bind(&at)
    .bind(&at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn mint_record_id() -> String {
    format!("rec_{}", Uuid::new_v4().simple())
}

/// Stable candidate identity (§6.7): minted once, never reused.
pub(super) fn mint_candidate_id() -> String {
    format!("cnd_{}", Uuid::new_v4().simple())
}

impl MemoryStore {
    /// §7.11: admission + candidate inserts + receipt, one transaction.
    pub(super) async fn capture(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidates: &[NewCandidate<'_>],
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let at = timestamp(now);
        let mut ids = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            sqlx::query(
                "INSERT INTO memory_candidates \
                 (candidate_id, space_id, subject_id, capture_visibility, capture_context_id, kind, \
                  content_json, provenance_json, classification_json, status, consumed_by_json, \
                  content_digest, occurred_at, expires_at, revision, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?, 1, ?)",
            )
            .bind(candidate.candidate_id)
            .bind(ctx.space_id)
            .bind(candidate.subject_id)
            .bind(candidate.placement.visibility)
            .bind(candidate.placement.context_id)
            .bind(candidate.kind)
            .bind(serialize(candidate.content, "candidate content")?)
            .bind(serialize(candidate.provenance, "candidate provenance")?)
            .bind(serialize(candidate.classification, "candidate classification")?)
            .bind(candidate.occurred_at)
            .bind(candidate.expires_at)
            .bind(&at)
            .execute(&mut *tx)
            .await?;
            ids.push(candidate.candidate_id.to_string());
        }
        let classified = ids.len();
        let result = json!({ "candidate_ids": ids, "classified": classified });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.12: the authorized page and its invalidation marker from one read.
    pub(super) async fn candidates(
        &self,
        space_id: &str,
        subject_id: &str,
        status: Option<&str>,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<(Vec<CandidateRow>, String)> {
        let mut tx = self.pool.begin().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grant_rows = grants_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grant_rows)?;
        let base = GrantRequirement {
            capability: "review",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        };
        if !covered(&grants, principal_id, space_id, now, &base) {
            return Err(StoreError::NotAuthorized(format!(
                "principal {principal_id} holds no review grant on subject {subject_id}"
            )));
        }
        let sql = format!(
            "SELECT {CANDIDATE_COLUMNS} FROM memory_candidates \
             WHERE space_id = ? AND subject_id = ? AND (? IS NULL OR status = ?) \
             ORDER BY created_at, candidate_id"
        );
        let rows = sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .bind(status)
            .bind(status)
            .fetch_all(&mut *tx)
            .await?;
        let subject_status = subject_in_tx(&mut tx, space_id, subject_id)
            .await?
            .map(|subject| subject.status);
        tx.commit().await?;
        // §7.7: `archived` hides the subject from the *default* listing; a
        // status-filtered listing is an explicit audit query and stays readable.
        let archived = status.is_none() && subject_status.as_deref() == Some("archived");
        let visible: Vec<CandidateRow> = if archived {
            Vec::new()
        } else {
            rows.into_iter()
                .filter(|row| !row.expired(now))
                .filter(|row| {
                    covered(
                        &grants,
                        principal_id,
                        space_id,
                        now,
                        &row.requirement("review"),
                    )
                })
                .collect()
        };
        let marker = snapshot_marker(
            visible
                .iter()
                .map(|row| (row.candidate_id.as_str(), row.revision)),
            &grant_rows,
            subject_status.as_deref(),
        )?;
        Ok((visible, marker))
    }

    /// §7.13: by-id read under current authorization (§7.17-style; an
    /// unauthorized candidate is reported as absent).
    pub(super) async fn candidate_for_read(
        &self,
        space_id: &str,
        candidate_id: &str,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<Option<CandidateRow>> {
        let mut tx = self.pool.begin().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = candidate_in_tx(&mut tx, space_id, candidate_id).await?;
        tx.commit().await?;
        Ok(row.filter(|row| {
            covered(
                &grants,
                principal_id,
                space_id,
                now,
                &row.requirement("review"),
            )
        }))
    }

    /// §7.14: dismissal consumes the candidate without producing records.
    pub(super) async fn dismiss_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        expected_revision: i64,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let candidate =
            consumable_candidate(&mut tx, ctx.space_id, candidate_id, expected_revision).await?;
        let receipt_id = mint_receipt_id();
        let consumed_by = json!({
            "op": "memory.candidates.dismiss",
            "receipt_id": receipt_id,
            "outcome": "dismissed",
            "record_ids": [],
        });
        let digest = content_digest(&candidate)?;
        consume_candidate(
            &mut tx,
            &candidate,
            expected_revision,
            &consumed_by,
            &digest,
            now,
        )
        .await?;
        let result = json!({ "candidate_id": candidate_id, "outcome": "dismissed" });
        self.commit_receipt_with(tx, ctx, receipt_id, &result, now)
            .await
    }

    /// §7.15: consumption + scrub + every record insert + receipt, one
    /// transaction (MEM-R009). Placement is inherited from the candidate and can
    /// never be chosen by the decision.
    pub(super) async fn review_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        expected_revision: i64,
        decision: &ReviewDecision<'_>,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let candidate =
            consumable_candidate(&mut tx, ctx.space_id, candidate_id, expected_revision).await?;
        let provenance: Value = serde_json::from_str(&candidate.provenance_json)
            .map_err(|e| StoreError::Corrupt(format!("candidate provenance: {e}")))?;
        let content: Value = match candidate.content_json.as_deref() {
            Some(raw) => serde_json::from_str(raw)
                .map_err(|e| StoreError::Corrupt(format!("candidate content: {e}")))?,
            None => {
                return Err(StoreError::Corrupt(format!(
                    "pending candidate {candidate_id} carries no content"
                )))
            }
        };
        let placement = candidate.scope();
        let mut record_ids = Vec::new();
        match decision {
            ReviewDecision::Promote {
                kind,
                content: override_content,
            } => {
                let record_id = mint_record_id();
                insert_record(
                    &mut tx,
                    ctx.space_id,
                    candidate.subject_id.as_str(),
                    placement,
                    kind.unwrap_or(candidate.kind.as_str()),
                    override_content.unwrap_or(&content),
                    &provenance,
                    candidate.occurred_at.as_deref(),
                    &record_id,
                    now,
                )
                .await?;
                record_ids.push(record_id);
            }
            ReviewDecision::Fragment { items } => {
                for (kind, fragment) in *items {
                    let record_id = mint_record_id();
                    insert_record(
                        &mut tx,
                        ctx.space_id,
                        candidate.subject_id.as_str(),
                        placement,
                        kind.as_str(),
                        fragment,
                        &provenance,
                        candidate.occurred_at.as_deref(),
                        &record_id,
                        now,
                    )
                    .await?;
                    record_ids.push(record_id);
                }
            }
            ReviewDecision::Drop => {}
        }
        let receipt_id = mint_receipt_id();
        let consumed_by = json!({
            "op": "memory.candidates.review",
            "receipt_id": receipt_id,
            "outcome": decision.outcome(),
            "record_ids": record_ids,
        });
        let digest = content_digest(&candidate)?;
        consume_candidate(
            &mut tx,
            &candidate,
            expected_revision,
            &consumed_by,
            &digest,
            now,
        )
        .await?;
        let result = json!({ "outcome": decision.outcome(), "record_ids": record_ids });
        self.commit_receipt_with(tx, ctx, receipt_id, &result, now)
            .await
    }

    /// §7.20/§7.22 read path: authorization and tombstone/selection filters
    /// apply before the page is cut, so counts and cursors cover only what the
    /// principal may see (MEM-R006).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn records(
        &self,
        space_id: &str,
        subject_id: &str,
        selection: &Selection,
        filter: Option<&[String]>,
        kinds: &[String],
        include_tombstoned: bool,
        read: RecordRead,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<(Vec<RecordRow>, String)> {
        let mut tx = self.pool.begin().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grant_rows = grants_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grant_rows)?;
        let required = read.capabilities();
        if !required.iter().any(|capability| {
            covered(
                &grants,
                principal_id,
                space_id,
                now,
                &selection.base_requirement(capability, subject_id),
            )
        }) {
            return Err(StoreError::NotAuthorized(format!(
                "principal {principal_id} holds no {} grant covering the selection",
                required.join("/")
            )));
        }
        let admin = GrantRequirement {
            capability: "admin",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        };
        if include_tombstoned && !covered(&grants, principal_id, space_id, now, &admin) {
            return Err(StoreError::NotAuthorized(
                "include_tombstoned requires admin coverage (§7.20)".to_string(),
            ));
        }
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM memory_records \
             WHERE space_id = ? AND subject_id = ? ORDER BY created_at DESC, record_id"
        );
        let rows = sqlx::query_as::<_, RecordRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .fetch_all(&mut *tx)
            .await?;
        let subject_status = subject_in_tx(&mut tx, space_id, subject_id)
            .await?
            .map(|subject| subject.status);
        tx.commit().await?;
        // §7.7: an archived subject's records leave recall; the listing and
        // by-id audit routes still serve them.
        let archived = read.excludes_archived() && subject_status.as_deref() == Some("archived");
        let subject_all = matches!(selection, Selection::SubjectAll);
        let rows = if archived { Vec::new() } else { rows };
        let visible: Vec<RecordRow> = rows
            .into_iter()
            .filter(|row| !row.tombstoned() || include_tombstoned)
            .filter(|row| kinds.is_empty() || kinds.contains(&row.kind))
            .filter(|row| selection.admits(&row.scope()))
            .filter(|row| {
                filter
                    .is_none_or(|context_ids| provenance_matches(&row.provenance_json, context_ids))
            })
            .filter(|row| {
                required.iter().any(|capability| {
                    covered(
                        &grants,
                        principal_id,
                        space_id,
                        now,
                        &row.requirement(capability, subject_all),
                    )
                })
            })
            .collect();
        let marker = snapshot_marker(
            visible
                .iter()
                .map(|row| (row.record_id.as_str(), row.revision)),
            &grant_rows,
            subject_status.as_deref(),
        )?;
        Ok((visible, marker))
    }

    /// §7.17: by-id read; a tombstoned record is admin-only and an unauthorized
    /// record reads as absent.
    pub(super) async fn record_for_read(
        &self,
        space_id: &str,
        record_id: &str,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<Option<RecordRow>> {
        let mut tx = self.pool.begin().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = record_in_tx(&mut tx, space_id, record_id).await?;
        tx.commit().await?;
        let subject_id = row.as_ref().map(|row| row.subject_id.clone());
        let admin = GrantRequirement {
            capability: "admin",
            subject_id: subject_id.as_deref(),
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        };
        Ok(row.filter(|row| {
            if !covered(
                &grants,
                principal_id,
                space_id,
                now,
                &row.requirement("records_read", false),
            ) && !covered(
                &grants,
                principal_id,
                space_id,
                now,
                &row.requirement("recall", false),
            ) {
                return false;
            }
            !row.tombstoned() || covered(&grants, principal_id, space_id, now, &admin)
        }))
    }

    /// §7.16 direct authored write.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn create_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        subject_id: &str,
        placement: Placement<'_>,
        kind: &str,
        content: &Value,
        provenance: &Value,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let record_id = mint_record_id();
        insert_record(
            &mut tx,
            ctx.space_id,
            subject_id,
            placement,
            kind,
            content,
            provenance,
            None,
            &record_id,
            now,
        )
        .await?;
        let result = json!({ "record_id": record_id, "revision": 1 });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.18: kind/content edit under CAS. Placement is not editable here.
    pub(super) async fn update_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        record_id: &str,
        expected_revision: i64,
        content: Option<&Value>,
        kind: Option<&str>,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let record = record_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
        require_subject_writable(
            &mut tx,
            ctx.space_id,
            record.subject_id.as_str(),
            &[record.scope()],
        )
        .await?;
        // MEM-R014: deletion is terminal, so a tombstoned record is never edited
        // back to life.
        if record.tombstoned() {
            return Err(StoreError::StatusTransition(format!(
                "record {record_id} is tombstoned"
            )));
        }
        if record.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        let content_json = match content {
            Some(content) => Some(serialize(content, "record content")?),
            None => None,
        };
        let updated = sqlx::query(
            "UPDATE memory_records SET content_json = COALESCE(?, content_json), \
             kind = COALESCE(?, kind), updated_at = ?, revision = revision + 1 \
             WHERE space_id = ? AND record_id = ? AND revision = ?",
        )
        .bind(content_json)
        .bind(kind)
        .bind(timestamp(now))
        .bind(ctx.space_id)
        .bind(record_id)
        .bind(record.revision)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        let result = json!({ "record_id": record_id, "revision": record.revision + 1 });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.19/MEM-R014: content removed, tombstone retained, receipts untouched.
    pub(super) async fn delete_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        record_id: &str,
        expected_revision: i64,
        reason: Option<&str>,
        deleted_by: &str,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let record = record_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
        // §7.7/MEM-R014: deletion scrubs content irreversibly, so the subject
        // and placement gates apply exactly as they do to update and promote.
        require_subject_writable(
            &mut tx,
            ctx.space_id,
            record.subject_id.as_str(),
            &[record.scope()],
        )
        .await?;
        if record.tombstoned() {
            return Err(StoreError::StatusTransition(format!(
                "record {record_id} is already tombstoned"
            )));
        }
        if record.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        let tombstone = json!({
            "deleted_at": timestamp(now),
            "deleted_by": deleted_by,
            "reason": reason,
        });
        let updated = sqlx::query(
            "UPDATE memory_records SET content_json = ?, tombstone_json = ?, updated_at = ?, \
             revision = revision + 1 \
             WHERE space_id = ? AND record_id = ? AND revision = ?",
        )
        .bind(serialize(&json!({ "text": "" }), "scrubbed content")?)
        .bind(serialize(&tombstone, "tombstone")?)
        .bind(timestamp(now))
        .bind(ctx.space_id)
        .bind(record_id)
        .bind(record.revision)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        let result = json!({ "record_id": record_id, "tombstoned": true });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.21 explicit placement change; provenance is untouched.
    pub(super) async fn promote_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        record_id: &str,
        expected_revision: i64,
        target: Placement<'_>,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let record = record_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
        require_subject_writable(
            &mut tx,
            ctx.space_id,
            record.subject_id.as_str(),
            &[record.scope(), target],
        )
        .await?;
        if record.tombstoned() {
            return Err(StoreError::StatusTransition(format!(
                "record {record_id} is tombstoned"
            )));
        }
        if record.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        let revision = if record.scope_visibility == target.visibility
            && record.scope_context_id.as_deref() == target.context_id
        {
            // §7.0.4: a genuine no-op keeps the revision.
            record.revision
        } else {
            let updated = sqlx::query(
                "UPDATE memory_records SET scope_visibility = ?, scope_context_id = ?, \
                 updated_at = ?, revision = revision + 1 \
                 WHERE space_id = ? AND record_id = ? AND revision = ?",
            )
            .bind(target.visibility)
            .bind(target.context_id)
            .bind(timestamp(now))
            .bind(ctx.space_id)
            .bind(record_id)
            .bind(record.revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict {
                    current: record.revision,
                });
            }
            record.revision + 1
        };
        let scope = json!({
            "visibility": target.visibility,
            "context_id": target.context_id,
        });
        let result = json!({ "record_id": record_id, "revision": revision, "scope": scope });
        self.commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.23 profile read: every present profile, or the requested kinds that
    /// are present. Absence is honest-empty, never an error.
    pub(super) async fn profiles(
        &self,
        space_id: &str,
        subject_id: &str,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<Option<Vec<ProfileRow>>> {
        let mut tx = self.pool.begin().await?;
        space_in_tx(&mut tx, space_id).await?;
        let rows = grants_in_tx(&mut tx, space_id).await?;
        let visible = rows.iter().any(|grant| {
            let Ok(wire) = grant.wire() else {
                return false;
            };
            profiles_grant_covers(&wire, principal_id, space_id, now, subject_id)
        });
        if !visible {
            tx.commit().await?;
            return Ok(None);
        }
        let sql = format!(
            "SELECT {PROFILE_COLUMNS} FROM memory_profiles \
             WHERE space_id = ? AND subject_id = ? ORDER BY profile_kind"
        );
        let rows = sqlx::query_as::<_, ProfileRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(rows))
    }

    /// §7.24 create-or-update under CAS; `expected == None` is create-only.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn upsert_profile(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        subject_id: &str,
        profile_kind: &str,
        expected_revision: Option<i64>,
        document: &Value,
        updated_by: &str,
    ) -> StoreResult<ReceiptRow> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT revision FROM memory_profiles \
             WHERE space_id = ? AND subject_id = ? AND profile_kind = ?",
        )
        .bind(ctx.space_id)
        .bind(subject_id)
        .bind(profile_kind)
        .fetch_optional(&mut *tx)
        .await?;
        let at = timestamp(now);
        let revision = match (existing, expected_revision) {
            (None, None) => {
                sqlx::query(
                    "INSERT INTO memory_profiles \
                     (space_id, subject_id, profile_kind, document_json, updated_by, revision, \
                      created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
                )
                .bind(ctx.space_id)
                .bind(subject_id)
                .bind(profile_kind)
                .bind(serialize(document, "profile document")?)
                .bind(updated_by)
                .bind(&at)
                .bind(&at)
                .execute(&mut *tx)
                .await?;
                1
            }
            (None, Some(_)) => {
                // Create-only was not requested but the row does not exist: the
                // expectation cannot be met.
                return Err(StoreError::RevisionConflict { current: 0 });
            }
            (Some((current,)), None) => {
                // `expected_revision: null` means create-only (§7.24).
                return Err(StoreError::RevisionConflict { current });
            }
            (Some((current,)), Some(expected)) => {
                if current != expected {
                    return Err(StoreError::RevisionConflict { current });
                }
                let updated = sqlx::query(
                    "UPDATE memory_profiles SET document_json = ?, updated_by = ?, \
                     updated_at = ?, revision = revision + 1 \
                     WHERE space_id = ? AND subject_id = ? AND profile_kind = ? AND revision = ?",
                )
                .bind(serialize(document, "profile document")?)
                .bind(updated_by)
                .bind(&at)
                .bind(ctx.space_id)
                .bind(subject_id)
                .bind(profile_kind)
                .bind(current)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() != 1 {
                    return Err(StoreError::RevisionConflict { current });
                }
                current + 1
            }
        };
        let result = json!({ "profile_kind": profile_kind, "revision": revision });
        self.commit_receipt(tx, ctx, &result, now).await
    }
}

/// §6.13: at least one provenance source context is in the filter list.
fn provenance_matches(provenance_json: &str, context_ids: &[String]) -> bool {
    let Ok(provenance) = serde_json::from_str::<Value>(provenance_json) else {
        return false;
    };
    provenance
        .get("source_context_ids")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources.iter().any(|source| {
                source
                    .as_str()
                    .is_some_and(|source| context_ids.iter().any(|known| known == source))
            })
        })
}

/// §7.23 coverage: any capability on the subject, with shared access or
/// unrestricted coverage (profiles are subject-level trusted documents).
fn profiles_grant_covers(
    grant: &MemoryGrant,
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    subject_id: &str,
) -> bool {
    // Every capability the grant lists is admitted by the same subject/space
    // filter, so probing the grant's own first capability keeps the §6.5
    // evaluator (expiry, revoke, subject scope) as the single source of truth.
    let Some(capability) = grant_capabilities(grant).first() else {
        return false;
    };
    let unrestricted = match grant {
        MemoryGrant::RestrictedGrant(grant) => grant.context_ids.is_empty(),
        MemoryGrant::UnrestrictedGrant(_) => true,
    };
    let shared = match grant {
        MemoryGrant::RestrictedGrant(grant) => grant.shared_access.unwrap_or(false),
        MemoryGrant::UnrestrictedGrant(_) => true,
    };
    if !unrestricted && !shared {
        return false;
    }
    let requirement = GrantRequirement {
        capability,
        subject_id: Some(subject_id),
        context_ids: Vec::new(),
        reads_shared: true,
        subject_all: false,
    };
    grant_covers(grant, principal_id, space_id, now, &requirement)
}

fn grant_capabilities(grant: &MemoryGrant) -> &[String] {
    match grant {
        MemoryGrant::RestrictedGrant(grant) => &grant.capabilities,
        MemoryGrant::UnrestrictedGrant(grant) => &grant.capabilities,
    }
}

/// §6.7: 64-hex SHA-256 over the JCS-canonical candidate content, computed
/// before the body is scrubbed.
fn content_digest(candidate: &CandidateRow) -> StoreResult<String> {
    let raw = candidate
        .content_json
        .as_deref()
        .ok_or_else(|| StoreError::Corrupt("candidate has no content to digest".to_string()))?;
    let content: Value = serde_json::from_str(raw)
        .map_err(|e| StoreError::Corrupt(format!("candidate content: {e}")))?;
    match spoke_operations::jcs_sha256_hex(&content) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(StoreError::Corrupt(reject.message)),
    }
}
