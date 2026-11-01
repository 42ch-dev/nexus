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
//!   ([`MemoryStore::reclaim_expired`]) — all namespaces, keyless
//!   receipts included — so eviction is committed whatever the operation then
//!   does: a replay refusal, an authorization failure or any other early return
//!   drops only the effect transaction. A sweep failure is logged and never
//!   fails or rolls back the operation it precedes. `memory.receipts.get` stays
//!   read-only (MEM-R011) and only reports what the sweep left. Within one
//!   `MemoryStore` the clock never runs backwards ([`MemoryStore::now`]), so a
//!   body seen past its retention window is not served again for as long as that
//!   provider lives, whatever the wall clock does next. Across a restart the
//!   guarantee is the committed floor: every sweep commits the reading it decided
//!   at into `memory_provider_meta`, in the same transaction as the eviction, and
//!   [`MemoryStore::open`] seeds its clock from that row before any evaluation —
//!   so a reopened provider resumes from the last committed reading instead of
//!   from zero, and a wall clock that moved backwards cannot re-open what a
//!   committed sweep closed. One limit stays open and is not claimed as closed: a
//!   read cannot persist the expiry it observes (MEM-R011), so an expiry first
//!   observed after the last committed sweep is not carried across a restart —
//!   dropping the provider and reopening it, with nothing abrupt involved, serves
//!   that body again. The committed floor bounds the re-exposure window to that
//!   segment. Closing it needs a read-path persistence contract change — an owner
//!   decision, not a provider choice (compass D13 residual, open pending owner
//!   adjudication).
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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicU8;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

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

use super::journal::{self, FileEffect};

/// §6.10 receipt ids are provider-assigned opaque strings.
fn mint_receipt_id() -> String {
    format!("rcp_{}", Uuid::new_v4().simple())
}

/// The `memory_provider_meta` row holding the durable retention clock floor
/// (microseconds since the epoch, decimal text).
const CLOCK_FLOOR_KEY: &str = "clock_floor_micros";

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
    /// §5.2/D-10: a journaled file intent for this subject could not be settled
    /// and is quarantined. Reads of the subject answer the terminal
    /// `INTERNAL_ERROR` rather than serve half-committed content.
    Unreconciled {
        subject_id: String,
    },
    /// §5.2 step 2: an earlier intent for this operation's file target is still
    /// unsettled, so publishing now would strand its owed projection. Raised
    /// before this operation commits, so it is a retryable no-effect refusal.
    UnsettledPredecessor {
        receipt_id: String,
        state: String,
    },
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

pub(super) type StoreResult<T> = Result<T, StoreError>;

/// Test-only commit fault, injected by the §14.2/§14.3 proof; compiled out of a
/// production build (`test-hooks`).
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_NONE: u8 = 0;
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_BEFORE: u8 = 1;
#[cfg(feature = "test-hooks")]
const COMMIT_FAULT_AFTER: u8 = 2;

/// Test-only file-effect fault, injected by the §5.2/C-113 crash-window
/// proofs; compiled out of a production build (`test-hooks`). `AFTER_STAGE`
/// leaves the hidden staged file on disk and stops before the publish rename;
/// `AFTER_PUBLISH` leaves the target published and stops before the journal is
/// marked applied.
#[cfg(feature = "test-hooks")]
const FILE_FAULT_NONE: u8 = 0;
#[cfg(feature = "test-hooks")]
const FILE_FAULT_AFTER_STAGE: u8 = 1;
#[cfg(feature = "test-hooks")]
const FILE_FAULT_AFTER_PUBLISH: u8 = 2;

/// Test-only settle fault, injected by the §14.3 post-commit proofs; compiled
/// out of a production build (`test-hooks`). Both modes fail *after* the
/// journal COMMIT, so the provider must report `OUTCOME_UNCERTAIN`.
#[cfg(feature = "test-hooks")]
const SETTLE_FAULT_NONE: u8 = 0;
/// The settle transaction does not open.
#[cfg(feature = "test-hooks")]
const SETTLE_FAULT_OPEN: u8 = 1;
/// The settle transaction opens but its write fails.
#[cfg(feature = "test-hooks")]
const SETTLE_FAULT_WRITE: u8 = 2;

/// §5.2 step 2 proof seam: the rendezvous a parked publication waits at — one
/// signal that it holds the workspace publication section (gate and flock), one
/// that frees it. Test only.
#[cfg(feature = "test-hooks")]
struct PublishRendezvous {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Barrier>,
}

/// §6.5/§7.28 proof seam: the rendezvous a parked read waits at — `entered`
/// reports that it holds its connection and will sample the clock only after
/// the wait, `release` frees it. Test only.
#[cfg(feature = "test-hooks")]
struct ReadRendezvous {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Barrier>,
}

/// §6.10 receipt status while its journaled file effect is not yet published
/// (§5.2 step 2): durable, but not terminal.
pub(super) const RECEIPT_PENDING: &str = "pending";

/// §5.2 step 3: the receipt of an intent that could not be published. Terminal:
/// `error_json` carries the failure envelope, and a later re-drive publishes the
/// intent and replaces this status with `committed`.
pub(super) const RECEIPT_FAILED: &str = "failed";

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

/// The publication gate of one workspace in this process (§5.2 step 2).
///
/// Keyed on the provider's projection root — the workspace directory holding
/// `state.db` and `memory-store/` — so every provider instance opened on that
/// workspace shares one lock and their settle-to-publish windows cannot
/// interleave. A second *process* is excluded by the workspace
/// [`journal::PublishFlock`] instead; this gate is the in-process half of the
/// same section, and it is taken **before** that flock.
fn workspace_publish_gate(files_root: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static GATES: LazyLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    let key = files_root.parent().map_or_else(
        || files_root.to_path_buf(),
        |workspace| std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf()),
    );
    let mut gates = GATES.lock().expect("publish gate registry");
    Arc::clone(gates.entry(key).or_default())
}

/// The held publication section of one operation on this workspace (§5.2 step 2):
/// this process's workspace gate and the cross-process [`journal::PublishFlock`],
/// in that order.
///
/// **One lock order for every file-effect path: gate → flock → transaction.** The
/// gate is taken first so same-process waiters queue on an async mutex and never
/// occupy a blocking worker: only the gate holder ever waits for the flock, which
/// makes one blocked waiter per workspace the permanent ceiling — the holder's
/// own file work cannot starve behind a crowd of its same-process peers. No path
/// takes a write transaction before this section, so the pair cannot deadlock.
/// The ceiling survives cancellation: the waiting task owns the gate guard
/// ([`journal::PublishFlock::acquire`] takes it by value and returns it with the
/// lease), so a cancelled publication drops only its future and cannot release
/// the gate while its flock wait is still running — the next caller parks on the
/// gate instead of starting a second waiter.
///
/// Held from before the predecessor settlement until after the last publish,
/// which is what keeps another provider process out of the displacement's
/// link→unlink window. The lease itself is only *parked* here: every file
/// operation runs inside [`PublicationGuard::files`], whose blocking task owns it
/// ([`journal::PublishFlock::section`]).
pub(super) struct PublicationGuard {
    lease: Option<journal::PublishFlock>,
    _gate: tokio::sync::OwnedMutexGuard<()>,
}

impl PublicationGuard {
    /// Run one file-effect section (`stage`, `publish` or `settle`) on a blocking
    /// task that owns the lease for its whole duration, then take the lease back.
    ///
    /// The lease is always held by this guard or by that task — never by the
    /// caller's future — so a cancelled request drops the handle while the
    /// ordered filesystem operations still finish and release the lock
    /// themselves. The work must be `'static` and synchronous for the same
    /// reason: nothing may stay queued on the blocking pool past the section.
    async fn files<T, F>(&mut self, work: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let lease = self.lease.take().expect("publication lease");
        let (lease, out) = lease.section(work).await;
        self.lease = Some(lease);
        out
    }
}

