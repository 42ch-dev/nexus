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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicU64;
#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicU8;
#[cfg(feature = "test-hooks")]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use spoke_operations::spoke_schemas::MemoryGrant;
use spoke_operations::{
    grant_covers, is_freeze_allowlisted, validate_grant_issuance, GrantRequirement, IssuedGrant,
    SpokeResult,
};
use sqlx::{Sqlite, SqlitePool};
use uuid::Uuid;

use nexus_local_db::writer_protocol::{init_guarded_pool, GuardedPool};
use nexus_local_db::{begin_immediate, LocalDbError};

use super::created_at_cmp;
use super::journal::{self, FileEffect};
use super::legacy::{self, LegacyRoots};

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
    /// The request names something the store cannot admit (§7/I-000149: a
    /// second subject naming a bearer that already has one).
    InvalidInput {
        field: String,
        reason: String,
    },
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
    /// The store was opened read-only: a mutation is refused before admission,
    /// retention, the publication lock or any connection write (fix round 2,
    /// finding F3).
    ReadOnly(String),
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

/// A capture transaction's two failure classes.
///
/// The store's own refusal travels as [`StoreError`] and maps onto the port
/// vocabulary. The caller's binding provenance (compass D15) is not a provider
/// refusal — it is the legacy-domain admission the Character capture always
/// answered — so it travels as itself and the adapter keeps that error.
pub(super) enum CaptureError {
    Store(StoreError),
    Binding(LocalDbError),
}

impl From<StoreError> for CaptureError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

impl From<sqlx::Error> for CaptureError {
    fn from(value: sqlx::Error) -> Self {
        Self::Store(StoreError::from(value))
    }
}

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

/// Proof seam: the rendezvous a paused write waits at once it has entered the
/// provider's write-admission machinery (past the read-only gate and the
/// admission counter) and before it opens its writer transaction or runs
/// retention. `entered` reports that arrival; `release` frees it. Test only.
#[cfg(feature = "test-hooks")]
struct AdmissionRendezvous {
    /// Monotonic identity of this arm: a stale test handle can only disarm the
    /// arm it owns, never a replacement.
    token: u64,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Barrier>,
}

/// Upper bound on the provider's wait at a parked admission. A test that
/// disappears, cancels, or never releases cannot leave a write parked forever:
/// after the bound the write proceeds normally (the pause never changes the
/// operation's outcome).
#[cfg(feature = "test-hooks")]
const ADMISSION_HOLD_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

/// D15 proof seam: the rendezvous a parked capture waits at — `entered` reports
/// that it holds its write transaction and has already validated the caller's
/// binding, `release` frees it. Test only.
#[cfg(feature = "test-hooks")]
struct CaptureRendezvous {
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
    /// `None` on a read-only store, which holds no admission at all.
    guarded: Option<GuardedPool>,
    /// Root of the provider's own markdown projection, next to `state.db`.
    files_root: PathBuf,
    /// Legacy bearer roots read by the projection bridge ([`super::legacy`]).
    /// Held here so every composed read path projects the same sources.
    legacy: LegacyRoots,
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
    /// Whether this database carries the provider schema. A read-only open can
    /// be taken over a workspace no writer ever provisioned; its reads then
    /// answer the space's absence instead of a driver error (fix round 2,
    /// finding F2).
    provisioned: AtomicBool,
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
    /// Test-only rendezvous a write waits at once it has entered the
    /// write-admission machinery and before its writer transaction — `entered`
    /// tells the test the write arrived, `release` frees it — so a competing
    /// transition can be observed at the contested boundary.
    #[cfg(feature = "test-hooks")]
    admission_barrier: std::sync::Mutex<Option<AdmissionRendezvous>>,
    /// Monotonic token source for admission arms.
    #[cfg(feature = "test-hooks")]
    admission_token: AtomicU64,
    /// Test-only rendezvous a capture waits at once its write transaction holds
    /// the lock and has validated the caller's binding — `entered` tells the
    /// test the capture is parked, `release` frees it — so a test can show a
    /// concurrent `remove_binding` cannot interleave (compass D15).
    #[cfg(feature = "test-hooks")]
    capture_barrier: std::sync::Mutex<Option<CaptureRendezvous>>,
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
    /// Test-only count of write entries that reached the admission or
    /// publication machinery, i.e. passed the read-only gate. A refused
    /// read-only write leaves it at zero (fix round 3, finding F5).
    #[cfg(feature = "test-hooks")]
    lock_attempts: AtomicUsize,
    /// Test-only report a disposition acquisition sends **from inside the lock
    /// attempt** when its non-blocking probe finds the section held, i.e. the
    /// caller is really about to block on the OS lock (fix round 4, finding
    /// R4-F3).
    #[cfg(feature = "test-hooks")]
    disposition_wait_report: std::sync::Mutex<Option<Arc<tokio::sync::Notify>>>,
}

const SPACE_COLUMNS: &str = "s.space_id, s.display_name, s.authority_host_id, \
     s.authority_generation, s.status, s.revision, s.created_at, \
     (SELECT COUNT(*) FROM memory_receipts r WHERE r.space_id = s.space_id \
      AND r.status = 'pending') AS pending_receipts";