pub(super) struct MemoryStore {
    pool: SqlitePool,
    /// Holds this provider's writer admission for the provider's lifetime.
    _guarded: GuardedPool,
    /// Root of the provider's own markdown projection, next to `state.db`.
    files_root: PathBuf,
    /// §5.2 step 2: serializes the settle-to-publish window of every operation
    /// with a file effect (and the recovery pass) **within this process**. SQLite
    /// serializes the row writes, but the publish happens after that transaction
    /// commits, so without one order per target two committed updates of it could
    /// publish out of order and leave the file older than the row.
    ///
    /// Scoped to the **workspace**, not to this provider: two provider instances
    /// opened on one workspace in this process share it, so a predecessor they
    /// both owe cannot interleave. Another *process* is excluded by the workspace
    /// [`journal::PublishFlock`], which every file-effect path takes **after**
    /// this gate ([`PublicationGuard`] states the order) — neither lock alone is
    /// the publication guarantee.
    publish_gate: Arc<tokio::sync::Mutex<()>>,
    /// Latest provider-clock reading in microseconds since the epoch. A floor,
    /// so retention verdicts cannot be re-derived downwards (§6.10); seeded from
    /// the persisted floor at [`Self::open`] and persisted again by every sweep.
    clock_floor: AtomicI64,
    #[cfg(feature = "test-hooks")]
    commit_fault: AtomicU8,
    /// Test-only file-effect fault (§5.2/C-113 crash windows).
    #[cfg(feature = "test-hooks")]
    file_fault: AtomicU8,
    /// Test-only settle fault: a failure of the post-commit journal/receipt
    /// settlement, injected in the two modes the real path can hit (§14.3).
    #[cfg(feature = "test-hooks")]
    settle_fault: AtomicU8,
    /// Test-only offset applied to the provider clock (§6.5/§6.10).
    #[cfg(feature = "test-hooks")]
    clock_skew: AtomicI64,
    /// Test-only rendezvous a read waits at once it holds its connection —
    /// `entered` tells the test the read is parked, `release` frees it — so a
    /// test can move the clock while the read is queued.
    #[cfg(feature = "test-hooks")]
    read_barrier: std::sync::Mutex<Option<ReadRendezvous>>,
    /// Test-only rendezvous a publication waits at **while it holds the
    /// workspace publication gate** — `entered` tells the test the gate is held,
    /// `release` frees it — so a second provider instance on the same workspace
    /// can be shown waiting.
    #[cfg(feature = "test-hooks")]
    publish_barrier: std::sync::Mutex<Option<PublishRendezvous>>,
    /// Test-only report a workspace flock **wait** sends from its blocking task
    /// just before it blocks, so a regression can cancel callers that have
    /// really reached the wait. Every wait reports while it is installed.
    #[cfg(feature = "test-hooks")]
    flock_wait_report: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
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
        self.reclaim_expired().await;
        let tx = begin_immediate(&self.pool).await?;
        Ok((tx, self.now()))
    }

    /// Open a read transaction and read the authorization clock **after** the
    /// connection is held (§6.5). Acquiring a pooled connection can queue for as
    /// long as another read holds it or the pool refills, so a clock read by the
    /// caller beforehand would let a grant that expires during that wait still
    /// authorize disclosure. Every content/authorization read takes its `now`
    /// from here.
    async fn begin_read(
        &self,
    ) -> StoreResult<(sqlx::Transaction<'_, sqlx::Sqlite>, DateTime<Utc>)> {
        let tx = self.pool.begin().await?;
        #[cfg(feature = "test-hooks")]
        self.read_gate().await;
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
    ///
    /// The same transaction records the reading it decided at
    /// ([`record_clock_floor`]), so the clock this sweep judged retention by is
    /// what the next process resumes from. It also reclaims the two families
    /// that age out on the same clock: applied journal shells (§5.2/MEM-R014,
    /// `JOURNAL_RETENTION_SECONDS`) and expired pending candidates (§6.7).
    async fn reclaim_expired(&self) {
        let mut tx = match begin_immediate(&self.pool).await {
            Ok(tx) => tx,
            Err(error) => {
                tracing::warn!(?error, "memory retention sweep skipped");
                return;
            }
        };
        if let Err(error) = retention_sweep(&mut tx, self.now()).await {
            tracing::warn!(?error, "memory retention sweep failed");
            return;
        }
        if let Err(error) = tx.commit().await {
            tracing::warn!(?error, "memory retention sweep did not commit");
        }
    }

    /// The provider clock: every authorization decision and every §6.10
    /// retention window is evaluated against it, and it is the one place a test
    /// may move time. Writes sample it after the write lock, reads after their
    /// connection ([`Self::begin_read`]).
    ///
    /// It never runs backwards within a process: the latest reading is a floor
    /// for every later one, so a wall-clock correction (or a test clock move)
    /// cannot re-open a window that already closed. A receipt body the sweep
    /// reported past its window stays reported — retention is one-way, not a
    /// projection of the current clock.
    ///
    /// A restart resumes the floor from the last committed sweep rather than
    /// from zero, which bounds re-exposure to the segment since that sweep; it
    /// does not close the restart case. An expiry observed only on the read path
    /// is not persisted (MEM-R011 keeps reads read-only), so the body can be
    /// served again after a restart plus a backward clock until the owner
    /// adjudicates a read-path persistence contract change — compass D13
    /// residual, open, not closed.
    fn now(&self) -> DateTime<Utc> {
        #[cfg(feature = "test-hooks")]
        let observed =
            Utc::now() + chrono::Duration::seconds(self.clock_skew.load(Ordering::SeqCst));
        #[cfg(not(feature = "test-hooks"))]
        let observed = Utc::now();
        let micros = observed.timestamp_micros();
        let floor = self.clock_floor.fetch_max(micros, Ordering::SeqCst);
        DateTime::from_timestamp_micros(floor.max(micros)).unwrap_or(observed)
    }

    /// Open the provider's pool and seed the retention clock with its durable
    /// floor (§6.10).
    ///
    /// The floor is read here, before any evaluation, so the first sweep of this
    /// process — [`Self::provision`]'s — already judges retention at a reading
    /// the previous process committed to, not at a wall clock that may have moved
    /// backwards across the restart.
    pub(super) async fn open(db_path: &Path, creator_id: &str) -> StoreResult<Self> {
        let guarded = init_guarded_pool(db_path, creator_id).await?;
        let pool = guarded.clone_pool();
        let files_root = db_path.parent().map_or_else(
            || PathBuf::from(journal::FILES_DIR),
            |dir| dir.join(journal::FILES_DIR),
        );
        let persisted: Option<String> =
            sqlx::query_scalar("SELECT value FROM memory_provider_meta WHERE key = ?")
                .bind(CLOCK_FLOOR_KEY)
                .fetch_optional(&pool)
                .await?;
        let clock_floor = persisted
            .map(|value| {
                value.parse::<i64>().map_err(|error| {
                    StoreError::Corrupt(format!("memory_provider_meta.{CLOCK_FLOOR_KEY}: {error}"))
                })
            })
            .transpose()?
            .unwrap_or(0);
        let publish_gate = workspace_publish_gate(&files_root);
        Ok(Self {
            pool,
            _guarded: guarded,
            files_root,
            publish_gate,
            clock_floor: AtomicI64::new(clock_floor),
            #[cfg(feature = "test-hooks")]
            commit_fault: AtomicU8::new(COMMIT_FAULT_NONE),
            #[cfg(feature = "test-hooks")]
            file_fault: AtomicU8::new(FILE_FAULT_NONE),
            #[cfg(feature = "test-hooks")]
            settle_fault: AtomicU8::new(SETTLE_FAULT_NONE),
            #[cfg(feature = "test-hooks")]
            clock_skew: AtomicI64::new(0),
            #[cfg(feature = "test-hooks")]
            read_barrier: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            publish_barrier: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            flock_wait_report: std::sync::Mutex::new(None),
        })
    }

    /// §6.5/§6.10 proof seam: move the provider clock forward so the grant
    /// expiries and receipt retention windows elapse. Compiled out of a
    /// production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn advance_clock(&self, seconds: i64) {
        self.clock_skew.fetch_add(seconds, Ordering::SeqCst);
    }

    /// §6.5/§7.28 proof seam: park the next read after it holds its connection —
    /// `entered` signals it is parked, `release` frees it — so a test can move the
    /// clock while the read is queued and know it has not been sampled yet.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn park_next_read(
        &self,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Barrier>,
    ) {
        *self.read_barrier.lock().expect("read gate") = Some(ReadRendezvous { entered, release });
    }

    /// §5.2 step 2 proof seam: park the next publication once it holds the
    /// workspace publication section and before it settles or stages anything, so
    /// a test can observe another provider instance on the same workspace waiting
    /// for it. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn park_next_publish(
        &self,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Barrier>,
    ) {
        *self.publish_barrier.lock().expect("publish barrier") =
            Some(PublishRendezvous { entered, release });
    }

    /// §5.2 step 2 proof seam: have every workspace flock **wait** report from
    /// its blocking task just before it blocks, so a test can cancel callers that
    /// have really reached the wait. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn report_flock_waits(&self, report: std::sync::mpsc::Sender<()>) {
        *self.flock_wait_report.lock().expect("flock wait report") = Some(report);
    }

    #[cfg(feature = "test-hooks")]
    async fn read_gate(&self) {
        let parked = {
            let mut gate = self.read_barrier.lock().expect("read gate");
            gate.take()
        };
        if let Some(rendezvous) = parked {
            rendezvous.entered.notify_one();
            rendezvous.release.wait().await;
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

    /// Force the next journal publish to stop at a crash window: a `false`
    /// fault stops after the hidden staged write (target not yet renamed), a
    /// `true` fault stops after the publish rename (content already in place,
    /// journal not yet marked applied). §5.2/C-113 proof seam only.
    #[cfg(feature = "test-hooks")]
    pub(super) fn inject_file_fault(&self, after_publish: bool) {
        self.file_fault.store(
            if after_publish {
                FILE_FAULT_AFTER_PUBLISH
            } else {
                FILE_FAULT_AFTER_STAGE
            },
            Ordering::SeqCst,
        );
    }

    /// Force the next post-commit settle step to fail: `before_write` fails
    /// before the settle transaction opens, otherwise its write fails. Both are
    /// failures *after* the journal COMMIT (§14.3 proof seam only).
    #[cfg(feature = "test-hooks")]
    pub(super) fn inject_settle_fault(&self, before_write: bool) {
        self.settle_fault.store(
            if before_write {
                SETTLE_FAULT_OPEN
            } else {
                SETTLE_FAULT_WRITE
            },
            Ordering::SeqCst,
        );
    }

    /// The provider's markdown projection root (§5.2). Test seam only.
    #[cfg(feature = "test-hooks")]
    pub(super) fn file_root(&self) -> &Path {
        &self.files_root
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
        // §5.2/D-10: a reopen settles whatever the previous process journaled
        // but did not publish, before provisioning makes anything else visible.
        // The pass is bounded and best-effort: an intent it cannot settle stays
        // quarantined and the subject's reads report that explicitly.
        self.recover_journal(space_id).await;
        // §6.10 retention is durable even for a deployment that only reads:
        // opening the provider reclaims what has aged out, and does so in its
        // own committed transaction so a provisioning failure cannot put an
        // already-evicted body back either.
        self.reclaim_expired().await;
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
        let (mut tx, now) = self.begin_read().await?;
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

    /// §7.6 subject read: the "any live grant reaches this subject" check and
    /// the subject row come from ONE read transaction on the store's clock
    /// (S-4), so a revoke committing between them cannot disclose the subject.
    pub(super) async fn subject_for_read(
        &self,
        principal_id: &str,
        space_id: &str,
        subject_id: &str,
    ) -> StoreResult<SubjectRow> {
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let rows = grants_in_tx(&mut tx, space_id).await?;
        if !any_grant_for_subject(&rows, principal_id, space_id, subject_id, now) {
            return Err(StoreError::NotAuthorized(format!(
                "principal {principal_id} holds no grant on subject {subject_id}"
            )));
        }
        let subject = subject_in_tx(&mut tx, space_id, subject_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("subject {subject_id}")))?;
        tx.commit().await?;
        Ok(subject)
    }

    /// The visible spaces AND their invalidation marker from one consistent read
    /// (§7.0.7, MEM-R026). The marker is the sum of the visible revisions, which
    /// only grows while the set is unchanged and moves when a space drifts in or
    /// out of visibility.
    pub(super) async fn visible_spaces(
        &self,
        principal_id: &str,
    ) -> StoreResult<(Vec<SpaceRow>, String)> {
        let (mut tx, now) = self.begin_read().await?;
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

    /// §7.6/§7.8/§7.0.7 self-view read: the `admin` verdict, the page it selects
    /// and the invalidation marker come from ONE read transaction on the store's
    /// clock (S-4), so a revoke committing between the verdict and the page can
    /// neither widen a self-view to the space-wide one nor authorize the
    /// `principal_id` filter. Returns the page, its marker, the principal
    /// filter actually applied — admin sees the requested filter, everyone else
    /// sees their own grants — and whether more rows follow it.
    pub(super) async fn grants_page_for(
        &self,
        principal_id: &str,
        space_id: &str,
        subject_id: Option<&str>,
        principal_filter: Option<&str>,
        page: Option<(u64, u64)>,
    ) -> StoreResult<(Vec<GrantRow>, String, Option<String>, bool)> {
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grant_rows = grants_in_tx(&mut tx, space_id).await?;
        let admin = covered(
            &wire_grants(&grant_rows)?,
            principal_id,
            space_id,
            now,
            &super::admin_requirement(subject_id),
        );
        if !admin && principal_filter.is_some() {
            return Err(StoreError::NotAuthorized(
                "the principal_id filter is admin-only (§7.8)".to_string(),
            ));
        }
        let filter = if admin {
            principal_filter
        } else {
            Some(principal_id)
        };
        // Constant predicates only: the absent-filter variants are separate
        // statements so the (space_id, …) indexes stay usable.
        let mut where_sql = String::from(" WHERE space_id = ?");
        let mut binds: Vec<&str> = vec![space_id];
        if let Some(subject_id) = subject_id {
            where_sql.push_str(" AND subject_id = ?");
            binds.push(subject_id);
        }
        if let Some(filter) = filter {
            where_sql.push_str(" AND principal_id = ?");
            binds.push(filter);
        }
        let page_clause = if page.is_some() {
            " LIMIT ? OFFSET ?"
        } else {
            ""
        };
        let rows_sql = format!(
            "SELECT {GRANT_COLUMNS} FROM memory_grants{where_sql} ORDER BY grant_id{page_clause}"
        );
        let mut query = sqlx::query_as::<_, GrantRow>(sqlx::AssertSqlSafe(rows_sql));
        for bind in binds {
            query = query.bind(bind);
        }
        if let Some((limit, offset)) = page {
            query = query
                .bind(page_limit_with_probe(limit))
                .bind(i64::try_from(offset).unwrap_or(i64::MAX));
        }
        let mut rows = query.fetch_all(&mut *tx).await?;
        let more = page.is_some_and(|(limit, _)| page_had_more(&mut rows, limit));
        let (marker,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(revision), 0) FROM memory_grants WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((rows, marker.to_string(), filter.map(str::to_string), more))
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
        let row = self
            .write_receipt(&mut tx, ctx, &receipt_id, Some(result), now)
            .await?;
        self.commit_effect(tx).await?;
        Ok(row)
    }

    /// §5.2/D-10 journal-first commit for an op with file effects.
    ///
    /// The intent row joins the effect rows and a **pending** receipt in the
    /// SAME guarded transaction; its commit is the operation's point of no
    /// return. Only after it does the file publish run, and only once the file
    /// is consistent does one settlement transaction mark the intent applied,
    /// scrub its content and terminalize the receipt. Every failure past the
    /// commit — publish, settlement, applied mark — is `OUTCOME_UNCERTAIN`:
    /// the effect committed and the journal keeps the intent recoverable.
    ///
    /// The caller's [`PublicationGuard`] is passed through, not re-taken: its
    /// file sections own the lease while they run, so the publication and the
    /// recovery that follows a cancelled caller stay ordered.
    #[allow(clippy::too_many_arguments)]
    async fn commit_journalled(
        &self,
        mut tx: sqlx::Transaction<'_, sqlx::Sqlite>,
        publication: &mut PublicationGuard,
        ctx: &OpContext<'_>,
        receipt_id: String,
        subject_id: &str,
        effects: &[FileEffect],
        result: &Value,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        write_journal_entry(&mut tx, ctx, &receipt_id, subject_id, effects, result, now).await?;
        self.write_receipt(&mut tx, ctx, &receipt_id, None, now)
            .await?;
        self.commit_effect(tx).await?;
        if let Err(error) = self
            .publish_effects(publication, &receipt_id, effects)
            .await
        {
            // §5.2 step 3/S-1: terminalize the intent now. Leaving it `pending`
            // silently withheld the subject from every read until the next open,
            // and a deterministic refusal was reported by no channel at all. The
            // payload is kept, which is the evidence a recovery pass re-drives;
            // the response stays `OUTCOME_UNCERTAIN` because the file write may
            // have landed before it failed (§14.3).
            let failure = quarantine_failure(&receipt_id, subject_id, &describe(&error));
            if let Err(settle) = self
                .settle_journal(&receipt_id, Settlement::Quarantined(failure), now)
                .await
            {
                // The publish failure is the caller's outcome; a settle failure
                // only means the intent is still `pending` and the next open
                // (or the next same-key replay) recovers it.
                tracing::warn!(
                    receipt_id = %receipt_id,
                    ?settle,
                    "memory journal quarantine skipped"
                );
            }
            return Err(error);
        }
        self.settle_journal(&receipt_id, Settlement::Applied(result), now)
            .await
            .map_err(|error| match error {
                StoreError::CommitUnknown(_) => error,
                other => StoreError::CommitUnknown(format!(
                    "journal settlement after COMMIT: {}",
                    describe(&other)
                )),
            })
    }

    /// Append the receipt row to the still-open effect transaction.
    ///
    /// `result: None` writes the §5.2 pending shape — durable at the point of
    /// no return, carrying no payload and no terminal stamp until the file
    /// effect is consistent (the table's own §6.10 CHECK enforces the shape).
    async fn write_receipt(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &OpContext<'_>,
        receipt_id: &str,
        result: Option<&Value>,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let at = timestamp(now);
        let (status, result_json, completed_at) = match result {
            Some(result) => (
                "committed",
                Some(
                    serde_json::to_string(result)
                        .map_err(|e| StoreError::Corrupt(format!("receipt result: {e}")))?,
                ),
                Some(at.clone()),
            ),
            None => (RECEIPT_PENDING, None, None),
        };
        sqlx::query(
            "INSERT INTO memory_receipts \
             (space_id, requester_principal_id, op, idempotency_key, receipt_id, payload_digest, \
              status, result_json, error_json, created_at, completed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(ctx.space_id)
        .bind(ctx.requester_principal_id)
        .bind(ctx.op)
        .bind(ctx.idempotency_key)
        .bind(receipt_id)
        .bind(ctx.payload_digest)
        .bind(status)
        .bind(&result_json)
        .bind(&at)
        .bind(&completed_at)
        .execute(&mut **tx)
        .await?;
        Ok(ReceiptRow {
            space_id: ctx.space_id.to_string(),
            requester_principal_id: ctx.requester_principal_id.to_string(),
            op: ctx.op.to_string(),
            idempotency_key: ctx.idempotency_key.map(str::to_string),
            receipt_id: receipt_id.to_string(),
            payload_digest: ctx.payload_digest.to_string(),
            status: status.to_string(),
            result_json,
            error_json: None,
            created_at: at,
            completed_at,
        })
    }

    /// `COMMIT` with the §14.2/§14.3 proof seam applied at its boundary.
    async fn commit_effect(&self, tx: sqlx::Transaction<'_, sqlx::Sqlite>) -> StoreResult<()> {
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
        Ok(())
    }

    /// §5.2 step 2: stage every planned file, verify and publish it under the
    /// intent's ownership rule. A failure here keeps the entry `pending` and the
    /// receipt pending, so the next open — or the next operation on the same
    /// target — settles it before anything else publishes there.
    ///
    /// Both halves run as [`PublicationGuard::files`] sections: the blocking task
    /// that stages or publishes owns the workspace lease until its last
    /// filesystem call is done, so a cancelled request cannot release it under a
    /// queued unlink. Staging and publishing are two sections only because the
    /// §5.2 crash seam between them is a real protocol point — the staged file
    /// surviving without the target is exactly what recovery resolves.
    async fn publish_effects(
        &self,
        publication: &mut PublicationGuard,
        receipt_id: &str,
        effects: &[FileEffect],
    ) -> StoreResult<()> {
        let publish_error = |step: &str, error: std::io::Error| {
            StoreError::CommitUnknown(format!("file effect {step} after journal COMMIT: {error}"))
        };
        let planned: Arc<[FileEffect]> = effects.to_vec().into();
        let staged = {
            let (root, receipt, planned) = (
                self.files_root.clone(),
                receipt_id.to_string(),
                Arc::clone(&planned),
            );
            publication
                .files(move || {
                    planned
                        .iter()
                        .try_for_each(|effect| journal::stage(&root, &receipt, effect))
                })
                .await
        };
        staged.map_err(|error| publish_error("staging failed", error))?;
        #[cfg(feature = "test-hooks")]
        let fault = self.file_fault.load(Ordering::SeqCst);
        #[cfg(feature = "test-hooks")]
        if fault == FILE_FAULT_AFTER_STAGE {
            self.file_fault.store(FILE_FAULT_NONE, Ordering::SeqCst);
            return Err(StoreError::CommitUnknown(
                "injected failure after staging the file effect".to_string(),
            ));
        }
        let published = {
            let (root, receipt, planned) = (
                self.files_root.clone(),
                receipt_id.to_string(),
                Arc::clone(&planned),
            );
            publication
                .files(move || {
                    planned
                        .iter()
                        .try_for_each(|effect| journal::publish(&root, &receipt, effect))
                })
                .await
        };
        published.map_err(|error| publish_error("publish failed", error))?;
        #[cfg(feature = "test-hooks")]
        if fault == FILE_FAULT_AFTER_PUBLISH {
            self.file_fault.store(FILE_FAULT_NONE, Ordering::SeqCst);
            return Err(StoreError::CommitUnknown(
                "injected failure after publishing the file effect".to_string(),
            ));
        }
        Ok(())
    }

    /// §5.2 steps 2–3: record how one intent settled in one transaction — the
    /// journal state, the payload it keeps or loses, and the receipt's terminal
    /// outcome. Doing all of it together makes the scrub crash-safe: the intent
    /// is never applied without its content gone, and re-driving a still
    /// unsettled entry re-publishes idempotently before settling it again.
    async fn settle_journal(
        &self,
        receipt_id: &str,
        settlement: Settlement<'_>,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        #[cfg(feature = "test-hooks")]
        let fault = self.settle_fault.swap(SETTLE_FAULT_NONE, Ordering::SeqCst);
        #[cfg(feature = "test-hooks")]
        if fault == SETTLE_FAULT_OPEN {
            return Err(StoreError::Database(nexus_local_db::LocalDbError::Io(
                "injected journal settle failure: no transaction".to_string(),
            )));
        }
        let mut tx = begin_immediate(&self.pool).await?;
        let at = timestamp(now);
        match &settlement {
            Settlement::Applied(_) => {
                sqlx::query(
                    "UPDATE memory_journal SET state = ?, targets_json = '[]', updated_at = ? \
                     WHERE receipt_id = ? AND state <> ?",
                )
                .bind(journal::STATE_APPLIED)
                .bind(&at)
                .bind(receipt_id)
                .bind(journal::STATE_APPLIED)
                .execute(&mut *tx)
                .await?;
            }
            // The payload stays put: it is the recovery evidence a re-drive
            // republishes, and deleting it here is what stranded the intent.
            Settlement::Quarantined(_) => {
                sqlx::query(
                    "UPDATE memory_journal SET state = ?, updated_at = ? \
                     WHERE receipt_id = ? AND state <> ?",
                )
                .bind(journal::STATE_QUARANTINED)
                .bind(&at)
                .bind(receipt_id)
                .bind(journal::STATE_APPLIED)
                .execute(&mut *tx)
                .await?;
            }
        }
        #[cfg(feature = "test-hooks")]
        if fault == SETTLE_FAULT_WRITE {
            // The way a real settle write fails: the transaction opened and its
            // first write landed, then the receipt write did not.
            return Err(StoreError::Database(nexus_local_db::LocalDbError::Io(
                "injected journal settle failure: write".to_string(),
            )));
        }
        match &settlement {
            Settlement::Applied(result) => {
                let result_json = serde_json::to_string(result)
                    .map_err(|e| StoreError::Corrupt(format!("receipt result: {e}")))?;
                // A re-drive publishes an intent whose receipt already recorded
                // the failure, so both unsettled shapes are replaceable.
                sqlx::query(
                    "UPDATE memory_receipts SET status = 'committed', result_json = ?, \
                     error_json = NULL, completed_at = ? \
                     WHERE receipt_id = ? AND status <> 'committed'",
                )
                .bind(&result_json)
                .bind(&at)
                .bind(receipt_id)
                .execute(&mut *tx)
                .await?;
            }
            Settlement::Quarantined(failure) => {
                let error_json = serde_json::to_string(failure)
                    .map_err(|e| StoreError::Corrupt(format!("quarantine failure: {e}")))?;
                sqlx::query(
                    "UPDATE memory_receipts SET status = 'failed', error_json = ?, completed_at = ? \
                     WHERE receipt_id = ? AND status = ?",
                )
                .bind(&error_json)
                .bind(&at)
                .bind(receipt_id)
                .bind(RECEIPT_PENDING)
                .execute(&mut *tx)
                .await?;
            }
        }
        let sql = format!("SELECT {RECEIPT_COLUMNS} FROM memory_receipts WHERE receipt_id = ?");
        let row = sqlx::query_as::<_, ReceiptRow>(sqlx::AssertSqlSafe(sql))
            .bind(receipt_id)
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit()
            .await
            .map_err(|e| StoreError::CommitUnknown(format!("journal settle COMMIT: {e}")))?;
        // The retention sweep may have reclaimed the receipt of an intent that
        // was still unsettled (the receipt's dedup window can end before the
        // target is resolved). The journal row is the recovery authority, so its
        // settlement still commits — only a caller that needs this receipt row
        // is told it is gone.
        row.ok_or_else(|| StoreError::NotFound(format!("receipt {receipt_id}")))
    }

    /// §5.2 step 2: take the workspace publication lock and settle every earlier
    /// intent that owns one of this operation's file targets.
    ///
    /// A predecessor must not be published over while it is unsettled: the
    /// classic window is a create that stopped after staging — row, journal and
    /// hidden staged file, no target — followed by a delete of that record, which
    /// would install a tombstone as though no projection had ever been owed and
    /// strand the earlier body. Whatever cannot be settled inline refuses this
    /// operation before its own effect commits.
    ///
    /// Settlement is not admission. It runs only after the caller's §6.5
    /// coverage and §7.0.5 dedup probe have been evaluated in
    /// [`Self::admitted`], so a caller without `records_write` gets the standard
    /// `NOT_AUTHORIZED` instead of another intent's receipt id and state, and a
    /// same-key replay reaches its own recorded receipt — answered by the
    /// operation's in-transaction probe — instead of being gated on a later
    /// intent it did not create. A replay settles nothing: it produces no effect.
    ///
    /// The section is taken before any transaction (**gate → flock →
    /// transaction** is the one lock order this store uses) and held across the
    /// operation's effect by the returned guard.
    async fn begin_publication(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        targets: &[String],
    ) -> StoreResult<PublicationGuard> {
        let mut publication = self.begin_publication_section().await?;
        #[cfg(feature = "test-hooks")]
        {
            let parked = self.publish_barrier.lock().expect("publish barrier").take();
            if let Some(rendezvous) = parked {
                rendezvous.entered.notify_one();
                rendezvous.release.wait().await;
            }
        }
        if self.admitted(ctx, admission).await?.is_some() {
            return Ok(publication);
        }
        let entries: Vec<JournalEntry> = sqlx::query_as(UNSETTLED_TARGETS_SQL)
            .bind(ctx.space_id)
            .bind(targets_array(targets)?)
            .fetch_all(&self.pool)
            .await?;
        for entry in entries {
            self.settle_intent(&mut publication, &entry).await;
        }
        Ok(publication)
    }

    /// §5.2 step 2's section, in the store's one lock order: this process's
    /// workspace gate first, then the workspace [`journal::PublishFlock`] that
    /// keeps another provider process's file effects out of this one's
    /// displacement. Both are held until the returned guard drops.
    ///
    /// The gate comes first because the flock wait is a blocking syscall on the
    /// blocking pool: waiting behind the gate keeps one waiter per workspace at
    /// most ([`PublicationGuard`] documents the property). The gate is handed to
    /// the task that runs the wait, so a cancelled caller cannot release it while
    /// that wait is still in flight — the task holds it until the lease is taken
    /// and hands both back.
    async fn begin_publication_section(&self) -> StoreResult<PublicationGuard> {
        let gate = Arc::clone(&self.publish_gate).lock_owned().await;
        #[cfg(feature = "test-hooks")]
        let report = self
            .flock_wait_report
            .lock()
            .expect("flock wait report")
            .clone();
        #[cfg(not(feature = "test-hooks"))]
        let report: Option<std::sync::mpsc::Sender<()>> = None;
        let (lease, gate) = journal::PublishFlock::acquire(&self.files_root, gate, move || {
            if let Some(report) = report {
                let _ = report.send(());
            }
        })
        .await
        .map_err(|error| {
            StoreError::Database(nexus_local_db::LocalDbError::Io(format!(
                "memory publication lock {}: {error}",
                self.files_root.join(journal::PUBLISH_LOCK_FILE).display()
            )))
        })?;
        Ok(PublicationGuard {
            lease: Some(lease),
            _gate: gate,
        })
    }

    /// §6.5 + §7.0.5 ahead of §5.2 settlement: the caller's authorization and the
    /// dedup probe, evaluated in a transaction of their own and kept by neither.
    /// The operation's admission transaction re-evaluates both as the authority
    /// for its effect; this only decides what a caller may learn — and whether
    /// there is anything to settle — before predecessor state is touched.
    async fn admitted(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
    ) -> StoreResult<Option<ReceiptRow>> {
        let mut tx = begin_immediate(&self.pool).await?;
        let now = self.now();
        let _ = authorize(&mut tx, admission, now).await?;
        let row = probe(&mut tx, ctx, now).await?;
        tx.rollback().await?;
        Ok(row)
    }

    /// §5.2 step 3: settle — and re-drive — the unsettled intents of this space.
    /// A pass that cannot run is logged and never fails the open. The pass holds
    /// the workspace publication section, so it never publishes inside another
    /// process's displacement.
    async fn recover_journal(&self, space_id: &str) {
        let mut publication = match self.begin_publication_section().await {
            Ok(publication) => publication,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "memory journal recovery skipped: no publication lock"
                );
                return;
            }
        };
        if let Err(error) = self.settle_unsettled(&mut publication, space_id).await {
            tracing::warn!(?error, "memory journal recovery pass skipped");
        }
    }

    /// §5.2 step 3, bounded and fair. Recoverable work (`pending`, `deferred`)
    /// settles first so a still-conflicting quarantine cannot starve it;
    /// quarantined intents are re-driven with whatever bound capacity is left,
    /// which is how an operator-resolved target gets its owed projection
    /// published.
    ///
    /// The pass resumes after the `seq` the previous pass stopped at and wraps,
    /// so a head that keeps conflicting is skipped in favour of the tail: with
    /// more unsettled intents than the bound, the 65th behind 64 persistently
    /// conflicting ones is reached on the next pass rather than never. The bound
    /// itself is unchanged — one pass never settles more than
    /// [`journal::RECOVERY_BOUND`] entries.
    async fn settle_unsettled(
        &self,
        publication: &mut PublicationGuard,
        space_id: &str,
    ) -> StoreResult<()> {
        let cursor = self.recovery_cursor(space_id).await?;
        let entries: Vec<JournalEntry> = sqlx::query_as(
            "SELECT seq, receipt_id, subject_id, targets_json, result_json FROM memory_journal \
             WHERE space_id = ? AND state <> 'applied' \
             ORDER BY state = ?, CASE WHEN seq > ? THEN 0 ELSE 1 END, seq LIMIT ?",
        )
        .bind(space_id)
        .bind(journal::STATE_QUARANTINED)
        .bind(cursor)
        .bind(journal::RECOVERY_BOUND)
        .fetch_all(&self.pool)
        .await?;
        let advance = entries.last().map(|entry| entry.seq);
        for entry in &entries {
            self.settle_intent(publication, entry).await;
        }
        // §5.2 step 5: what the bound did not reach is terminal until a later
        // pass settles it, never silently withheld.
        sqlx::query(
            "UPDATE memory_journal SET state = ?, updated_at = ? WHERE space_id = ? AND state = ?",
        )
        .bind(journal::STATE_DEFERRED)
        .bind(timestamp(self.now()))
        .bind(space_id)
        .bind(journal::STATE_PENDING)
        .execute(&self.pool)
        .await?;
        if let Some(seq) = advance {
            if let Err(error) = self.record_recovery_cursor(space_id, seq).await {
                // The pass did its work; only the rotation position is lost, and
                // the next pass simply starts over from the oldest intent.
                tracing::warn!(%space_id, ?error, "memory journal recovery cursor not recorded");
            }
        }
        Ok(())
    }

    /// Where the last bounded pass of this space stopped. Durable: a pass that
    /// restarted at the oldest unsettled intent every reopen would retry the
    /// same conflicting head forever.
    async fn recovery_cursor(&self, space_id: &str) -> StoreResult<i64> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM memory_provider_meta WHERE key = ?")
                .bind(recovery_cursor_key(space_id))
                .fetch_optional(&self.pool)
                .await?;
        value.map_or(Ok(0), |value| {
            value.parse::<i64>().map_err(|error| {
                StoreError::Corrupt(format!("memory_provider_meta recovery cursor: {error}"))
            })
        })
    }

    async fn record_recovery_cursor(&self, space_id: &str, seq: i64) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO memory_provider_meta (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, \
             updated_at = excluded.updated_at",
        )
        .bind(recovery_cursor_key(space_id))
        .bind(seq.to_string())
        .bind(timestamp(self.now()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Settle one journal row from the crash-consistent disk state and record the
    /// outcome. The caller holds the publication section (workspace gate and
    /// flock); the filesystem half runs inside [`PublicationGuard::files`],
    /// whose blocking task owns the lease until its last file operation returns.
    async fn settle_intent(&self, publication: &mut PublicationGuard, entry: &JournalEntry) {
        if let Err(error) = self.settle_entry(publication, entry).await {
            tracing::warn!(receipt_id = %entry.receipt_id, ?error, "memory journal settle skipped");
        }
    }

    /// [`Self::settle_intent`] returning the settled receipt: a re-drive
    /// publishes what the journal owes and then records the outcome in one
    /// transaction.
    async fn settle_entry(
        &self,
        publication: &mut PublicationGuard,
        entry: &JournalEntry,
    ) -> StoreResult<ReceiptRow> {
        let (root, receipt_id, targets_json) = (
            self.files_root.clone(),
            entry.receipt_id.clone(),
            entry.targets_json.clone(),
        );
        let settled = publication
            .files(move || settle_entry(&root, &receipt_id, &targets_json))
            .await;
        let outcome = entry.outcome();
        let settlement = match &settled {
            Ok(()) => Settlement::Applied(&outcome),
            Err(reason) => {
                tracing::warn!(
                    receipt_id = %entry.receipt_id,
                    %reason,
                    "memory journal intent quarantined"
                );
                Settlement::Quarantined(quarantine_failure(
                    &entry.receipt_id,
                    &entry.subject_id,
                    reason,
                ))
            }
        };
        self.settle_journal(&entry.receipt_id, settlement, self.now())
            .await
    }

    /// §5.2 step 2 on the replay path (S-2): when a same-key replay reaches a
    /// receipt whose journaled intent is still unsettled, re-drive and settle it
    /// before answering. The replay then serves the published outcome — or the
    /// recorded terminal failure when the target still conflicts — so the
    /// duplicate-key channel can never answer `OUTCOME_UNCERTAIN` forever for an
    /// intent a later pass could have settled.
    async fn settle_replayed(
        &self,
        publication: &mut PublicationGuard,
        row: ReceiptRow,
    ) -> StoreResult<ReceiptRow> {
        if row.status != RECEIPT_PENDING {
            return Ok(row);
        }
        let entry: Option<JournalEntry> = sqlx::query_as(
            "SELECT seq, receipt_id, subject_id, targets_json, result_json FROM memory_journal \
             WHERE receipt_id = ?",
        )
        .bind(&row.receipt_id)
        .fetch_optional(&self.pool)
        .await?;
        // Both rows commit in one transaction, so a pending receipt without its
        // intent is a store state this provider cannot produce.
        let entry = entry.ok_or_else(|| {
            StoreError::Corrupt(format!(
                "pending receipt {} has no journal intent",
                row.receipt_id
            ))
        })?;
        self.settle_entry(publication, &entry).await
    }
}

/// How one intent settled, as [`MemoryStore::settle_journal`] records it.
enum Settlement<'a> {
    /// The file effect is consistent: scrub the journaled payload — MEM-R014,
    /// a published intent keeps no deleted or superseded body — and commit the
    /// receipt with this content-free outcome.
    Applied(&'a Value),
    /// The intent could not be published: keep its payload, which is the
    /// recovery evidence a later pass republishes once the target is resolved,
    /// and record the failure on the receipt.
    Quarantined(Value),
}

/// The unsettled intents that own one of the bound file targets. `json_each`
/// expands the target list, so this is exact equality on each intent's own
/// `path` field — never a pattern match over the serialized JSON.
const UNSETTLED_TARGETS_SQL: &str = "SELECT j.seq, j.receipt_id, j.subject_id, j.targets_json, \
     j.result_json FROM memory_journal AS j \
     WHERE j.space_id = ? AND j.state <> 'applied' AND EXISTS ( \
         SELECT 1 FROM json_each(j.targets_json) AS e \
         WHERE json_extract(e.value, '$.path') IN (SELECT value FROM json_each(?))) \
     ORDER BY j.seq";

/// Whether any intent that owns one of the bound targets is still unsettled.
const UNSETTLED_TARGET_SQL: &str = "SELECT j.receipt_id, j.state FROM memory_journal AS j \
     WHERE j.space_id = ? AND j.state <> 'applied' AND EXISTS ( \
         SELECT 1 FROM json_each(j.targets_json) AS e \
         WHERE json_extract(e.value, '$.path') IN (SELECT value FROM json_each(?))) \
     ORDER BY j.seq LIMIT 1";

/// One unsettled journal row, in the shape both settlement passes read.
#[derive(sqlx::FromRow)]
struct JournalEntry {
    /// Settlement order: predecessors publish in commit order, and the bounded
    /// recovery pass rotates on it.
    seq: i64,
    receipt_id: String,
    subject_id: String,
    targets_json: String,
    result_json: Option<String>,
}

/// The `memory_provider_meta` row holding where a space's last bounded recovery
/// pass stopped (§5.2 step 3 fairness).
fn recovery_cursor_key(space_id: &str) -> String {
    format!("recovery_cursor:{space_id}")
}

impl JournalEntry {
    /// The content-free outcome the receipt is terminalized with. A row whose
    /// recorded outcome cannot be read still terminalizes — as a committed
    /// receipt with a null result — so no channel reports an already-published
    /// intent as perpetually pending.
    fn outcome(&self) -> Value {
        self.result_json
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or(Value::Null)
    }
}

/// The bound target list in the JSON-array form `json_each` expands.
fn targets_array(targets: &[String]) -> StoreResult<String> {
    serde_json::to_string(targets)
        .map_err(|e| StoreError::Corrupt(format!("journal target list: {e}")))
}

/// The first unsettled intent owning one of `targets`, if any. Read through the
/// caller's executor so the same check runs against the pool (before an
/// operation) and inside its write transaction (cross-process backstop).
async fn unsettled_target<'e, E>(
    executor: E,
    space_id: &str,
    targets: &[String],
) -> StoreResult<Option<(String, String)>>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let row: Option<(String, String)> = sqlx::query_as(UNSETTLED_TARGET_SQL)
        .bind(space_id)
        .bind(targets_array(targets)?)
        .fetch_optional(executor)
        .await?;
    Ok(row)
}