pub(super) const SUBJECT_COLUMNS: &str = "space_id, subject_id, kind_ns, kind_id, status, \
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
    /// Close this provider's own admitted pool.
    ///
    /// The provider holds the only handle to its writer admission, so closing
    /// the pool here — explicitly, at the end of this provider's lifetime — is
    /// what lets the cooperative admission settle. A read-only store simply
    /// closes its read-only pool; it never held an admission. Idempotent.
    pub(super) async fn close(&self) {
        self.pool.close().await;
    }

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
        // A read-only store refuses before the retention sweep and before the
        // connection is touched (fix round 2, finding F3).
        self.require_writable()?;
        #[cfg(feature = "test-hooks")]
        self.lock_attempts.fetch_add(1, Ordering::SeqCst);
        #[cfg(feature = "test-hooks")]
        self.admission_gate().await;
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
    pub(super) async fn open(
        db_path: &Path,
        creator_id: &str,
        user_home: &Path,
    ) -> StoreResult<Self> {
        let guarded = init_guarded_pool(db_path, creator_id).await?;
        let pool = guarded.clone_pool();
        Self::compose(pool, Some(guarded), db_path, creator_id, user_home).await
    }

    /// Open the same store over a **read-only** connection (NXM-5 Task 2 fix
    /// round 1, finding F6).
    ///
    /// No writer admission, no space provisioning, no journal recovery and no
    /// retention sweep: a read-only core can serve memory reads without writing
    /// anything. Every write path still refuses (the core refuses it before it
    /// reaches this store), so this mode has no effect half at all.
    ///
    /// A database no writer open has migrated yet carries no provider tables;
    /// the retention clock then starts at the same zero floor a fresh storage
    /// has, instead of failing the whole read-only open over an absent table.
    pub(super) async fn open_read_only(
        db_path: &Path,
        creator_id: &str,
        user_home: &Path,
    ) -> StoreResult<Self> {
        let pool = nexus_local_db::open_pool_read_only(db_path).await?;
        Self::compose(pool, None, db_path, creator_id, user_home).await
    }

    /// Whether this store may write. A read-only store holds no writer
    /// admission; its reads include the legacy projection exactly as a
    /// writer-open store's do.
    pub(super) const fn writable(&self) -> bool {
        self.guarded.is_some()
    }

    /// The directory of the workspace memory-disposition lock files (fix round
    /// 2, finding F1): next to `state.db`, so two providers over one workspace —
    /// in this process or in another — contend on the same files.
    pub(super) fn disposition_dir(&self) -> PathBuf {
        self.files_root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("memory_locks")
    }

    async fn compose(
        pool: SqlitePool,
        guarded: Option<GuardedPool>,
        db_path: &Path,
        creator_id: &str,
        user_home: &Path,
    ) -> StoreResult<Self> {
        let files_root = db_path.parent().map_or_else(
            || PathBuf::from(journal::FILES_DIR),
            |dir| dir.join(journal::FILES_DIR),
        );
        let clock_floor = Self::stored_clock_floor(&pool).await?;
        let provisioned = Self::provider_tables_present(&pool).await?;
        let publish_gate = workspace_publish_gate(&files_root);
        Ok(Self {
            pool,
            guarded,
            files_root,
            legacy: LegacyRoots::new(user_home.to_path_buf(), creator_id),
            publish_gate,
            clock_floor: AtomicI64::new(clock_floor),
            provisioned: AtomicBool::new(provisioned),
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
            admission_barrier: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            admission_token: AtomicU64::new(0),
            #[cfg(feature = "test-hooks")]
            capture_barrier: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            publish_barrier: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            flock_wait_report: std::sync::Mutex::new(None),
            #[cfg(feature = "test-hooks")]
            lock_attempts: AtomicUsize::new(0),
            #[cfg(feature = "test-hooks")]
            disposition_wait_report: std::sync::Mutex::new(None),
        })
    }

    /// Whether the provider schema exists in this database. A read-only open
    /// can precede any writer provisioning it.
    async fn provider_tables_present(pool: &SqlitePool) -> StoreResult<bool> {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master \
             WHERE type = 'table' AND name = 'memory_spaces')",
        )
        .fetch_one(pool)
        .await?;
        Ok(present)
    }

    /// Whether the provider schema exists **now**: the open-time snapshot once
    /// it is true, otherwise a fresh probe of the database (fix round 3,
    /// finding F2).
    ///
    /// The snapshot alone went stale for a read-only instance that opened before
    /// any writer provisioned the workspace: the only other update is the writer
    /// instance's own `provision`. The probe is a read — no durable write — and
    /// runs only while the cache says absent, so a provisioned store never
    /// re-checks on the read path.
    async fn provider_schema_present(&self) -> StoreResult<bool> {
        if self.provisioned.load(Ordering::Relaxed) {
            return Ok(true);
        }
        let present = Self::provider_tables_present(&self.pool).await?;
        self.provisioned.store(present, Ordering::Relaxed);
        Ok(present)
    }

    /// Refuse a read against a database that carries no provider schema: the
    /// space genuinely does not exist there (fix round 2, finding F2), so the
    /// answer is the space's absence rather than a driver error.
    async fn require_provisioned(&self, space_id: &str) -> StoreResult<()> {
        if self.provider_schema_present().await? {
            Ok(())
        } else {
            Err(StoreError::NotFound(format!("space {space_id}")))
        }
    }

    /// Refuse a mutation on a read-only store before it reaches retention, the
    /// publication lock or the connection (fix round 2, finding F3).
    fn require_writable(&self) -> StoreResult<()> {
        if self.writable() {
            Ok(())
        } else {
            Err(StoreError::ReadOnly(
                "this memory provider was opened read-only; write operations are refused"
                    .to_string(),
            ))
        }
    }

    /// The durable retention floor, or `0` when the provider has no meta table
    /// yet (a database only ever opened read-only).
    async fn stored_clock_floor(pool: &SqlitePool) -> StoreResult<i64> {
        let present: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master \
             WHERE type = 'table' AND name = 'memory_provider_meta')",
        )
        .fetch_one(pool)
        .await?;
        if !present {
            return Ok(0);
        }
        let persisted: Option<String> =
            sqlx::query_scalar("SELECT value FROM memory_provider_meta WHERE key = ?")
                .bind(CLOCK_FLOOR_KEY)
                .fetch_optional(pool)
                .await?;
        Ok(persisted
            .map(|value| {
                value.parse::<i64>().map_err(|error| {
                    StoreError::Corrupt(format!("memory_provider_meta.{CLOCK_FLOOR_KEY}: {error}"))
                })
            })
            .transpose()?
            .unwrap_or(0))
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

    /// D15 proof seam: park the next capture once its write transaction holds
    /// the lock and has validated the caller's binding — `entered` signals it is
    /// parked, `release` frees it — so a test can show a concurrent
    /// `remove_binding` cannot slip between that validation and the insert.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn park_next_capture(
        &self,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Barrier>,
    ) {
        *self.capture_barrier.lock().expect("capture gate") =
            Some(CaptureRendezvous { entered, release });
    }

    /// Park the next write once it has entered the write-admission machinery and
    /// before its writer transaction (proof seam). `entered` reports that
    /// arrival, `release` frees it. Single next use, like the capture seam.
    #[cfg(feature = "test-hooks")]
    pub(super) fn park_next_admission(
        &self,
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Barrier>,
    ) -> u64 {
        let token = self.admission_token.fetch_add(1, Ordering::SeqCst) + 1;
        *self.admission_barrier.lock().expect("admission gate") = Some(AdmissionRendezvous {
            token,
            entered,
            release,
        });
        token
    }

    /// Disarm exactly the admission arm `token` owns (test control). A stale
    /// handle cannot clear a newer arm; once the provider has taken an arm it is
    /// no longer in the slot and this is a no-op (the provider's bounded wait
    /// covers that case).
    #[cfg(feature = "test-hooks")]
    pub(super) fn disarm_next_admission(&self, token: u64) {
        let mut gate = self.admission_barrier.lock().expect("admission gate");
        if gate.as_ref().is_some_and(|armed| armed.token == token) {
            *gate = None;
        }
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

    /// NXM-5 Task 2 fix round 4 (finding R4-F3) proof seam: report, from inside
    /// the lock attempt, that the section is held and the caller is about to
    /// block on the OS lock. The report fires only after the non-blocking probe
    /// has failed, so a regression that observes it knows the dismissal really
    /// reached the contended lock, not merely that its task has been scheduled.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn report_disposition_waits(&self, report: Arc<tokio::sync::Notify>) {
        *self
            .disposition_wait_report
            .lock()
            .expect("disposition wait report") = Some(report);
    }

    /// The armed report of [`Self::report_disposition_waits`], if any.
    #[cfg(feature = "test-hooks")]
    pub(super) fn take_disposition_wait_report(&self) -> Option<Arc<tokio::sync::Notify>> {
        self.disposition_wait_report
            .lock()
            .expect("disposition wait report")
            .take()
    }

    /// NXM-5 Task 2 fix round 3 (finding F5) proof seam: how many write entries
    /// passed the read-only gate and reached the admission or publication
    /// machinery. Zero after refused writes proves no lock was attempted.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    pub(super) fn lock_attempts(&self) -> usize {
        self.lock_attempts.load(Ordering::SeqCst)
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

    /// The [`Self::park_next_admission`] wait, taken at the same point in every
    /// write: after the read-only gate and the admission counter, before the
    /// retention pass and before the writer transaction.
    #[cfg(feature = "test-hooks")]
    async fn admission_gate(&self) {
        let parked = {
            let mut gate = self.admission_barrier.lock().expect("admission gate");
            gate.take()
        };
        if let Some(rendezvous) = parked {
            rendezvous.entered.notify_one();
            // Bounded: a vanished, cancelled or never-releasing test cannot park
            // this write forever, and expiry proceeds normally (the pause never
            // changes the operation's outcome).
            let _ = tokio::time::timeout(ADMISSION_HOLD_BOUND, rendezvous.release.wait()).await;
        }
    }

    /// The [`Self::park_next_capture`] wait, taken at the same point in every
    /// capture: past the caller's binding validation, before any insert.
    #[cfg(feature = "test-hooks")]
    async fn capture_gate(&self) {
        let parked = {
            let mut gate = self.capture_barrier.lock().expect("capture gate");
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
        self.provisioned.store(true, Ordering::Relaxed);
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
        // A workspace with no provider schema has no receipt and no space
        // (fix round 3, finding F3): the declared absence, not a driver error.
        self.require_provisioned(space_id).await?;
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
        // Same absence policy as the other space-scoped reads (fix round 3,
        // finding F3): authorization is unchanged where the space exists.
        self.require_provisioned(space_id).await?;
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
        // A workspace no writer ever provisioned carries no provider space
        // (fix round 2, finding F2): an empty listing, not a driver error. The
        // check re-probes while the cache says absent, so a later writer
        // provisioning is observed here (fix round 3, finding F2).
        if !self.provider_schema_present().await? {
            return Ok((Vec::new(), "0".to_string()));
        }
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
            // §7/I-000149: one subject owns a bearer naming. Reads resolve a
            // bearer to the smallest subject naming it, so admitting a second
            // naming here would let a later, smaller ensure re-assign the
            // projection and resurrect a consumed legacy candidate. Refused in
            // this transaction, before insertion.
            if let Some((kind_ns, kind_id)) = kind {
                if let Some(owner) = subject_naming(&mut tx, ctx.space_id, kind_ns, kind_id).await?
                {
                    return Err(StoreError::InvalidInput {
                        field: "kind".to_string(),
                        reason: format!(
                            "subject {} already names {kind_ns}/{kind_id} in space {}; one subject owns a bearer",
                            owner.subject_id, ctx.space_id
                        ),
                    });
                }
            }
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
        // The grants page is space-scoped too: a workspace with no provider
        // schema answers the space's absence (fix round 3, finding F3).
        self.require_provisioned(space_id).await?;
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
        // The publication path refuses a read-only store before it takes this
        // process's gate or the workspace flock (fix round 2, finding F3).
        self.require_writable()?;
        #[cfg(feature = "test-hooks")]
        self.lock_attempts.fetch_add(1, Ordering::SeqCst);
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
        // I-000305: a changed payload under a retained key conflicts before
        // the authorization resolves the request's target row, so a changed
        // target whose candidate is gone answers the key instead of
        // NOT_FOUND. The replay disclosure stays in `probe` below, behind
        // this authorization.
        conflict_only_probe(&mut tx, ctx, now).await?;
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

    /// The placement a committed promotion left for one record, while the cache
    /// invalidation it owes has not succeeded yet (R4-F3 round 2). `None` when
    /// no promotion of this record is outstanding.
    pub(super) async fn soul_invalidation_binding(
        &self,
        space_id: &str,
        record_id: &str,
    ) -> StoreResult<Option<String>> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM memory_provider_meta WHERE key = ?")
                .bind(soul_invalidation_key(space_id, record_id))
                .fetch_optional(&self.pool)
                .await?;
        Ok(value)
    }

    /// Clear that row once the invalidation it names ran; a repeated clear (a
    /// retry after a crash between the delete and this clear) is a no-op.
    pub(super) async fn clear_soul_invalidation(
        &self,
        space_id: &str,
        record_id: &str,
    ) -> StoreResult<()> {
        sqlx::query("DELETE FROM memory_provider_meta WHERE key = ?")
            .bind(soul_invalidation_key(space_id, record_id))
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
        // The receipt is already durable, so a settlement failure here is the
        // same uncertainty `commit_journalled` reports — never a retryable
        // `INTERNAL_ERROR` (§14.2/§14.3).
        self.settle_entry(publication, &entry)
            .await
            .map_err(|error| match error {
                StoreError::CommitUnknown(_) => error,
                other => StoreError::CommitUnknown(format!(
                    "journal settlement on replay after COMMIT: {}",
                    describe(&other)
                )),
            })
    }

    /// §5.2 step 3 on the completed-marker path (P5-F2): settle every unsettled
    /// intent this operation still owns, then refuse while one remains.
    ///
    /// P5's marker commits with its intent in one transaction, so a marker can
    /// outlive an unpublished projection. The pass reports the space complete
    /// only once it is published; whatever the re-drive cannot publish is the
    /// retryable [`StoreError::UnsettledPredecessor`] refusal, never a silent
    /// `already_complete`.
    async fn settle_own_intents(
        &self,
        publication: &mut PublicationGuard,
        ctx: &OpContext<'_>,
    ) -> StoreResult<()> {
        let entries: Vec<JournalEntry> = sqlx::query_as(
            "SELECT seq, receipt_id, subject_id, targets_json, result_json FROM memory_journal \
             WHERE space_id = ? AND op = ? AND state <> 'applied' ORDER BY seq",
        )
        .bind(ctx.space_id)
        .bind(ctx.op)
        .fetch_all(&self.pool)
        .await?;
        for entry in &entries {
            // A settle that cannot publish records its quarantine; the re-read
            // below decides whether the projection is still owed.
            self.settle_intent(publication, entry).await;
        }
        let unsettled: Option<(String, String)> = sqlx::query_as(
            "SELECT receipt_id, state FROM memory_journal \
             WHERE space_id = ? AND op = ? AND state <> 'applied' ORDER BY seq LIMIT 1",
        )
        .bind(ctx.space_id)
        .bind(ctx.op)
        .fetch_optional(&self.pool)
        .await?;
        unsettled.map_or(Ok(()), |(receipt_id, state)| {
            Err(StoreError::UnsettledPredecessor { receipt_id, state })
        })
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

/// The `memory_provider_meta` row holding the placement a committed
/// Character-fragment promotion left behind, until its SOUL-narrative cache is
/// invalidated (R4-F3 round 2).
fn soul_invalidation_key(space_id: &str, record_id: &str) -> String {
    format!("soul_invalidation:{space_id}:{record_id}")
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

/// The unsettled journal state that fences one subject (D-10 read isolation).
///
/// The row's own `subject_id` fences its subject, and so does every subject its
/// recorded outcome lists as committed-but-unpublished: P5's migration commits
/// file effects for **every** bearer in one intent, so fencing only the row's
/// own subject left the other bearers readable while their files were owed
/// (P5-F1).
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
         WHERE space_id = ? AND state <> 'applied' \
           AND (subject_id = ? OR EXISTS ( \
               SELECT 1 FROM json_each(json_extract(result_json, '$.fenced_subjects')) \
               WHERE json_each.value = ?)) \
         ORDER BY CASE state WHEN ? THEN 0 WHEN ? THEN 1 ELSE 2 END, created_at LIMIT 1",
    )
    .bind(space_id)
    .bind(subject_id)
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
///
/// An op is admitted only when **one** grant satisfies every requirement of the
/// op — its required capabilities and the whole data access it names (§6.5:
/// "at least one grant that … includes the op's required capability … and
/// covers the op's data access"; the published `grant_covers` helper evaluates
/// one grant against one requirement). Two grants that each satisfy a different
/// requirement never union into an admission.
async fn authorize(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    admission: &Admission<'_>,
    now: DateTime<Utc>,
) -> StoreResult<Vec<GrantRow>> {
    space_in_tx(tx, admission.space_id).await?;
    let rows = grants_in_tx(tx, admission.space_id).await?;
    let grants = wire_grants(&rows)?;
    let refuse = |requirements: &[GrantRequirement<'_>]| -> StoreError {
        let capability = requirements
            .first()
            .map_or("any", |requirement| requirement.capability);
        StoreError::NotAuthorized(format!(
            "principal {} holds no grant covering {} in space {}",
            admission.principal_id, capability, admission.space_id
        ))
    };
    match &admission.coverage {
        Coverage::Requirements(requirements) => {
            if !one_grant_covers(
                &grants,
                admission.principal_id,
                admission.space_id,
                now,
                requirements,
            ) {
                return Err(refuse(requirements));
            }
        }
        Coverage::Candidate {
            candidate_id,
            capabilities,
        } => {
            let candidate = candidate_in_tx(tx, admission.space_id, candidate_id)
                .await?
                .ok_or_else(|| StoreError::NotFound(format!("candidate {candidate_id}")))?;
            let requirements: Vec<GrantRequirement<'_>> = capabilities
                .iter()
                .map(|capability| candidate.requirement(capability))
                .collect();
            if !one_grant_covers(
                &grants,
                admission.principal_id,
                admission.space_id,
                now,
                &requirements,
            ) {
                // C-102/MEM-R026: a by-ID candidate op hides incomplete coverage
                // exactly as it hides an id this store never held — the same
                // code AND the same message the absent row produces — so no
                // caller without coverage can learn that the candidate exists.
                return Err(StoreError::NotFound(format!("candidate {candidate_id}")));
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
            let mut requirements: Vec<GrantRequirement<'_>> = Vec::new();
            for capability in *capabilities {
                requirements.push(record.requirement(capability, false));
                if let Some(target) = target {
                    requirements.push(GrantRequirement {
                        capability,
                        subject_id: Some(record.subject_id.as_str()),
                        context_ids: target.context_id.map(str::to_string).into_iter().collect(),
                        reads_shared: target.visibility == Placement::SHARED,
                        subject_all: false,
                    });
                }
            }
            if !one_grant_covers(
                &grants,
                admission.principal_id,
                admission.space_id,
                now,
                &requirements,
            ) {
                return Err(refuse(&requirements));
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

/// Persist the placement one Character-fragment promotion leaves, in the
/// promotion's own transaction (R4-F3 round 2).
///
/// The record cannot carry its previous placement afterwards (the promotion
/// overwrites its scope) and its provenance is a client claim that need not name
/// the placement at all, so the post-commit cache invalidation reads this row
/// instead of guessing. Committed with the promotion: a promotion that does not
/// commit leaves no row.
async fn record_soul_invalidation(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    record_id: &str,
    context_id: &str,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO memory_provider_meta (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(soul_invalidation_key(space_id, record_id))
    .bind(context_id)
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

/// Judge only the payload-conflict half of the dedup namespace, before any
/// target lookup (I-000305).
///
/// A retained key bound to a different payload answers
/// `IDEMPOTENCY_CONFLICT` even when the request's target row is gone; the
/// pre-fix order let `authorize` resolve the candidate first and report the
/// missing target instead. The disclosure half — serving a same-payload
/// replay — stays with [`probe`] behind the caller's authorization, so this
/// preflight reveals only that the caller's own key is bound.
///
/// A receipt past the retention window is treated exactly as [`probe`] does
/// (swept or swept-in-place, then the key starts over), so this preflight can
/// never conflict a key the full probe would free.
async fn conflict_only_probe(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ctx: &OpContext<'_>,
    now: DateTime<Utc>,
) -> StoreResult<()> {
    let Some(idempotency_key) = ctx.idempotency_key else {
        return Ok(());
    };
    let sql = format!(
        "SELECT {RECEIPT_COLUMNS} FROM memory_receipts \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?"
    );
    let existing = sqlx::query_as::<_, ReceiptRow>(sqlx::AssertSqlSafe(sql))
        .bind(ctx.space_id)
        .bind(ctx.requester_principal_id)
        .bind(ctx.op)
        .bind(idempotency_key)
        .fetch_optional(&mut **tx)
        .await?;
    let Some(row) = existing else {
        return Ok(());
    };
    if matches!(retention(&row, now)?, Retention::Expired) {
        return Ok(());
    }
    if row.payload_digest != ctx.payload_digest {
        return Err(StoreError::IdempotencyConflict);
    }
    Ok(())
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

/// The subject a bearer kind is already registered to in this space (§7/
/// I-000149), if any. It is the same row the read side's smallest-id rule
/// resolves, so `ensure_subject` can refuse a second naming instead of letting
/// a later ensure re-select which subject owns the bearer's projection.
async fn subject_naming(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    kind_ns: &str,
    kind_id: &str,
) -> StoreResult<Option<SubjectRow>> {
    let sql = format!(
        "SELECT {SUBJECT_COLUMNS} FROM memory_subjects \
         WHERE space_id = ? AND kind_ns = ? AND kind_id = ? ORDER BY subject_id LIMIT 1"
    );
    Ok(sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(kind_ns)
        .bind(kind_id)
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
///
/// A projected row has no provider revision to bump, so it enters the marker as
/// `(id, digest)` under `projected` — [`legacy::record_version`] covers every
/// value the merged read serves. A bearer file rewritten between pages therefore
/// invalidates the cursor even though the projected revision stays 1.
fn snapshot_marker<'a>(
    rows: impl Iterator<Item = (&'a str, i64)>,
    projections: impl Iterator<Item = (&'a str, &'a str)>,
    grants: &[GrantRow],
    subject_status: Option<&str>,
) -> StoreResult<String> {
    let mut visible: Vec<(String, i64)> = rows
        .map(|(record_id, revision)| (record_id.to_string(), revision))
        .collect();
    visible.sort();
    let mut projected: Vec<(String, String)> = projections
        .map(|(record_id, version)| (record_id.to_string(), version.to_string()))
        .collect();
    projected.sort();
    let mut grant_revisions: Vec<(String, i64)> = grants
        .iter()
        .map(|grant| (grant.grant_id.clone(), grant.revision))
        .collect();
    grant_revisions.sort();
    let parts = json!({
        "rows": visible,
        "projected": projected,
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

/// Cut one page out of a merged list that is already ordered the page's way.
fn page_rows<T>(rows: Vec<T>, limit: u64, offset: u64) -> Vec<T> {
    rows.into_iter()
        .skip(usize::try_from(offset).unwrap_or(usize::MAX))
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .collect()
}

/// Every provider record id of the subject, independent of the request
/// predicate: a migrated legacy object carries the bridge's own external id, so
/// the provider row must win even when the predicate would hide it (see the
/// merge in [`MemoryStore::records`]).
async fn provider_record_ids(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<HashSet<String>> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT record_id FROM memory_records WHERE space_id = ? AND subject_id = ?",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(ids.into_iter().collect())
}

/// Every provider candidate id of the subject, independent of the request
/// status filter (see the merge in [`MemoryStore::candidates`]).
async fn provider_candidate_ids(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<HashSet<String>> {
    let ids = sqlx::query_scalar::<_, String>(
        "SELECT candidate_id FROM memory_candidates WHERE space_id = ? AND subject_id = ?",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(ids.into_iter().collect())
}

/// A pending/consumed review candidate as stored (§6.7).
#[derive(Clone, Debug, sqlx::FromRow)]
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
#[derive(Clone, Debug, sqlx::FromRow)]
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
#[derive(Clone, Debug, sqlx::FromRow)]
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

/// What consumes one candidate. A dismissal (§7.14) and a review (§7.15) share
/// the one mutation-time adoption path and the one consumption write (NXM-5
/// D13); they differ only in the records they stage and the outcome they
/// journal.
#[derive(Clone, Copy)]
enum ConsumeStep<'a> {
    Dismiss,
    Review(&'a ReviewDecision<'a>),
}

impl ConsumeStep<'_> {
    /// The candidate-capture capabilities this consumption's coverage admits.
    const fn capabilities(self) -> &'static [&'static str] {
        match self {
            Self::Dismiss => &["review"],
            Self::Review(_) => &["review", "records_write"],
        }
    }

    const fn op(self) -> &'static str {
        match self {
            Self::Dismiss => "memory.candidates.dismiss",
            Self::Review(_) => "memory.candidates.review",
        }
    }

    const fn outcome(self) -> &'static str {
        match self {
            Self::Dismiss => "dismissed",
            Self::Review(decision) => decision.outcome(),
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

/// §6.5 admission of one op: does ONE grant satisfy every requirement?
///
/// The protocol admits an op only if the authenticated principal holds **at
/// least one** grant that includes the op's required capabilities and covers
/// the op's whole data access. Requirements are never unioned across grants —
/// a `review` grant and a separate `records_write` grant do not compose into
/// `memory.candidates.review` (§7.15), and a promote grant that covers only the
/// source placement does not compose with one that covers only the target
/// (§7.21).
fn one_grant_covers(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirements: &[GrantRequirement<'_>],
) -> bool {
    grants.iter().any(|grant| {
        requirements
            .iter()
            .all(|requirement| grant_covers(grant, principal_id, space_id, now, requirement))
    })
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
/// its immutable capture placement, then check that it is still pending and —
/// when the caller named a revision — at that revision. `None` consumes at the
/// revision read here (the dismissal has no caller-supplied CAS).
async fn consumable_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    candidate_id: &str,
    expected_revision: Option<i64>,
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
    if expected_revision.is_some_and(|expected| candidate.revision != expected) {
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

/// §7.15 record inserts for one review decision: the records the decision
/// yields (in insertion order) and their ids. Content and provenance are parsed
/// before the decision is matched, so a corrupt candidate payload refuses the
/// whole decision exactly as it did before the adoption path was shared.
async fn stage_review_records(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    candidate: &CandidateRow,
    decision: &ReviewDecision<'_>,
    now: DateTime<Utc>,
) -> StoreResult<(Vec<FileEffect>, Vec<String>)> {
    let provenance: Value = serde_json::from_str(&candidate.provenance_json)
        .map_err(|e| StoreError::Corrupt(format!("candidate provenance: {e}")))?;
    let content: Value = match candidate.content_json.as_deref() {
        Some(raw) => serde_json::from_str(raw)
            .map_err(|e| StoreError::Corrupt(format!("candidate content: {e}")))?,
        None => {
            return Err(StoreError::Corrupt(format!(
                "pending candidate {} carries no content",
                candidate.candidate_id
            )))
        }
    };
    let placement = candidate.scope();
    let mut effects = Vec::new();
    let mut record_ids = Vec::new();
    match decision {
        ReviewDecision::Promote {
            kind,
            content: override_content,
        } => {
            let record_id = mint_record_id();
            effects.push(
                insert_record(
                    tx,
                    candidate.space_id.as_str(),
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
                        tx,
                        candidate.space_id.as_str(),
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
    Ok((effects, record_ids))
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

/// The adoption insert itself: every projected row `ids` names, under the id
/// the bridge projects it with, inside the caller's transaction.
///
/// A dismissal adopts what it consumes, so "adopt then mutate" is one guarded
/// write and no adopted pending row can be left behind (NXM-5 Task 2 fix round
/// 3, finding F1: there is no standalone adoption). A capture adopts the row it
/// just wrote, which is the one adopter that may run past the completion marker:
/// the marker covers the rows that existed when it was published, not a capture
/// that arrives afterwards (`include_migrated`).
/// Returns the number of rows actually inserted.
#[allow(clippy::too_many_arguments)]
async fn adopt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    principal_id: &str,
    subject_id: &str,
    ids: &[String],
    now: DateTime<Utc>,
    include_migrated: bool,
) -> StoreResult<i64> {
    let mut adopted = 0_i64;
    for id in ids {
        let Some(row) =
            legacy::candidate_by_id_in_tx(tx, roots, space_id, id, include_migrated).await?
        else {
            continue;
        };
        // The bridge resolves the bearer's canonical subject; an id that
        // resolves under another subject is not this caller's to adopt.
        if row.subject_id != subject_id {
            continue;
        }
        let requirements = [row.requirement("capture")];
        let admission = Admission {
            op: super::OP_LEGACY_ADOPT,
            space_id,
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: Some(subject_id),
            placement_gate: Some(row.scope()),
            freeze_gated: true,
        };
        let _ = authorize(tx, &admission, now).await?;
        gate(tx, &admission).await?;
        let inserted = sqlx::query(
            "INSERT INTO memory_candidates \
             (candidate_id, space_id, subject_id, capture_visibility, capture_context_id, kind, \
              content_json, provenance_json, classification_json, status, consumed_by_json, \
              content_digest, occurred_at, expires_at, revision, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?, ?, ?) \
             ON CONFLICT(candidate_id) DO NOTHING",
        )
        .bind(row.candidate_id.as_str())
        .bind(row.space_id.as_str())
        .bind(row.subject_id.as_str())
        .bind(row.capture_visibility.as_str())
        .bind(row.capture_context_id.as_deref())
        .bind(row.kind.as_str())
        .bind(row.content_json.as_deref())
        .bind(row.provenance_json.as_str())
        .bind(row.classification_json.as_str())
        .bind(row.occurred_at.as_deref())
        .bind(row.expires_at.as_deref())
        .bind(row.revision)
        .bind(legacy::provider_timestamp(&row.created_at))
        .execute(&mut **tx)
        .await?;
        adopted += i64::try_from(inserted.rows_affected()).unwrap_or(0);
    }
    Ok(adopted)
}

/// [`adopt_in_tx`] for projected **records**: every row `ids` names is inserted
/// under the id the bridge projects it with, inside the caller's transaction.
///
/// Only the mutation that consumes the projection calls this (the explicit
/// fragment promotion), so "adopt then mutate" is one guarded write and no
/// adopted record can be left behind by a promotion that did not commit. The
/// row the provider already owns is left as it is: the id and the content are
/// the projection's own, so a second adoption is a no-op.
/// Returns the number of rows actually inserted.
async fn adopt_records_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    roots: &LegacyRoots,
    space_id: &str,
    principal_id: &str,
    subject_id: &str,
    ids: &[String],
    now: DateTime<Utc>,
) -> StoreResult<i64> {
    let mut adopted = 0_i64;
    for id in ids {
        let Some(row) = legacy::record_by_id_in_tx(tx, roots, space_id, id).await? else {
            continue;
        };
        // The bridge resolves the bearer's canonical subject; an id that
        // resolves under another subject is not this caller's to adopt.
        if row.subject_id != subject_id {
            continue;
        }
        let requirements = [row.requirement("records_write", false)];
        let admission = Admission {
            op: super::OP_LEGACY_ADOPT,
            space_id,
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: Some(subject_id),
            placement_gate: Some(row.scope()),
            freeze_gated: true,
        };
        let _ = authorize(tx, &admission, now).await?;
        gate(tx, &admission).await?;
        let inserted = sqlx::query(
            "INSERT INTO memory_records \
             (record_id, space_id, subject_id, kind, content_json, scope_visibility, \
              scope_context_id, provenance_json, import_ref_json, occurred_at, tombstone_json, \
              revision, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(record_id) DO NOTHING",
        )
        .bind(row.record_id.as_str())
        .bind(row.space_id.as_str())
        .bind(row.subject_id.as_str())
        .bind(row.kind.as_str())
        .bind(row.content_json.as_str())
        .bind(row.scope_visibility.as_str())
        .bind(row.scope_context_id.as_deref())
        .bind(row.provenance_json.as_str())
        .bind(row.import_ref_json.as_deref())
        .bind(row.occurred_at.as_deref())
        .bind(row.tombstone_json.as_deref())
        .bind(row.revision)
        .bind(row.created_at.as_str())
        .bind(row.updated_at.as_str())
        .execute(&mut **tx)
        .await?;
        adopted += i64::try_from(inserted.rows_affected()).unwrap_or(0);
    }
    Ok(adopted)
}

/// What one legacy-content migration pass did.
pub(super) enum MigrationOutcome {
    /// The pass committed its objects, its journal intent and the manifest.
    Migrated(Box<ReceiptRow>),
    /// The space was already migrated and its manifest still matches the source.
    Verified(Value),
}

/// P5's migration insert: the projected row verbatim, including the revision the
/// bridge preserves (a Character fragment carries its legacy revision + 1) and
/// the legacy timestamps — a migrated row serves exactly what the projection
/// served before the marker existed.
async fn insert_legacy_record(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: &RecordRow,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO memory_records \
         (record_id, space_id, subject_id, kind, content_json, scope_visibility, scope_context_id, \
          provenance_json, import_ref_json, occurred_at, tombstone_json, revision, created_at, \
          updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)",
    )
    .bind(row.record_id.as_str())
    .bind(row.space_id.as_str())
    .bind(row.subject_id.as_str())
    .bind(row.kind.as_str())
    .bind(row.content_json.as_str())
    .bind(row.scope_visibility.as_str())
    .bind(row.scope_context_id.as_deref())
    .bind(row.provenance_json.as_str())
    .bind(row.import_ref_json.as_deref())
    .bind(row.occurred_at.as_deref())
    .bind(row.revision)
    .bind(row.created_at.as_str())
    .bind(row.updated_at.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// [`insert_legacy_record`] for a legacy pending-review candidate. A candidate
/// owns no projection until it is consumed, so the pass journals no file.
async fn insert_legacy_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: &CandidateRow,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO memory_candidates \
         (candidate_id, space_id, subject_id, capture_visibility, capture_context_id, kind, \
          content_json, provenance_json, classification_json, status, consumed_by_json, \
          content_digest, occurred_at, expires_at, revision, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?, ?, ?)",
    )
    .bind(row.candidate_id.as_str())
    .bind(row.space_id.as_str())
    .bind(row.subject_id.as_str())
    .bind(row.capture_visibility.as_str())
    .bind(row.capture_context_id.as_deref())
    .bind(row.kind.as_str())
    .bind(row.content_json.as_deref())
    .bind(row.provenance_json.as_str())
    .bind(row.classification_json.as_str())
    .bind(row.occurred_at.as_deref())
    .bind(row.expires_at.as_deref())
    .bind(row.revision)
    .bind(row.created_at.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// [`insert_legacy_record`] for the one `SOUL.md` profile.
async fn insert_legacy_profile(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: &ProfileRow,
) -> StoreResult<()> {
    sqlx::query(
        "INSERT INTO memory_profiles \
         (space_id, subject_id, profile_kind, document_json, updated_by, revision, created_at, \
          updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(row.space_id.as_str())
    .bind(row.subject_id.as_str())
    .bind(row.profile_kind.as_str())
    .bind(row.document_json.as_str())
    .bind(row.updated_by.as_str())
    .bind(row.revision)
    .bind(row.created_at.as_str())
    .bind(row.updated_at.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Whether a provider row at a projected record's legacy id is that same legacy
/// object: the bridge's own provenance names the same source, for the same
/// subject and creation time. Content, kind, placement, revision and
/// timestamps are what the edit entry points rewrite after they adopt the
/// projection (D13), so a legitimate successor may differ in all of them.
fn same_legacy_record(stored: &RecordRow, projected: &RecordRow) -> bool {
    stored.subject_id == projected.subject_id
        && same_legacy_provenance(&stored.provenance_json, &projected.provenance_json)
        && stored.created_at == projected.created_at
}

/// Whether two bridge provenances still name the same legacy object.
///
/// The bridge's own claim — `captured_by` and `captured_at` (with the empty
/// anchor set) — is the identity. `source_context_ids` is the placement
/// provenance a binding removal releases on the retained legacy row (F2), so a
/// projection that lost exactly that list is still the successor's source;
/// anything else, including a foreign or malformed claim, must compare equal.
fn same_legacy_provenance(stored: &str, projected: &str) -> bool {
    if stored == projected {
        return true;
    }
    let (Ok(stored), Ok(projected)) = (
        serde_json::from_str::<Value>(stored),
        serde_json::from_str::<Value>(projected),
    ) else {
        return false;
    };
    let released = |value: &Value| {
        value
            .get("source_context_ids")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    };
    let names_a_source = |value: &Value| {
        value
            .get("source_context_ids")
            .and_then(Value::as_array)
            .is_some_and(|ids| !ids.is_empty())
    };
    stored["captured_by"] == projected["captured_by"]
        && stored["captured_at"] == projected["captured_at"]
        && stored["anchors"] == projected["anchors"]
        && names_a_source(&stored)
        && released(&projected)
}

/// A provider row that already owns a candidate's migrated id must still be
/// that legacy candidate: a consumed shell keeps the provider's own outcome (it
/// names the body it consumed through `content_digest`), while a pending row
/// must still carry the legacy content and the metadata the manifest records.
fn require_same_candidate(stored: &CandidateRow, projected: &CandidateRow) -> StoreResult<()> {
    if stored.status == "consumed" {
        let consumed = stored.content_digest.as_deref().ok_or_else(|| {
            StoreError::Corrupt(format!(
                "consumed candidate {} carries no content_digest",
                stored.candidate_id
            ))
        })?;
        if consumed == content_digest(projected)? {
            return Ok(());
        }
        return Err(StoreError::IdempotencyConflict);
    }
    if stored.subject_id == projected.subject_id
        && stored.kind == projected.kind
        && stored.content_json == projected.content_json
        && stored.capture_visibility == projected.capture_visibility
        && stored.capture_context_id == projected.capture_context_id
        && stored.provenance_json == projected.provenance_json
        && stored.classification_json == projected.classification_json
        && stored.occurred_at == projected.occurred_at
        && stored.expires_at == projected.expires_at
        && stored.revision == projected.revision
        // Adoption writes the provider's whole-second rendering into the
        // column, while the projection keeps the legacy precision, so the two
        // values compare at the precision the provider actually stores (F1:
        // the same creation instant, not the same text).
        && legacy::provider_timestamp(&stored.created_at)
            == legacy::provider_timestamp(&projected.created_at)
    {
        return Ok(());
    }
    Err(StoreError::IdempotencyConflict)
}

/// Resolve every projected row against the provider's own row before the pass
/// records or writes anything.
///
/// A legacy id the provider already owns was written by a just-in-time
/// adoption (an edit, a dismissal, a promotion) or by an interrupted earlier
/// pass, so the stored row is the object's successor: its content and revision
/// are what provider-only reads serve, and the marker must record those (a
/// pre-migration edit never touches the legacy source). A row that collides
/// with a projected id without carrying that legacy identity is unrelated
/// content, and the pass refuses instead of absorbing it.
///
/// The marker records the **served** values beside the legacy source they came
/// from (`source_digest`), so a re-run detects a source that moved. The source
/// is normalized for a legitimate binding release: an owned row's source value
/// gets back the binding its successor still names, because recording the
/// released projection instead would re-conflict a release the gate proved
/// legitimate (F2) without protecting anything a read serves.
async fn resolve_legacy_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    snapshot: &mut legacy::MigrationSnapshot,
) -> StoreResult<()> {
    let mut source_records = Vec::with_capacity(snapshot.records.len());
    for row in &mut snapshot.records {
        let stored = record_in_tx(tx, space_id, row.record_id.as_str()).await?;
        if let Some(stored) = &stored {
            if !same_legacy_record(stored, row) {
                return Err(StoreError::IdempotencyConflict);
            }
            restore_released_record_binding(stored, row)?;
        }
        source_records.push(row.clone());
        if let Some(stored) = stored {
            *row = stored;
        }
    }
    for row in &mut snapshot.candidates {
        let Some(stored) = candidate_in_tx(tx, space_id, row.candidate_id.as_str()).await? else {
            continue;
        };
        restore_released_binding(&stored, row)?;
        require_same_candidate(&stored, row)?;
    }
    let mut source_profiles = Vec::with_capacity(snapshot.profiles.len());
    for row in &mut snapshot.profiles {
        let stored = profile_in_tx(tx, space_id, &row.subject_id, &row.profile_kind).await?;
        source_profiles.push(row.clone());
        let Some(stored) = stored else {
            continue;
        };
        // The stored profile of the subject's kind *is* that document: a
        // pre-migration `SOUL.md` write creates it without adopting a
        // projection, so its document and revision are the provider's own.
        *row = stored;
    }
    let source =
        legacy::migration_manifest(&source_records, &snapshot.candidates, &source_profiles)?;
    let served =
        legacy::migration_manifest(&snapshot.records, &snapshot.candidates, &snapshot.profiles)?;
    snapshot.manifest = json!({
        "digest": served["digest"],
        "source_digest": source["digest"],
        "counts": served["counts"],
        "families": served["families"],
    });
    Ok(())
}

/// The record half of [`restore_released_binding`]: a binding removal clears the
/// legacy fragment row's binding, so its projection loses the placement and the
/// `actor_world_binding_id` content member the successor's provenance still
/// names. A row that still projects a binding, or whose successor never named
/// one, is left exactly as it projects.
fn restore_released_record_binding(
    stored: &RecordRow,
    projected: &mut RecordRow,
) -> StoreResult<()> {
    if projected.scope_context_id.is_some() {
        return Ok(());
    }
    let provenance: Value = serde_json::from_str(&stored.provenance_json)
        .map_err(|error| StoreError::Corrupt(format!("record provenance: {error}")))?;
    let Some(binding) = provenance["source_context_ids"]
        .as_array()
        .and_then(|ids| ids.first())
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    let mut content: Value = serde_json::from_str(&projected.content_json)
        .map_err(|error| StoreError::Corrupt(format!("record content: {error}")))?;
    let Some(legacy) = content
        .pointer_mut("/structured/nexus_legacy")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    if legacy.get("actor_world_binding_id") != Some(&Value::Null) {
        return Ok(());
    }
    legacy.insert(
        "actor_world_binding_id".to_string(),
        Value::String(binding.to_string()),
    );
    projected.content_json = serialize(&content, "legacy record content")?;
    projected.scope_visibility = Placement::CONTEXT.to_string();
    projected.scope_context_id = Some(binding.to_string());
    projected
        .provenance_json
        .clone_from(&stored.provenance_json);
    Ok(())
}

/// A binding removal clears the legacy row's binding while the provider still
/// owns the row it adopted from it; the successor's own capture context restores
/// what the removal released, so the release is not a change of the migrated
/// object (F2, beside PR-F1's successor identity). A pending candidate — one
/// removal cannot see, because it holds the binding — or a row whose successor
/// never carried a binding is left exactly as it projects.
fn restore_released_binding(
    stored: &CandidateRow,
    projected: &mut CandidateRow,
) -> StoreResult<()> {
    if stored.status != "consumed" || projected.capture_context_id.is_some() {
        return Ok(());
    }
    let Some(binding) = stored.capture_context_id.as_deref() else {
        return Ok(());
    };
    let Some(raw) = projected.content_json.as_deref() else {
        return Ok(());
    };
    let mut content: Value = serde_json::from_str(raw)
        .map_err(|error| StoreError::Corrupt(format!("candidate content: {error}")))?;
    let Some(legacy) = content
        .pointer_mut("/structured/nexus_legacy")
        .and_then(Value::as_object_mut)
    else {
        return Ok(());
    };
    if legacy.get("actor_world_binding_id") != Some(&Value::Null) {
        return Ok(());
    }
    legacy.insert(
        "actor_world_binding_id".to_string(),
        Value::String(binding.to_string()),
    );
    projected
        .capture_visibility
        .clone_from(&stored.capture_visibility);
    projected.capture_context_id = Some(binding.to_string());
    projected
        .provenance_json
        .clone_from(&stored.provenance_json);
    projected.content_json = Some(serialize(&content, "candidate content")?);
    Ok(())
}

/// What one record mutation applies after adoption (§7.18/§7.19). Both shares
/// of the path take the CAS, the subject/placement gate and the receipt.
#[derive(Clone, Copy)]
enum RecordMutation<'a> {
    Update {
        content: Option<&'a Value>,
        kind: Option<&'a str>,
    },
    Delete {
        reason: Option<&'a str>,
        deleted_by: &'a str,
    },
}

impl MemoryStore {
    /// The bearer subject that owns one provider-native candidate row, if this
    /// store holds it.
    ///
    /// A projected legacy id is not a row here (`None`), and neither is an id
    /// this space never held: the caller's own path answers that absence. The
    /// probe decides which per-bearer authority section a candidate mutation
    /// must take (NXM-5 Task 2 fix round 3, finding F1), so it runs before the
    /// mutation and reads only.
    pub(super) async fn candidate_subject(
        &self,
        space_id: &str,
        candidate_id: &str,
    ) -> StoreResult<Option<String>> {
        self.require_provisioned(space_id).await?;
        let (mut tx, _now) = self.begin_read().await?;
        let subject: Option<String> = sqlx::query_scalar(
            "SELECT subject_id FROM memory_candidates \
             WHERE space_id = ? AND candidate_id = ?",
        )
        .bind(space_id)
        .bind(candidate_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.rollback().await?;
        Ok(subject)
    }

    /// The bearer subject that owns one provider-native candidate, if this
    /// principal may review it (NXM-5 Task 2 fix round 5, finding R5-F1).
    ///
    /// Authorization is part of the probe, not a later step: the next thing the
    /// caller does with the answer is name the owner in an error, so a principal
    /// without `review` coverage must never reach it. An uncovered candidate
    /// answers exactly like one this store never held — the same `NotFound` the
    /// effect transaction raises — so the refusal discloses neither existence
    /// nor ownership. This is the disclosure check, not the effect check: the
    /// admission transaction still authorizes the write itself against the row
    /// it consumes, so a grant withdrawn between the two is refused there.
    ///
    /// A projected legacy id is still `None`: the bridge authorizes those inside
    /// the adoption, from the row it projects.
    pub(super) async fn reviewable_candidate_subject(
        &self,
        space_id: &str,
        candidate_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<String>> {
        self.require_provisioned(space_id).await?;
        let (mut tx, now) = self.begin_read().await?;
        let subject: Option<String> = sqlx::query_scalar(
            "SELECT subject_id FROM memory_candidates \
             WHERE space_id = ? AND candidate_id = ?",
        )
        .bind(space_id)
        .bind(candidate_id)
        .fetch_optional(&mut *tx)
        .await?;
        if subject.is_none() {
            tx.rollback().await?;
            return Ok(None);
        }
        let admission = Admission {
            op: super::OP_CANDIDATES_DISMISS,
            space_id,
            principal_id,
            coverage: Coverage::Candidate {
                candidate_id,
                capabilities: &["review"],
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        let admitted = authorize(&mut tx, &admission, now).await;
        tx.rollback().await?;
        match admitted {
            Ok(_) => Ok(subject),
            Err(StoreError::NotAuthorized(_)) => {
                Err(StoreError::NotFound(format!("candidate {candidate_id}")))
            }
            Err(error) => Err(error),
        }
    }

    /// Materialize the named projected legacy candidates into the provider under
    /// their deterministic ids, without consuming them.
    ///
    /// The Character capture writes the retained queue row and adopts it in the
    /// same call: the one-shot migration turns every legacy queue read off, so a
    /// capture that only wrote the legacy row would be invisible to the list,
    /// count, dismissal and review paths afterwards (fix round 1, Greptile
    /// `legacy.rs:318`). Adoption is the projection's own insert, so the row the
    /// pass later compares is byte-identical to the source.
    pub(super) async fn adopt_legacy_candidates(
        &self,
        space_id: &str,
        principal_id: &str,
        subject_id: &str,
        ids: &[String],
    ) -> StoreResult<()> {
        let (mut tx, now) = self.begin_admission().await?;
        adopt_in_tx(
            &mut tx,
            &self.legacy,
            space_id,
            principal_id,
            subject_id,
            ids,
            now,
            true,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// §7.11: admission + candidate inserts + receipt, one transaction.
    ///
    /// `keep_existing` is the caller's identity guarantee: a capture whose
    /// candidate id is deterministic (the Character capture's bridge id) may
    /// find that id already stored — an earlier capture, or a legacy row the
    /// bridge adopted under the same id — and the retained answer for a
    /// re-delivered capture is the same success the legacy queue's
    /// `INSERT OR IGNORE` gave, so the insert leaves the stored row in place.
    /// A minted id keeps the strict insert.
    ///
    /// `binding` is the caller's provenance obligation (compass D15): the
    /// Character binding a binding-local capture stands on is re-read on this
    /// transaction's own connection, under its `BEGIN IMMEDIATE`, before any
    /// insert. `remove_binding` takes the same write lock and counts the
    /// pending candidate this transaction inserts, so it cannot delete the
    /// binding in between: whichever transaction commits first, the other sees
    /// its result and refuses.
    ///
    /// A deterministic id that names an **unmigrated legacy row** with no
    /// provider row is the stock state of a pre-fix interrupted capture
    /// (compass D16). The legacy row is the authoritative content of that
    /// attempt, so the projection is adopted under the same id and the
    /// re-delivery writes nothing of its own — migration's same-candidate
    /// check then sees one version, never two.
    pub(super) async fn capture(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidates: &[NewCandidate<'_>],
        keep_existing: bool,
        binding: Option<&super::CaptureBinding<'_>>,
    ) -> Result<ReceiptRow, CaptureError> {
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            return Ok(row);
        }
        gate(&mut tx, admission).await?;
        if let Some(binding) = binding {
            nexus_local_db::require_valid_provenance_tx(
                &mut tx,
                binding.owner_creator,
                binding.character,
                Some(binding.binding),
            )
            .await
            .map_err(CaptureError::Binding)?;
        }
        #[cfg(feature = "test-hooks")]
        self.capture_gate().await;
        let mut replayed = Vec::with_capacity(candidates.len());
        if keep_existing {
            for candidate in candidates {
                let ids = [candidate.candidate_id.to_string()];
                let adopted = adopt_in_tx(
                    &mut tx,
                    &self.legacy,
                    ctx.space_id,
                    ctx.requester_principal_id,
                    candidate.subject_id,
                    &ids,
                    now,
                    true,
                )
                .await?;
                if adopted > 0 {
                    replayed.push(candidate.candidate_id);
                }
            }
        }
        let at = timestamp(now);
        let mut ids = Vec::with_capacity(candidates.len());
        let conflict = if keep_existing {
            " ON CONFLICT(candidate_id) DO NOTHING"
        } else {
            ""
        };
        let insert = format!(
            "INSERT INTO memory_candidates \
             (candidate_id, space_id, subject_id, capture_visibility, capture_context_id, kind, \
              content_json, provenance_json, classification_json, status, consumed_by_json, \
              content_digest, occurred_at, expires_at, revision, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, NULL, ?, ?, 1, ?){conflict}"
        );
        for candidate in candidates {
            ids.push(candidate.candidate_id.to_string());
            if replayed.contains(&candidate.candidate_id) {
                continue;
            }
            sqlx::query(sqlx::AssertSqlSafe(insert.clone()))
                .bind(candidate.candidate_id)
                .bind(ctx.space_id)
                .bind(candidate.subject_id)
                .bind(candidate.placement.visibility)
                .bind(candidate.placement.context_id)
                .bind(candidate.kind)
                .bind(serialize(candidate.content, "candidate content")?)
                .bind(serialize(candidate.provenance, "candidate provenance")?)
                .bind(serialize(
                    candidate.classification,
                    "candidate classification",
                )?)
                .bind(candidate.occurred_at)
                .bind(candidate.expires_at)
                .bind(&at)
                .execute(&mut *tx)
                .await?;
        }
        let classified = ids.len();
        let result = json!({ "candidate_ids": ids, "classified": classified });
        self.commit_receipt(tx, ctx, &result, now)
            .await
            .map_err(CaptureError::Store)
    }

    /// §7.12: the authorized page and its invalidation marker from one read.
    ///
    /// A candidate's `review` coverage depends on its own capture placement and
    /// on grants only `grant_covers` evaluates, so the page cannot be cut in SQL
    /// before coverage: a predicate-set `LIMIT/OFFSET` would let hidden
    /// candidates consume a page and skew the cursor (MEM-R006). The identity
    /// set — never bodies — is read whole under the predicate and
    /// coverage-filtered, cursor offsets index that authorized set, and only the
    /// page's ids then fetch their bodies, so rows past the page do not cross
    /// the read (S-10).
    #[allow(clippy::too_many_lines)]
    pub(super) async fn candidates(
        &self,
        space_id: &str,
        subject_id: &str,
        status: Option<&str>,
        principal_id: &str,
        page: Option<(u64, u64)>,
    ) -> StoreResult<(Vec<CandidateRow>, String, bool)> {
        self.require_provisioned(space_id).await?;
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
        let subject_row = subject_in_tx(&mut tx, space_id, subject_id).await?;
        let subject_status = subject_row.as_ref().map(|subject| subject.status.clone());
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
        // §7.7: `archived` hides the subject from the *default* listing; a
        // status-filtered listing is an explicit audit query and stays readable.
        // §5.2/D-10: an unsettled intent withholds the subject either way.
        let archived = status.is_none() && subject_status.as_deref() == Some("archived");
        let hidden = withheld(unsettled.as_deref(), subject_id)?;
        if archived || hidden {
            tx.commit().await?;
            let marker = snapshot_marker(
                std::iter::empty(),
                std::iter::empty(),
                &grant_rows,
                subject_status.as_deref(),
            )?;
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
        // Both statements run in the read's snapshot: the identity set (no
        // bodies) is filtered here, because `grant_covers` is the one
        // authorization authority and coverage needs each row's placement;
        // only the page's own ids then fetch their bodies.
        let identity_sql = format!(
            "SELECT candidate_id, subject_id, capture_visibility, capture_context_id, revision, \
             created_at FROM memory_candidates{where_sql} ORDER BY created_at, candidate_id"
        );
        let mut identity_query = sqlx::query_as::<
            _,
            (String, String, String, Option<String>, i64, String),
        >(sqlx::AssertSqlSafe(identity_sql))
        .bind(space_id)
        .bind(subject_id);
        if let Some(status) = status {
            identity_query = identity_query.bind(status);
        }
        let identity = identity_query
            .bind(timestamp(now))
            .fetch_all(&mut *tx)
            .await?;
        let mut authorized: Vec<(String, i64, String)> = identity
            .iter()
            .filter(|row| {
                covered(
                    &grants,
                    principal_id,
                    space_id,
                    now,
                    &candidate_requirement("review", &row.1, &row.2, row.3.as_deref()),
                )
            })
            .map(|row| (row.0.clone(), row.4, row.5.clone()))
            .collect();
        // Legacy read bridge (NXM-5 Task 1): projected queue entries join the
        // same authorized identity set under the same coverage rule. A migrated
        // candidate carries the bridge's own external id, so the provider row
        // wins and the projection is dropped.
        let mut projected: HashMap<String, CandidateRow> = HashMap::new();
        let legacy_rows = legacy::candidates_in_tx(
            &mut tx,
            &self.legacy,
            space_id,
            subject_row.as_ref(),
            subject_id,
            status,
        )
        .await?;
        if !legacy_rows.is_empty() {
            let owned = provider_candidate_ids(&mut tx, space_id, subject_id).await?;
            for row in legacy_rows {
                if owned.contains(&row.candidate_id)
                    || !covered(
                        &grants,
                        principal_id,
                        space_id,
                        now,
                        &candidate_requirement(
                            "review",
                            &row.subject_id,
                            &row.capture_visibility,
                            row.capture_context_id.as_deref(),
                        ),
                    )
                {
                    continue;
                }
                authorized.push((
                    row.candidate_id.clone(),
                    row.revision,
                    row.created_at.clone(),
                ));
                projected.insert(row.candidate_id.clone(), row);
            }
            authorized.sort_by(|left, right| {
                created_at_cmp(&left.2, &right.2).then_with(|| left.0.cmp(&right.0))
            });
        }
        // Cursor offsets index the authorized set, never the predicate set: a
        // hidden candidate can neither consume a page nor skip a visible row
        // across a boundary (MEM-R006).
        let (start, take) = page.map_or((0, authorized.len()), |(limit, offset)| {
            (
                usize::try_from(offset)
                    .unwrap_or(usize::MAX)
                    .min(authorized.len()),
                usize::try_from(limit).unwrap_or(usize::MAX),
            )
        });
        let end = start.saturating_add(take).min(authorized.len());
        let more = end < authorized.len();
        let page_ids: Vec<String> = authorized[start..end]
            .iter()
            .map(|(candidate_id, _, _)| candidate_id.clone())
            .collect();
        let stored_ids: Vec<&str> = page_ids
            .iter()
            .filter(|candidate_id| !projected.contains_key(*candidate_id))
            .map(String::as_str)
            .collect();
        let mut rows = if stored_ids.is_empty() {
            Vec::new()
        } else {
            let placeholders = std::iter::repeat_n("?", stored_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let page_sql = format!(
                "SELECT {CANDIDATE_COLUMNS} FROM memory_candidates \
                 WHERE candidate_id IN ({placeholders}) ORDER BY created_at, candidate_id"
            );
            let mut rows_query = sqlx::query_as::<_, CandidateRow>(sqlx::AssertSqlSafe(page_sql));
            for candidate_id in &stored_ids {
                rows_query = rows_query.bind(*candidate_id);
            }
            rows_query.fetch_all(&mut *tx).await?
        };
        rows.extend(
            page_ids
                .iter()
                .filter_map(|candidate_id| projected.remove(candidate_id)),
        );
        rows.sort_by(|left, right| {
            created_at_cmp(&left.created_at, &right.created_at)
                .then_with(|| left.candidate_id.cmp(&right.candidate_id))
        });
        tx.commit().await?;
        let marker = snapshot_marker(
            authorized
                .iter()
                .map(|(candidate_id, revision, _)| (candidate_id.as_str(), *revision)),
            // A pending-review row is insert-once, so a projected candidate keeps
            // one fixed identity for as long as it is pending; only records can
            // change content while keeping their id.
            std::iter::empty(),
            &grant_rows,
            subject_status.as_deref(),
        )?;
        Ok((rows, marker, more))
    }

    /// §7.13: by-id read under current authorization (§7.17-style; an
    /// unauthorized candidate is reported as absent).
    pub(super) async fn candidate_for_read(
        &self,
        space_id: &str,
        candidate_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<CandidateRow>> {
        self.require_provisioned(space_id).await?;
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = candidate_in_tx(&mut tx, space_id, candidate_id).await?;
        // Legacy read bridge (NXM-5 Task 1): a projected id resolves to its
        // unmigrated queue entry; coverage below applies to the projection.
        let row = match row {
            Some(row) => Some(row),
            None => {
                legacy::candidate_by_id_in_tx(&mut tx, &self.legacy, space_id, candidate_id, false)
                    .await?
            }
        };
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
        self.consume_adopting(
            ctx,
            admission,
            candidate_id,
            None,
            Some(expected_revision),
            ConsumeStep::Dismiss,
        )
        .await
    }

    /// NXM-5 Task 2: dismiss a legacy-derived candidate, adopting the projection
    /// under its deterministic external id **inside this dismissal's guarded
    /// transaction**.
    ///
    /// Adoption and consumption commit together or not at all: there is no
    /// window in which an adopted pending row exists without the dismissal that
    /// asked for it, and no window in which a legacy review drain can see a row
    /// the provider has taken ownership of but not yet consumed (NXM-5 Task 2
    /// fix round 1, finding F4). `expected_revision` is `None` because the E4
    /// dismissal has no caller-supplied CAS: the revision read inside the
    /// transaction is the one consumed.
    pub(super) async fn dismiss_legacy_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        subject_id: &str,
        expected_revision: Option<i64>,
    ) -> StoreResult<ReceiptRow> {
        self.consume_adopting(
            ctx,
            admission,
            candidate_id,
            Some(subject_id),
            expected_revision,
            ConsumeStep::Dismiss,
        )
        .await
    }

    #[allow(clippy::too_many_lines)]
    async fn consume_adopting(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        adopt_subject: Option<&str>,
        expected_revision: Option<i64>,
        step: ConsumeStep<'_>,
    ) -> StoreResult<ReceiptRow> {
        // Resolve what this consumption has to be authorized as BEFORE the
        // publication section (which evaluates the caller's admission): a row the
        // provider does not own yet is authorized exactly as the bridge projects
        // it, because the adopted row carries the same subject and immutable
        // capture placement.
        let projected = match adopt_subject {
            Some(_) => {
                let (mut read, _) = self.begin_read().await?;
                let owned = candidate_in_tx(&mut read, ctx.space_id, candidate_id).await?;
                let row = match owned {
                    Some(_) => None,
                    None => {
                        legacy::candidate_by_id_in_tx(
                            &mut read,
                            &self.legacy,
                            ctx.space_id,
                            candidate_id,
                            false,
                        )
                        .await?
                    }
                };
                read.rollback().await?;
                row
            }
            None => None,
        };
        let requirements: Vec<GrantRequirement<'_>> = projected
            .iter()
            .flat_map(|row| {
                step.capabilities()
                    .iter()
                    .map(move |capability| row.requirement(capability))
            })
            .collect();
        let projected_admission = Admission {
            op: super::OP_LEGACY_ADOPT,
            space_id: ctx.space_id,
            principal_id: admission.principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: projected.as_ref().map(|row| row.subject_id.as_str()),
            placement_gate: projected.as_ref().map(CandidateRow::scope),
            freeze_gated: true,
        };
        let effective = if projected.is_some() {
            &projected_admission
        } else {
            admission
        };
        let targets = [journal::candidate_target(candidate_id)];
        let mut publication = self.begin_publication(ctx, effective, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, effective, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        if let Some(subject_id) = adopt_subject {
            adopt_in_tx(
                &mut tx,
                &self.legacy,
                ctx.space_id,
                admission.principal_id,
                subject_id,
                &[candidate_id.to_string()],
                now,
                false,
            )
            .await?;
            // The projection above was read before this transaction. Re-authorize
            // against the adopted row inside it, so the dismissal is admitted on
            // exactly the placement it consumes.
            let _ = authorize(&mut tx, admission, now).await?;
        }
        gate(&mut tx, effective).await?;
        let candidate =
            consumable_candidate(&mut tx, ctx.space_id, candidate_id, expected_revision).await?;
        // A dismissal that names a bearer consumes only that bearer's row (fix
        // round 4, finding R4-F1): the entry point already resolved the owner
        // before taking its section, and this guard keeps the match where the
        // consumption happens, so no future caller can bypass it.
        if let Some(subject_id) = adopt_subject {
            if candidate.subject_id != subject_id {
                return Err(StoreError::NotAuthorized(format!(
                    "candidate {candidate_id} belongs to subject {}, not {subject_id}",
                    candidate.subject_id
                )));
            }
        }
        let receipt_id = mint_receipt_id();
        let mut effects = Vec::new();
        let record_ids = match step {
            ConsumeStep::Dismiss => Vec::new(),
            ConsumeStep::Review(decision) => {
                let (records, record_ids) =
                    stage_review_records(&mut tx, &candidate, decision, now).await?;
                effects.extend(records);
                record_ids
            }
        };
        let consumed_by = json!({
            "op": step.op(),
            "receipt_id": receipt_id,
            "outcome": step.outcome(),
            "record_ids": record_ids,
        });
        let digest = content_digest(&candidate)?;
        let effect = consume_candidate(
            &mut tx,
            &candidate,
            expected_revision.unwrap_or(candidate.revision),
            &consumed_by,
            &digest,
        )
        .await?;
        effects.push(effect);
        let result = match step {
            ConsumeStep::Dismiss => {
                json!({ "candidate_id": candidate_id, "outcome": "dismissed" })
            }
            ConsumeStep::Review(_) => {
                json!({ "outcome": step.outcome(), "record_ids": record_ids })
            }
        };
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
        self.consume_adopting(
            ctx,
            admission,
            candidate_id,
            None,
            Some(expected_revision),
            ConsumeStep::Review(decision),
        )
        .await
    }

    /// NXM-8: review a legacy-derived candidate, adopting the projection under
    /// its deterministic external id **inside this review's guarded
    /// transaction** — the review half of the one mutation-time adoption path
    /// (NXM-5 D13). A row the provider already owns is consumed as it is.
    pub(super) async fn review_legacy_candidate(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        candidate_id: &str,
        subject_id: &str,
        expected_revision: i64,
        decision: &ReviewDecision<'_>,
    ) -> StoreResult<ReceiptRow> {
        self.consume_adopting(
            ctx,
            admission,
            candidate_id,
            Some(subject_id),
            Some(expected_revision),
            ConsumeStep::Review(decision),
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
        project_legacy: bool,
    ) -> StoreResult<(Vec<RecordRow>, String, u64)> {
        self.require_provisioned(space_id).await?;
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
        let subject_row = subject_in_tx(&mut tx, space_id, subject_id).await?;
        let subject_status = subject_row.as_ref().map(|subject| subject.status.clone());
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
        // §7.7: an archived subject's records leave recall; the listing and
        // by-id audit routes still serve them.
        // §5.2/D-10: an unsettled file intent withholds the subject from both.
        let archived = read.excludes_archived() && subject_status.as_deref() == Some("archived");
        let hidden = withheld(unsettled.as_deref(), subject_id)?;
        let subject_all = matches!(selection, Selection::SubjectAll);
        if archived || hidden {
            tx.commit().await?;
            let marker = snapshot_marker(
                std::iter::empty(),
                std::iter::empty(),
                &grant_rows,
                subject_status.as_deref(),
            )?;
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
        // Legacy read bridge (NXM-5 Task 1): the subject's unmigrated content is
        // projected through the same predicate, so the page below is a slice of
        // one authorized set spanning both sources. A provider-only read skips
        // the projection entirely: resolving a provider-native slug must not
        // parse a bearer file (fix round 5, B2).
        let legacy_rows = if project_legacy {
            legacy::records_in_tx(
                &mut tx,
                &self.legacy,
                space_id,
                subject_row.as_ref(),
                subject_id,
                selection,
                filter,
                kinds,
            )
            .await?
        } else {
            Vec::new()
        };
        let projected = !legacy_rows.is_empty();
        // A projected row inserts itself into the order, so the SQL half returns
        // a prefix that cannot omit a provider row of the merged page: a provider
        // row inside the merged prefix `[0, offset + limit)` ranks below
        // `offset + limit` in the table as well.
        let fetch = if projected {
            page.map(|(limit, offset)| (limit.saturating_add(offset), 0))
        } else {
            page
        };
        // `Some((limit, offset))` cuts the page in SQL; `None` is recall, which
        // ranks the whole authorized set (§9.2) and so cannot page before it.
        let page_clause = if fetch.is_some() {
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
        if let Some((limit, offset)) = fetch {
            rows_query = rows_query
                .bind(i64::try_from(limit).unwrap_or(i64::MAX))
                .bind(i64::try_from(offset).unwrap_or(i64::MAX));
        }
        let mut rows = rows_query.fetch_all(&mut *tx).await?;
        // The authorized identity set: the marker and the count, never the
        // bodies of rows the page will not serve. A whole-set read already has
        // them.
        let mut visible: Vec<(String, i64)> = if page.is_some() {
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
        let mut projected_versions: Vec<(String, String)> = Vec::new();
        if projected {
            // A migrated object carries this projection's own external id, so
            // the provider row wins whatever the request predicate says about it
            // — a tombstoned migrated record must stay deleted, not reappear
            // through the bridge.
            let owned = provider_record_ids(&mut tx, space_id, subject_id).await?;
            let legacy_rows: Vec<RecordRow> = legacy_rows
                .into_iter()
                .filter(|row| !owned.contains(&row.record_id))
                .collect();
            visible.extend(
                legacy_rows
                    .iter()
                    .map(|row| (row.record_id.clone(), row.revision)),
            );
            for row in &legacy_rows {
                projected_versions.push((row.record_id.clone(), legacy::record_version(row)?));
            }
            rows.extend(legacy_rows);
            rows.sort_by(|left, right| {
                created_at_cmp(&right.created_at, &left.created_at)
                    .then_with(|| left.record_id.cmp(&right.record_id))
            });
            if let Some((limit, offset)) = page {
                rows = page_rows(rows, limit, offset);
            }
        }
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
            projected_versions
                .iter()
                .map(|(record_id, version)| (record_id.as_str(), version.as_str())),
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
        self.require_provisioned(space_id).await?;
        let (mut tx, now) = self.begin_read().await?;
        space_in_tx(&mut tx, space_id).await?;
        let grants = wire_grants(&grants_in_tx(&mut tx, space_id).await?)?;
        let row = record_in_tx(&mut tx, space_id, record_id).await?;
        // Legacy read bridge (NXM-5 Task 1): a projected id resolves to its
        // unmigrated source; the coverage/tombstone/unsettled gates below apply
        // to the projected row exactly as they do to a provider row.
        let row = match row {
            Some(row) => Some(row),
            None => legacy::record_by_id_in_tx(&mut tx, &self.legacy, space_id, record_id).await?,
        };
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
        self.mutate_record(
            ctx,
            admission,
            record_id,
            expected_revision,
            RecordMutation::Update { content, kind },
            None,
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
        self.mutate_record(
            ctx,
            admission,
            record_id,
            expected_revision,
            RecordMutation::Delete { reason, deleted_by },
            None,
        )
        .await
    }

    /// §7.18/§7.19 mutation-time adoption for one record addressed by the id the
    /// E4 DTO serves (NXM-6 fix round 1, finding F2).
    ///
    /// A record the bridge still projects has no provider row, so the CAS would
    /// answer `NOT_FOUND` for a memory the user can read, edit and delete. The
    /// projection is adopted under its deterministic external id **inside this
    /// mutation's guarded transaction** (NXM-5 D13's mutation-time adoption,
    /// reused for records) and the mutation is re-authorized against the adopted
    /// row; the legacy surface is never written (§D6). A record the provider
    /// already owns is mutated as it is.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn update_legacy_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        adopt_subject: &str,
        record_id: &str,
        expected_revision: i64,
        content: Option<&Value>,
        kind: Option<&str>,
    ) -> StoreResult<ReceiptRow> {
        self.mutate_record(
            ctx,
            admission,
            record_id,
            expected_revision,
            RecordMutation::Update { content, kind },
            Some(adopt_subject),
        )
        .await
    }

    /// [`Self::update_legacy_record`] for a tombstone (§7.19/MEM-R014).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn delete_legacy_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        adopt_subject: &str,
        record_id: &str,
        expected_revision: i64,
        reason: Option<&str>,
        deleted_by: &str,
    ) -> StoreResult<ReceiptRow> {
        self.mutate_record(
            ctx,
            admission,
            record_id,
            expected_revision,
            RecordMutation::Delete { reason, deleted_by },
            Some(adopt_subject),
        )
        .await
    }

    /// §7.18/§7.19: adoption + edit/tombstone + receipt, one transaction.
    ///
    /// `adopt_subject` names the bearer whose adapter issued the mutation when
    /// the record may still be a bridge projection; see
    /// [`Self::update_legacy_record`]. Everything else is one path: the wire ops
    /// pass `None` and mutate a record the provider already owns.
    #[allow(clippy::too_many_lines)] // resolution, adoption, admission and the CAS are one write
    async fn mutate_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        record_id: &str,
        expected_revision: i64,
        step: RecordMutation<'_>,
        adopt_subject: Option<&str>,
    ) -> StoreResult<ReceiptRow> {
        // Resolve what this mutation has to be authorized as BEFORE the
        // publication section (which evaluates the caller's admission): a row
        // the provider does not own yet is authorized exactly as the bridge
        // projects it, because the adopted row carries its subject and its
        // placement verbatim.
        let projected = match adopt_subject {
            Some(_) => {
                let (mut read, _) = self.begin_read().await?;
                let owned = record_in_tx(&mut read, ctx.space_id, record_id).await?;
                let row = match owned {
                    Some(_) => None,
                    None => {
                        legacy::record_by_id_in_tx(&mut read, &self.legacy, ctx.space_id, record_id)
                            .await?
                    }
                };
                read.rollback().await?;
                row
            }
            None => None,
        };
        let capabilities: &[&str] = match &admission.coverage {
            Coverage::Record { capabilities, .. } => capabilities,
            _ => &[],
        };
        let requirements: Vec<GrantRequirement<'_>> = projected
            .iter()
            .flat_map(|row| {
                capabilities
                    .iter()
                    .map(move |capability| row.requirement(capability, false))
            })
            .collect();
        let projected_admission = Admission {
            op: super::OP_LEGACY_ADOPT,
            space_id: ctx.space_id,
            principal_id: admission.principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: projected.as_ref().map(|row| row.subject_id.as_str()),
            placement_gate: projected.as_ref().map(RecordRow::scope),
            freeze_gated: true,
        };
        let effective = if projected.is_some() {
            &projected_admission
        } else {
            admission
        };
        let targets = [journal::record_target(record_id)];
        let mut publication = self.begin_publication(ctx, effective, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, effective, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        if let Some(subject_id) = adopt_subject {
            adopt_records_in_tx(
                &mut tx,
                &self.legacy,
                ctx.space_id,
                admission.principal_id,
                subject_id,
                &[record_id.to_string()],
                now,
            )
            .await?;
            // Re-authorize against the adopted row inside this transaction, so
            // the mutation is admitted on exactly the row it writes.
            let _ = authorize(&mut tx, admission, now).await?;
        }
        gate(&mut tx, effective).await?;
        let record = record_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
        // R4-F2's shape: the adapter speaks for one bearer, so a record owned by
        // another subject is refused where the mutation commits.
        if let Some(subject_id) = adopt_subject {
            if record.subject_id != subject_id {
                return Err(StoreError::NotAuthorized(format!(
                    "record {record_id} belongs to subject {}, not {subject_id}",
                    record.subject_id
                )));
            }
        }
        // §7.7/MEM-R014: deletion scrubs content irreversibly, so the subject
        // and placement gates apply exactly as they do to update and promote.
        require_subject_writable(
            &mut tx,
            ctx.space_id,
            record.subject_id.as_str(),
            &[record.scope()],
        )
        .await?;
        // MEM-R014: deletion is terminal, so a tombstoned record is never edited
        // back to life nor deleted twice.
        if record.tombstoned() {
            return Err(StoreError::StatusTransition(match step {
                RecordMutation::Update { .. } => format!("record {record_id} is tombstoned"),
                RecordMutation::Delete { .. } => {
                    format!("record {record_id} is already tombstoned")
                }
            }));
        }
        if record.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
        }
        // §7.0.4/MEM-R007: an update that carries no content and names only the
        // kind the row already has changes nothing observable, so the row keeps
        // its revision, kind, content and timestamps. The operation still
        // commits its content-free receipt — and therefore its idempotency
        // binding — through the same no-op publication the promotion path uses,
        // and journals no record effect because no record byte changes.
        if let RecordMutation::Update {
            content: None,
            kind: Some(kind),
        } = &step
        {
            if *kind == record.kind.as_str() {
                let result = json!({ "record_id": record_id, "revision": record.revision });
                return self.commit_receipt(tx, ctx, &result, now).await;
            }
        }
        // The content-bearing projection this mutation replaces: the provider's
        // own previous version of the row, never whatever bytes sit at the
        // target (a deletion journals only its digest).
        let prior = journal::record_effect(&record)?.digest;
        let (result, updated) = match step {
            RecordMutation::Update { content, kind } => {
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
                (
                    json!({ "record_id": record_id, "revision": record.revision + 1 }),
                    updated,
                )
            }
            RecordMutation::Delete { reason, deleted_by } => {
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
                (
                    json!({ "record_id": record_id, "tombstoned": true }),
                    updated,
                )
            }
        };
        if updated.rows_affected() != 1 {
            return Err(StoreError::RevisionConflict {
                current: record.revision,
            });
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

    /// §7.21 explicit placement change; provenance is untouched.
    ///
    /// `adopt_subject` names the bearer whose activity lease the caller holds
    /// when the record may still be a bridge projection (the explicit Character
    /// fragment promotion, R4-F1). The projection is then adopted under its
    /// deterministic external id **inside this promotion's guarded transaction**
    /// — NXM-5 D13's mutation-time adoption, reused for records — and the
    /// promotion is re-authorized against the adopted row. Everything else is
    /// one path: the wire op passes `None` and promotes a record the provider
    /// already owns.
    #[allow(clippy::too_many_lines)] // resolution, adoption, admission and the CAS are one write
    pub(super) async fn promote_record(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
        record_id: &str,
        expected_revision: i64,
        target: Placement<'_>,
        adopt_subject: Option<&str>,
    ) -> StoreResult<ReceiptRow> {
        // Resolve what this promotion has to be authorized as BEFORE the
        // publication section (which evaluates the caller's admission): a row
        // the provider does not own yet is authorized exactly as the bridge
        // projects it, because the adopted row carries its subject and its
        // placement verbatim.
        let projected = match adopt_subject {
            Some(_) => {
                let (mut read, _) = self.begin_read().await?;
                let owned = record_in_tx(&mut read, ctx.space_id, record_id).await?;
                let row = match owned {
                    Some(_) => None,
                    None => {
                        legacy::record_by_id_in_tx(&mut read, &self.legacy, ctx.space_id, record_id)
                            .await?
                    }
                };
                read.rollback().await?;
                row
            }
            None => None,
        };
        let capabilities: &[&str] = match &admission.coverage {
            Coverage::Record { capabilities, .. } => capabilities,
            _ => &[],
        };
        let requirements: Vec<GrantRequirement<'_>> = projected
            .iter()
            .flat_map(|row| {
                capabilities
                    .iter()
                    .map(move |capability| row.requirement(capability, false))
            })
            .collect();
        let projected_admission = Admission {
            op: super::OP_LEGACY_ADOPT,
            space_id: ctx.space_id,
            principal_id: admission.principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: projected.as_ref().map(|row| row.subject_id.as_str()),
            placement_gate: projected.as_ref().map(RecordRow::scope),
            freeze_gated: true,
        };
        let effective = if projected.is_some() {
            &projected_admission
        } else {
            admission
        };
        let targets = [journal::record_target(record_id)];
        let mut publication = self.begin_publication(ctx, effective, &targets).await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, effective, now).await?;
        if let Some(row) = probe(&mut tx, ctx, now).await? {
            // §5.2 step 2/S-2: settle the replayed intent before serving it, so
            // a duplicate key reaches the published outcome (or the recorded
            // terminal failure) instead of `OUTCOME_UNCERTAIN`.
            tx.rollback().await?;
            return self.settle_replayed(&mut publication, row).await;
        }
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        if let Some(subject_id) = adopt_subject {
            adopt_records_in_tx(
                &mut tx,
                &self.legacy,
                ctx.space_id,
                admission.principal_id,
                subject_id,
                &[record_id.to_string()],
                now,
            )
            .await?;
            // Re-authorize against the adopted row inside this transaction, so
            // the promotion is admitted on exactly the row it mutates.
            let _ = authorize(&mut tx, admission, now).await?;
        }
        gate(&mut tx, effective).await?;
        let record = record_in_tx(&mut tx, ctx.space_id, record_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("record {record_id}")))?;
        // R4-F2: this seam promotes one bearer's own fragment, so the record the
        // lease names must be that bearer's and must be a `fact` fragment. The
        // check sits where the mutation commits, not at the caller's read, so no
        // caller can reach another Character's record through this path.
        if let Some(subject_id) = adopt_subject {
            if record.subject_id != subject_id {
                return Err(StoreError::NotAuthorized(format!(
                    "record {record_id} belongs to subject {}, not {subject_id}",
                    record.subject_id
                )));
            }
            if record.kind != legacy::FACT_KIND {
                return Err(StoreError::NotAuthorized(format!(
                    "record {record_id} is not a Character fragment"
                )));
            }
        }
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
        // R4-F3 round 2: the placement this promotion leaves is in neither the
        // row (its scope is the target now) nor its provenance (a client claim
        // that need not name the placement at all), so a post-commit cache
        // invalidation could not recover it. The promotion's own transaction
        // records it, and the invalidating caller clears it once the cache rows
        // are gone: a crash in between leaves one row a retry consumes.
        if adopt_subject.is_some() && revision != record.revision {
            if let Some(prior_context) = record.scope_context_id.as_deref() {
                record_soul_invalidation(&mut tx, ctx.space_id, record_id, prior_context, now)
                    .await?;
            }
        }
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

    /// P5's one-shot legacy-content migration (compass D7).
    ///
    /// Reads every legacy source of the space, writes the provider objects the
    /// bridge projects under their deterministic external ids, journals the
    /// record/profile projections, and records the completion manifest — all in
    /// **one** guarded transaction, because the manifest is the marker every
    /// projection entry point reads: the pass either migrates a space whole or
    /// leaves it exactly as it was.
    ///
    /// The single intent names every subject its file effects belong to
    /// (`fenced_subjects` in its recorded outcome), so an interrupted publication
    /// withholds **every** affected bearer, not only the first one (P5-F1).
    ///
    /// A provider row that already carries a migrated object's id (a
    /// just-in-time adoption, or an interrupted earlier pass) is never
    /// overwritten: a row that still carries the legacy object's identity is
    /// that object's successor — the entry points adopt and then edit it — so
    /// the manifest records the values provider-only reads serve, and a row
    /// that collides with the id without that identity is unrelated content and
    /// refuses `IDEMPOTENCY_CONFLICT`. A completed pass verifies its manifest
    /// instead of writing, but only after it has settled any intent the
    /// marker's pass left unsettled: a marker whose projection is still owed is
    /// not "complete" (P5-F2).
    pub(super) async fn migrate_legacy(
        &self,
        ctx: &OpContext<'_>,
        admission: &Admission<'_>,
    ) -> StoreResult<MigrationOutcome> {
        let mut publication = self.begin_publication_section().await?;
        let (mut tx, now) = self.begin_admission().await?;
        let _ = authorize(&mut tx, admission, now).await?;
        if let Some(recorded) = legacy::migration_marker(&mut tx, ctx.space_id).await? {
            let mut snapshot =
                legacy::migration_snapshot(&mut tx, &self.legacy, ctx.space_id).await?;
            resolve_legacy_snapshot(&mut tx, ctx.space_id, &mut snapshot).await?;
            tx.rollback().await?;
            // A committed marker can still owe its file publication: re-drive
            // whatever this op left unsettled before calling the space complete.
            self.settle_own_intents(&mut publication, ctx).await?;
            let recorded: Value = serde_json::from_str(&recorded)
                .map_err(|e| StoreError::Corrupt(format!("legacy migration marker: {e}")))?;
            if recorded != snapshot.manifest {
                return Err(StoreError::IdempotencyConflict);
            }
            return Ok(MigrationOutcome::Verified(snapshot.manifest));
        }
        gate(&mut tx, admission).await?;
        let mut snapshot = legacy::migration_snapshot(&mut tx, &self.legacy, ctx.space_id).await?;
        resolve_legacy_snapshot(&mut tx, ctx.space_id, &mut snapshot).await?;
        let mut effects = Vec::new();
        for subject in &snapshot.subjects {
            sqlx::query(
                "INSERT INTO memory_subjects \
                 (space_id, subject_id, kind_ns, kind_id, status, fenced_contexts_json, revision, \
                  created_at) \
                 VALUES (?, ?, ?, ?, 'active', '[]', 1, ?) \
                 ON CONFLICT (space_id, subject_id) DO NOTHING",
            )
            .bind(ctx.space_id)
            .bind(subject.subject_id.as_str())
            .bind(subject.kind_ns)
            .bind(subject.kind_id.as_str())
            .bind(timestamp(now))
            .execute(&mut *tx)
            .await?;
        }
        for row in &snapshot.records {
            if record_in_tx(&mut tx, ctx.space_id, &row.record_id)
                .await?
                .is_some()
            {
                continue;
            }
            insert_legacy_record(&mut tx, row).await?;
            effects.push(journal::record_effect(row)?);
        }
        for row in &snapshot.candidates {
            if candidate_in_tx(&mut tx, ctx.space_id, &row.candidate_id)
                .await?
                .is_some()
            {
                continue;
            }
            insert_legacy_candidate(&mut tx, row).await?;
        }
        for row in &snapshot.profiles {
            if profile_in_tx(&mut tx, ctx.space_id, &row.subject_id, &row.profile_kind)
                .await?
                .is_some()
            {
                continue;
            }
            insert_legacy_profile(&mut tx, row).await?;
            effects.push(journal::profile_effect(row)?);
        }
        let manifest = serde_json::to_string(&snapshot.manifest)
            .map_err(|e| StoreError::Corrupt(format!("legacy migration manifest: {e}")))?;
        sqlx::query("INSERT INTO memory_provider_meta (key, value, updated_at) VALUES (?, ?, ?)")
            .bind(legacy::legacy_migration_key(ctx.space_id))
            .bind(&manifest)
            .bind(timestamp(now))
            .execute(&mut *tx)
            .await?;
        let targets: Vec<String> = effects.iter().map(|effect| effect.path.clone()).collect();
        require_settled_targets(&mut tx, ctx.space_id, &targets).await?;
        let fences = legacy::migration_fences(&snapshot);
        let result = json!({
            "status": "migrated",
            "manifest": snapshot.manifest,
            "fenced_subjects": fences,
        });
        let subject_id = legacy::migration_journal_subject(&snapshot).to_string();
        self.commit_journalled(
            tx,
            &mut publication,
            ctx,
            mint_receipt_id(),
            &subject_id,
            &effects,
            &result,
            now,
        )
        .await
        .map(|receipt| MigrationOutcome::Migrated(Box::new(receipt)))
    }

    /// §7.23 profile read: every present profile, or the requested kinds that
    /// are present. Absence is honest-empty, never an error.
    pub(super) async fn profiles(
        &self,
        space_id: &str,
        subject_id: &str,
        principal_id: &str,
    ) -> StoreResult<Option<Vec<ProfileRow>>> {
        self.require_provisioned(space_id).await?;
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
        let subject_row = subject_in_tx(&mut tx, space_id, subject_id).await?;
        let unsettled = unsettled_in(&mut tx, space_id, subject_id).await?;
        let sql = format!(
            "SELECT {PROFILE_COLUMNS} FROM memory_profiles \
             WHERE space_id = ? AND subject_id = ? ORDER BY profile_kind"
        );
        let mut rows = sqlx::query_as::<_, ProfileRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .fetch_all(&mut *tx)
            .await?;
        // Legacy read bridge (NXM-5 Task 1): `SOUL.md` is the subject's authored
        // personality document until it is migrated. A stored profile of the
        // same kind always wins, so a migrated document never appears twice.
        let projected = legacy::profiles_in_tx(
            &mut tx,
            &self.legacy,
            space_id,
            subject_row.as_ref(),
            subject_id,
        )
        .await?;
        for row in projected {
            if rows
                .iter()
                .any(|stored| stored.profile_kind == row.profile_kind)
            {
                continue;
            }
            rows.push(row);
        }
        rows.sort_by(|left, right| left.profile_kind.cmp(&right.profile_kind));
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
pub(super) fn content_digest(candidate: &CandidateRow) -> StoreResult<String> {
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