/// §5.2 step 2's last check: the operation may not commit while an earlier
/// intent for its target is unsettled.
///
/// The pre-transaction pass normally clears them; this one runs inside the write
/// transaction, so it re-reads the journal instead of trusting that pass. The
/// workspace flock normally keeps another provider process out of this section
/// entirely; this check is the durable invariant behind it, not a second lock.
async fn require_settled_targets(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    targets: &[String],
) -> StoreResult<()> {
    let unsettled = unsettled_target(&mut **tx, space_id, targets).await?;
    match unsettled {
        Some((receipt_id, state)) => Err(StoreError::UnsettledPredecessor { receipt_id, state }),
        None => Ok(()),
    }
}

/// The terminal failure a quarantined intent records on its receipt (§5.2 step
/// 3), shaped as the §14.1 memory error envelope so `memory.receipts.get` serves
/// it verbatim and names the supported recovery path.
fn quarantine_failure(receipt_id: &str, subject_id: &str, reason: &str) -> Value {
    json!({
        "code": "INTERNAL_ERROR",
        "message": format!(
            "receipt {receipt_id} is failed: its committed file effect for subject {subject_id} \
             is quarantined ({reason}). The target was left untouched, memory.receipts.get serves \
             this failure, and the intent is re-driven once that target is resolved"
        ),
        "details": {
            "retry_class": "terminal",
            "subject_id": subject_id,
            "receipt_id": receipt_id,
        },
        "extensions": {},
    })
}

/// §5.2 step 3: settle one journaled intent from the crash-consistent disk
/// state. Every target must be published or regenerated; the first target that
/// cannot be is the entry's quarantine reason.
///
/// Synchronous by design: [`MemoryStore::settle_intent`] runs it inside a
/// [`PublicationGuard::files`] section, which owns the lease for the whole call.
fn settle_entry(root: &Path, receipt_id: &str, targets_json: &str) -> Result<(), String> {
    let effects: Vec<FileEffect> =
        serde_json::from_str(targets_json).map_err(|error| format!("targets: {error}"))?;
    for effect in &effects {
        match journal::settle(root, receipt_id, effect) {
            journal::Settled::Published => {}
            journal::Settled::Quarantined(reason) => return Err(reason),
        }
    }
    Ok(())
}

/// §5.2 step 1: append the planned file effects to the still-open effect
/// transaction, so the intent commits with — never after — the rows it
/// describes. The content-free outcome rides along so recovery can terminalize
/// the receipt after the file effect is consistent.
async fn write_journal_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ctx: &OpContext<'_>,
    receipt_id: &str,
    subject_id: &str,
    effects: &[FileEffect],
    result: &Value,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let targets = serde_json::to_string(effects)
        .map_err(|error| StoreError::Corrupt(format!("journal targets: {error}")))?;
    let result_json = serde_json::to_string(result)
        .map_err(|error| StoreError::Corrupt(format!("journal result: {error}")))?;
    let at = timestamp(now);
    sqlx::query(
        "INSERT INTO memory_journal \
         (seq, receipt_id, space_id, subject_id, requester_principal_id, op, idempotency_key, \
          state, targets_json, result_json, created_at, updated_at) \
         VALUES ((SELECT COALESCE(MAX(seq), 0) + 1 FROM memory_journal), ?, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?)",
    )
    .bind(receipt_id)
    .bind(ctx.space_id)
    .bind(subject_id)
    .bind(ctx.requester_principal_id)
    .bind(ctx.op)
    .bind(ctx.idempotency_key)
    .bind(journal::STATE_PENDING)
    .bind(targets)
    .bind(result_json)
    .bind(&at)
    .bind(&at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The unsettled journal state of one subject (D-10 read isolation).
///
/// Terminal unresolved states outrank a merely pending one: a quarantined
/// subject — and one a bounded pass left `deferred` — has an outcome no later
/// write can repair on its own, so a newer pending intent for the same subject
/// must not mask it and turn the terminal diagnosis into silent absence.
async fn unsettled_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT state FROM memory_journal \
         WHERE space_id = ? AND subject_id = ? AND state <> 'applied' \
         ORDER BY CASE state WHEN ? THEN 0 WHEN ? THEN 1 ELSE 2 END, created_at LIMIT 1",
    )
    .bind(space_id)
    .bind(subject_id)
    .bind(journal::STATE_QUARANTINED)
    .bind(journal::STATE_DEFERRED)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|(state,)| state))
}

/// §5.2 steps 4–5: only an intent that is still being recovered withholds the
/// subject's objects silently. A quarantined one — and one a bounded pass left
/// `deferred` — answers the terminal refusal instead of waiting forever.
fn withheld(state: Option<&str>, subject_id: &str) -> StoreResult<bool> {
    match state {
        Some(journal::STATE_PENDING) => Ok(true),
        Some(_) => Err(StoreError::Unreconciled {
            subject_id: subject_id.to_string(),
        }),
        None => Ok(false),
    }
}

/// Render a post-commit store failure for an `OUTCOME_UNCERTAIN` message.
fn describe(error: &StoreError) -> String {
    match error {
        StoreError::Database(error) => error.to_string(),
        other => format!("{other:?}"),
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

/// §7.24/S-3: the ops that write a subject's own content, and therefore need the
/// subject row to exist. `memory.subjects.ensure` is their creator; the
/// control-plane ops (grant issuance, fence) name a subject without writing its
/// content and keep their own admission rules.
fn writes_subject_content(op: &str) -> bool {
    matches!(
        op,
        super::OP_CAPTURE | super::OP_RECORDS_CREATE | super::OP_PROFILE_UPDATE
    )
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
        match subject_in_tx(tx, admission.space_id, subject_id).await? {
            Some(subject) => {
                require_write_allowed(&subject, admission.placement_gate.as_slice())?;
            }
            // §7.24/S-3: a content/profile write over a never-ensured subject is
            // a `NOT_FOUND`, never an implicit creation — a space-wide grantee
            // must not mint records or a trusted `personality` profile for a
            // subject `subjects.get` cannot read. `memory.subjects.ensure` is
            // the op that creates it, and control-plane ops (grant issuance)
            // name a subject without writing its content.
            None if writes_subject_content(admission.op) => {
                return Err(StoreError::NotFound(format!("subject {subject_id}")));
            }
            None => {}
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

/// Persist the retention clock floor (§6.10) in the sweep's transaction.
///
/// The upsert only ever moves the floor forward: two providers on one workspace,
/// or a sweep that runs behind another writer, can never lower the reading a
/// previous one committed. [`MemoryStore::open`] reads it back before evaluating
/// anything.
async fn record_clock_floor(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO memory_provider_meta (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at \
         WHERE CAST(excluded.value AS INTEGER) > CAST(memory_provider_meta.value AS INTEGER)",
    )
    .bind(CLOCK_FLOOR_KEY)
    .bind(now.timestamp_micros().to_string())
    .bind(timestamp(now))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// §6.10/MEM-R021 + §5.2/MEM-R014 + §6.7 reclamation in one transaction.
///
/// Evicts every receipt body older than `receipt_retention_seconds` across all
/// dedup namespaces and the keyless `memory.subjects.ensure` receipts too, then
/// drops every tombstone older than `dedup_retention_seconds`. Eviction is a
/// one-way rewrite — the payload is discarded, so a later clock move cannot
/// resurrect it. [`MemoryStore::reclaim_expired`] owns the transaction and its
/// commit; this function only writes the reclamation and the clock reading it
/// was decided at into it.
///
/// The same window bounds the two content families that would otherwise retain
/// rows forever: an applied journal shell (its file content was scrubbed at
/// settlement, so only the diagnostic trail ages out — an *unsettled* intent is
/// never reclaimed, it is the recovery authority) and an expired pending
/// candidate (never consumed, and excluded from every list).
async fn retention_sweep(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    record_clock_floor(tx, now).await?;
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
    sqlx::query("DELETE FROM memory_journal WHERE state = 'applied' AND created_at < ?")
        .bind(retention_cutoff(
            now,
            super::policy::JOURNAL_RETENTION_SECONDS,
        ))
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "DELETE FROM memory_candidates \
         WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at <= ?",
    )
    .bind(timestamp(now))
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

/// `LIMIT` for one page: the page's own rows plus one probe row that says
/// whether the set continues past it, so `next_cursor` never needs a count.
fn page_limit_with_probe(limit: u64) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1)
}

/// Drop the probe row and report whether it was there.
fn page_had_more<T>(rows: &mut Vec<T>, limit: u64) -> bool {
    let more = u64::try_from(rows.len()).unwrap_or(u64::MAX) > limit;
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    more
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
    /// consumed; a consumed shell keeps its audit visibility. The list SQL
    /// enforces this predicate itself (S-9), so no read-path helper re-decides
    /// it here.
    fn requirement<'a>(&'a self, capability: &'a str) -> GrantRequirement<'a> {
        candidate_requirement(
            capability,
            self.subject_id.as_str(),
            self.capture_visibility.as_str(),
            self.capture_context_id.as_deref(),
        )
    }
}

/// The §6.7 coverage one candidate names: its own subject and immutable capture
/// placement. Shared by the page rows and the identity set the invalidation
/// marker digests, so the two never drift.
fn candidate_requirement<'a>(
    capability: &'a str,
    subject_id: &'a str,
    visibility: &str,
    context_id: Option<&str>,
) -> GrantRequirement<'a> {
    GrantRequirement {
        capability,
        subject_id: Some(subject_id),
        context_ids: context_id.map(str::to_string).into_iter().collect(),
        reads_shared: visibility == Placement::SHARED,
        subject_all: false,
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

/// Does any live grant reach this subject at all? §7.6 asks for "any capability
/// on the subject", which `grant_covers` cannot express (it always names one
/// required capability), so the §6.5 applicability filters are applied directly.
/// Evaluated inside the caller's read transaction on the store's clock (S-4).
fn any_grant_for_subject(
    grants: &[GrantRow],
    principal_id: &str,
    space_id: &str,
    subject_id: &str,
    now: DateTime<Utc>,
) -> bool {
    let now_text = timestamp(now);
    grants.iter().any(|grant| {
        grant.principal_id == principal_id
            && grant.space_id == space_id
            && grant.revoked_at.is_none()
            && grant
                .expires_at
                .as_deref()
                .is_none_or(|expires_at| expires_at > now_text.as_str())
            && grant
                .subject_id
                .as_deref()
                .is_none_or(|scoped| scoped == subject_id)
    })
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

async fn profile_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
    profile_kind: &str,
) -> StoreResult<Option<ProfileRow>> {
    let sql = format!(
        "SELECT {PROFILE_COLUMNS} FROM memory_profiles \
         WHERE space_id = ? AND subject_id = ? AND profile_kind = ?"
    );
    Ok(sqlx::query_as::<_, ProfileRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(subject_id)
        .bind(profile_kind)
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
/// outcome and bump the revision in one statement guarded by the CAS, then
/// project the consumed shell as the operation's file effect (§5.2 seam 2).
async fn consume_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    candidate: &CandidateRow,
    expected_revision: i64,
    consumed_by: &Value,
    content_digest: &str,
) -> StoreResult<FileEffect> {
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
    let shell = candidate_in_tx(
        tx,
        candidate.space_id.as_str(),
        candidate.candidate_id.as_str(),
    )
    .await?
    .ok_or_else(|| {
        StoreError::Corrupt(format!(
            "candidate {} vanished after consumption",
            candidate.candidate_id
        ))
    })?;
    journal::candidate_effect(&shell)
}

/// §5.2 seam 1: insert the record, then project the stored row as the
/// operation's file effect.
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
) -> StoreResult<FileEffect> {
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
    record_effect_in_tx(tx, space_id, record_id).await
}

/// Re-read one stored record and project it (§5.2 seams 1 and 3).
async fn record_effect_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    record_id: &str,
) -> StoreResult<FileEffect> {
    let row = record_in_tx(tx, space_id, record_id)
        .await?
        .ok_or_else(|| StoreError::Corrupt(format!("record {record_id} is missing")))?;
    journal::record_effect(&row)
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
    ///
    /// The page is cut in SQL and the marker digests the whole authorized
    /// identity set, so the bodies of rows past the page never cross the read
    /// (S-10). Per-row `review` coverage still depends on each row's placement,
    /// so the page is also filtered by it — the cursor walks the predicate set,
    /// never the filtered result, and a short page never ends the listing.
    #[allow(clippy::too_many_lines)]
    pub(super) async fn candidates(
        &self,
        space_id: &str,
        subject_id: &str,
        status: Option<&str>,
        principal_id: &str,
        page: Option<(u64, u64)>,
    ) -> StoreResult<(Vec<CandidateRow>, String, bool)> {
        let (mut tx, now) = self.begin_read().await?;
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
        let subject_status = subject_in_tx(&mut tx, space_id, subject_id)
            .await?
            .map(|subject| subject.status);
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
        // §7.7: `archived` hides the subject from the *default* listing; a
        // status-filtered listing is an explicit audit query and stays readable.
        // §5.2/D-10: an unsettled intent withholds the subject either way.
        let archived = status.is_none() && subject_status.as_deref() == Some("archived");
        let hidden = withheld(unsettled.as_deref(), subject_id)?;
        if archived || hidden {
            tx.commit().await?;
            let marker =
                snapshot_marker(std::iter::empty(), &grant_rows, subject_status.as_deref())?;
            return Ok((Vec::new(), marker, false));
        }
        // Constant predicates, as separate statements rather than
        // `(? IS NULL OR status = ?)`: that shape defeats the status index, and
        // §6.7 expiry belongs to the read, not to a post-filter over every row
        // (S-10/S-9).
        let mut where_sql = String::from(" WHERE space_id = ? AND subject_id = ?");
        if status.is_some() {
            where_sql.push_str(" AND status = ?");
        }
        where_sql.push_str(
            " AND NOT (status = 'pending' AND expires_at IS NOT NULL AND expires_at <= ?)",
        );
        let page_clause = if page.is_some() {
            " LIMIT ? OFFSET ?"
        } else {
            ""
        };
        let page_sql = format!(
            "SELECT {CANDIDATE_COLUMNS} FROM memory_candidates{where_sql} \
             ORDER BY created_at, candidate_id{page_clause}"
        );
        let mut rows_query = sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(page_sql))
            .bind(space_id)
            .bind(subject_id);
        if let Some(status) = status {
            rows_query = rows_query.bind(status);
        }
        rows_query = rows_query.bind(timestamp(now));
        if let Some((limit, offset)) = page {
            rows_query = rows_query
                .bind(page_limit_with_probe(limit))
                .bind(i64::try_from(offset).unwrap_or(i64::MAX));
        }
        let mut rows = rows_query.fetch_all(&mut *tx).await?;
        let more = page.is_some_and(|(limit, _)| page_had_more(&mut rows, limit));
        // The authorized identity set: the marker digests identity + revision
        // for every row that can be on this listing, never their bodies.
        let mut identity_sql = String::from(
            "SELECT candidate_id, subject_id, capture_visibility, capture_context_id, revision \
             FROM memory_candidates",
        );
        identity_sql.push_str(&where_sql);
        let mut identity_query =
            sqlx::query_as::<_, (String, String, String, Option<String>, i64)>(
                sqlx::AssertSqlSafe(identity_sql),
            )
            .bind(space_id)
            .bind(subject_id);
        if let Some(status) = status {
            identity_query = identity_query.bind(status);
        }
        let identity = identity_query
            .bind(timestamp(now))
            .fetch_all(&mut *tx)
            .await?;
        tx.commit().await?;
        let visible: Vec<CandidateRow> = rows
            .into_iter()
            .filter(|row| {
                covered(
                    &grants,
                    principal_id,
                    space_id,
                    now,
                    &row.requirement("review"),
                )
            })
            .collect();
        let marker = snapshot_marker(
            identity
                .iter()
                .filter(|row| {
                    covered(
                        &grants,
                        principal_id,
                        space_id,
                        now,
                        &candidate_requirement(
                            "review",
                            row.1.as_str(),
                            row.2.as_str(),
                            row.3.as_deref(),
                        ),
                    )
                })
                .map(|row| (row.0.as_str(), row.4)),
            &grant_rows,
            subject_status.as_deref(),
        )?;
        Ok((visible, marker, more))
    }

    /// §7.13: by-id read under current authorization (§7.17-style; an
    /// unauthorized candidate is reported as absent).
    pub(super) async fn candidate_for_read(
        &self,
        space_id: &str,
        candidate_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<CandidateRow>> {
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = candidate_in_tx(&mut tx, space_id, candidate_id).await?;
        let unsettled = match &row {
            Some(row) => unsettled_in(&mut tx, space_id, row.subject_id.as_str()).await?,
            None => None,
        };
        tx.commit().await?;
        // §5.2/D-10: the quarantine refusal is a disclosure of both existence
        // and subject, so coverage is decided first — an unauthorized probe gets
        // the same absence it gets for any candidate.
        let Some(row) = row.filter(|row| {
            covered(
                &grants,
                principal_id,
                space_id,
                now,
                &row.requirement("review"),
            )
        }) else {
            return Ok(None);
        };
        if withheld(unsettled.as_deref(), row.subject_id.as_str())? {
            return Ok(None);
        }
        Ok(Some(row))
    }

    /// §7.14: dismissal consumes the candidate without producing records.
    pub(super) async fn dismiss_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        expected_revision: i64,
    ) -> StoreResult<ReceiptRow> {
        let targets = [journal::candidate_target(candidate_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
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
        let effect = consume_candidate(
            &mut tx,
            &candidate,
            expected_revision,
            &consumed_by,
            &digest,
        )
        .await?;
        let result = json!({ "candidate_id": candidate_id, "outcome": "dismissed" });
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            receipt_id,
            candidate.subject_id.as_str(),
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
    }

    /// §7.15: consumption + scrub + every record insert + receipt, one
    /// transaction (MEM-R009). Placement is inherited from the candidate and can
    /// never be chosen by the decision.
    #[allow(clippy::too_many_lines)]
    pub(super) async fn review_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        expected_revision: i64,
        decision: &ReviewDecision<'_>,
    ) -> StoreResult<ReceiptRow> {
        let targets = [journal::candidate_target(candidate_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
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
        let mut effects = Vec::new();
        match decision {
            ReviewDecision::Promote {
                kind,
                content: override_content,
            } => {
                let record_id = mint_record_id();
                effects.push(
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
                    .await?,
                );
                record_ids.push(record_id);
            }
            ReviewDecision::Fragment { items } => {
                for (kind, fragment) in *items {
                    let record_id = mint_record_id();
                    effects.push(
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
                        .await?,
                    );
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
        effects.push(
            consume_candidate(
                &mut tx,
                &candidate,
                expected_revision,
                &consumed_by,
                &digest,
            )
            .await?,
        );
        let result = json!({ "outcome": decision.outcome(), "record_ids": record_ids });
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            receipt_id,
            candidate.subject_id.as_str(),
            &effects,
            &result,
            now,
        )
        .await
    }

    /// §7.20/§7.22 read path: authorization, and every constant filter
    /// (tombstone, kind, selection, provenance) are applied by the SQL that cuts
    /// the page, so a page fetches only its own rows (S-10). The invalidation
    /// marker is a digest over the full authorized identity set, and
    /// `total_authorized` is that set's size — both read in the same snapshot as
    /// the page, so the cursor still covers only what the principal may see
    /// (MEM-R006).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_lines)]
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
        page: Option<(u64, u64)>,
    ) -> StoreResult<(Vec<RecordRow>, String, u64)> {
        let (mut tx, now) = self.begin_read().await?;
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
        let subject_status = subject_in_tx(&mut tx, space_id, subject_id)
            .await?
            .map(|subject| subject.status);
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
        // §7.7: an archived subject's records leave recall; the listing and
        // by-id audit routes still serve them.
        // §5.2/D-10: an unsettled file intent withholds the subject from both.
        let archived = read.excludes_archived() && subject_status.as_deref() == Some("archived");
        let hidden = withheld(unsettled.as_deref(), subject_id)?;
        let subject_all = matches!(selection, Selection::SubjectAll);
        if archived || hidden {
            tx.commit().await?;
            let marker =
                snapshot_marker(std::iter::empty(), &grant_rows, subject_status.as_deref())?;
            return Ok((Vec::new(), marker, 0));
        }
        let (where_sql, binds) = record_predicate(
            selection,
            filter,
            kinds,
            include_tombstoned,
            space_id,
            subject_id,
        );
        // `Some((limit, offset))` cuts the page in SQL; `None` is recall, which
        // ranks the whole authorized set (§9.2) and so cannot page before it.
        let page_clause = if page.is_some() {
            " LIMIT ? OFFSET ?"
        } else {
            ""
        };
        let page_sql = format!(
            "SELECT {RECORD_COLUMNS} FROM memory_records{where_sql} \
             ORDER BY created_at DESC, record_id{page_clause}"
        );
        let mut rows_query = sqlx::query_as::<_, RecordRow>(sqlx::AssertSqlSafe(page_sql));
        for bind in &binds {
            rows_query = rows_query.bind(bind);
        }
        if let Some((limit, offset)) = page {
            rows_query = rows_query
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .bind(i64::try_from(offset).unwrap_or(i64::MAX));
        }
        let rows = rows_query.fetch_all(&mut *tx).await?;
        // The authorized identity set: the marker and the count, never the
        // bodies of rows the page will not serve. A whole-set read already has
        // them.
        let visible: Vec<(String, i64)> = if page.is_some() {
            let identity_sql = format!(
                "SELECT record_id, revision FROM memory_records{where_sql} \
                 ORDER BY created_at DESC, record_id"
            );
            let mut identity =
                sqlx::query_as::<_, (String, i64)>(sqlx::AssertSqlSafe(identity_sql));
            for bind in &binds {
                identity = identity.bind(bind);
            }
            identity.fetch_all(&mut *tx).await?
        } else {
            rows.iter()
                .map(|row| (row.record_id.clone(), row.revision))
                .collect()
        };
        tx.commit().await?;
        // §6.5: the selection check above covers every row the selection admits
        // (a grant covering all of its named contexts covers each of them), so
        // the page is a slice of the authorized set and never needs a second
        // row-level refusal. This asserts that invariant in tests.
        debug_assert!(
            rows.iter().all(|row| required.iter().any(|capability| {
                covered(
                    &grants,
                    principal_id,
                    space_id,
                    now,
                    &row.requirement(capability, subject_all),
                )
            })),
            "the SQL page must be a slice of the authorized record set"
        );
        let marker = snapshot_marker(
            visible
                .iter()
                .map(|(record_id, revision)| (record_id.as_str(), *revision)),
            &grant_rows,
            subject_status.as_deref(),
        )?;
        Ok((
            rows,
            marker,
            u64::try_from(visible.len()).unwrap_or(u64::MAX),
        ))
    }

    /// §7.17: by-id read; a tombstoned record is admin-only and an unauthorized
    /// record reads as absent.
    pub(super) async fn record_for_read(
        &self,
        space_id: &str,
        record_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<RecordRow>> {
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = record_in_tx(&mut tx, space_id, record_id).await?;
        let unsettled = match &row {
            Some(row) => unsettled_in(&mut tx, space_id, row.subject_id.as_str()).await?,
            None => None,
        };
        tx.commit().await?;
        // §5.2/D-10: the quarantine refusal is a disclosure of both existence
        // and subject, so coverage is decided first — an unauthorized probe gets
        // the same absence it gets for any record.
        let Some(row) = row.filter(|row| {
            let subject_id = row.subject_id.as_str();
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
            let admin = GrantRequirement {
                capability: "admin",
                subject_id: Some(subject_id),
                context_ids: Vec::new(),
                reads_shared: false,
                subject_all: false,
            };
            !row.tombstoned() || covered(&grants, principal_id, space_id, now, &admin)
        }) else {
            return Ok(None);
        };
        if withheld(unsettled.as_deref(), row.subject_id.as_str())? {
            return Ok(None);
        }
        Ok(Some(row))
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
        let record_id = mint_record_id();
        // A create owns a target nothing has ever written, so its predecessor
        // check is trivially clear — it still runs like every other publication.
        let targets = [journal::record_target(&record_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        gate(&mut tx, admission).await?;
        let effect = insert_record(
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
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            subject_id,
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
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
        let targets = [journal::record_target(record_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
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
        // The projection this update replaces: the provider's own previous
        // version of the row, never whatever bytes sit at the target.
        let prior = journal::record_effect(&record)?.digest;
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
        let effect = record_effect_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .over(Some(prior));
        let result = json!({ "record_id": record_id, "revision": record.revision + 1 });
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            record.subject_id.as_str(),
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
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
        let targets = [journal::record_target(record_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
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
        // The content-bearing projection this deletion replaces; only its
        // digest is journaled, so the removed body is never retained.
        let prior = journal::record_effect(&record)?.digest;
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
        let effect = record_effect_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .over(Some(prior));
        let result = json!({ "record_id": record_id, "tombstoned": true });
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            record.subject_id.as_str(),
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
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
        let targets = [journal::record_target(record_id)];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
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
        // The projection this promotion replaces (unused by a §7.0.4 no-op).
        let prior = journal::record_effect(&record)?.digest;
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
        if revision == record.revision {
            // §7.0.4 no-op: the stored row is unchanged, so the projection is too.
            return self.commit_receipt(tx, ctx, &result, now).await;
        }
        let effect = record_effect_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .over(Some(prior));
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            record.subject_id.as_str(),
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
    }

    /// §7.23 profile read: every present profile, or the requested kinds that
    /// are present. Absence is honest-empty, never an error.
    pub(super) async fn profiles(
        &self,
        space_id: &str,
        subject_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<Vec<ProfileRow>>> {
        let (mut tx, now) = self.begin_read().await?;
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
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
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
        // §5.2/D-10: a subject whose file intent is unsettled has no readable
        // profile surface yet.
        if withheld(unsettled.as_deref(), subject_id)? {
            return Ok(None);
        }
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
        let targets = [journal::profile_target(subject_id, profile_kind)?];
        let mut publication = self.begin_publication(ctx, admission, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        gate(&mut tx, admission).await?;
        let existing = profile_in_tx(&mut tx, ctx.space_id, subject_id, profile_kind).await?;
        // The projection this upsert replaces: the provider's own previous
        // profile document, absent on the first write.
        let prior = existing
            .as_ref()
            .map(journal::profile_effect)
            .transpose()?
            .map(|effect| effect.digest);
        let at = timestamp(now);
        let revision = match (existing.as_ref().map(|row| row.revision), expected_revision) {
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
            (Some(current), None) => {
                // `expected_revision: null` means create-only (§7.24).
                return Err(StoreError::RevisionConflict { current });
            }
            (Some(current), Some(expected)) => {
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
        let profile = profile_in_tx(&mut tx, ctx.space_id, subject_id, profile_kind)
            .await?
            .ok_or_else(|| {
                StoreError::Corrupt(format!(
                    "profile {profile_kind} of subject {subject_id} is missing after the write"
                ))
            })?;
        let effect = journal::profile_effect(&profile)?.over(prior);
        let result = json!({ "profile_kind": profile_kind, "revision": revision });
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            subject_id,
            std::slice::from_ref(&effect),
            &result,
            now,
        )
        .await
    }
}

/// §6.13/§7.20/§7.22: the SQL predicate of a record page, with its binds in
/// placeholder order. Every term is constant for the request — none depends on
/// the row's authorization (that is decided once, for the whole selection) — so
/// pushing it into the read is what lets the page be cut in SQL.
fn record_predicate(
    selection: &Selection,
    filter: Option<&[String]>,
    kinds: &[String],
    include_tombstoned: bool,
    space_id: &str,
    subject_id: &str,
) -> (String, Vec<String>) {
    let mut sql = String::from(" WHERE space_id = ? AND subject_id = ?");
    let mut binds = vec![space_id.to_string(), subject_id.to_string()];
    if !include_tombstoned {
        sql.push_str(" AND tombstone_json IS NULL");
    }
    if !kinds.is_empty() {
        sql.push_str(" AND kind IN (");
        push_placeholders(&mut sql, kinds, &mut binds);
        sql.push(')');
    }
    match selection {
        Selection::SubjectAll => {}
        Selection::SharedOnly => sql.push_str(" AND scope_visibility = 'shared'"),
        Selection::SelectedContexts {
            context_ids,
            include_shared,
        } => {
            if context_ids.is_empty() && !include_shared {
                // Neither a named context nor shared placement is admitted.
                sql.push_str(" AND 0");
            } else if context_ids.is_empty() {
                sql.push_str(" AND scope_visibility = 'shared'");
            } else {
                sql.push_str(" AND (");
                if *include_shared {
                    sql.push_str("scope_visibility = 'shared' OR ");
                }
                sql.push_str("scope_context_id IN (");
                push_placeholders(&mut sql, context_ids, &mut binds);
                sql.push_str("))");
            }
        }
    }
    if let Some(context_ids) = filter {
        if context_ids.is_empty() {
            sql.push_str(" AND 0");
        } else {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM json_each(json_extract(provenance_json, \
                 '$.source_context_ids')) AS source WHERE source.value IN (",
            );
            push_placeholders(&mut sql, context_ids, &mut binds);
            sql.push_str("))");
        }
    }
    (sql, binds)
}

/// Append one `?` per value and collect the values in bind order.
fn push_placeholders(sql: &mut String, values: &[String], binds: &mut Vec<String>) {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            sql.push(',');
        }
        sql.push('?');
        binds.push(value.clone());
    }
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
