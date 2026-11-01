//! Durable intent journal and staged-file publish for the provider's
//! cross-store writes (spoke-memory adoption plan §5.2, D-10, C-113).
//!
//! SQLite cannot make a markdown write atomic with a row write, so the
//! provider never claims file+SQL atomicity. Instead:
//!
//! 1. **Intent first.** Every mutating content operation that produces a file
//!    effect commits a [`FileEffect`] list — target, exact bytes, content
//!    digest and the digest of the projection it replaces — into
//!    `memory_journal` in the SAME guarded transaction as its rows and
//!    receipt. That commit is the point of no return.
//! 2. **Staged publish.** The bytes land in a hidden staged file
//!    (`.<name>.<receipt_id>.staged`, dot-prefixed so no reader lists it),
//!    are verified against the intent digest, and are installed with a
//!    no-clobber link. Only then is the entry marked `applied`, its content
//!    scrubbed and its receipt terminalized.
//! 3. **Ownership-safe replacement.** The target is never checked and then
//!    removed: it is first given a second name by a no-clobber hard link to a
//!    hidden scratch sibling (`.<name>.<receipt_id>.prior`) and only then
//!    unlinked. The link replaces nothing — when the name was taken since the
//!    ownership read, the CURRENT bytes are re-checked: this intent's own
//!    obsolete leftover is dropped and the link retried once, anything else is
//!    preserved and the attempt refuses with the target still in place. Only
//!    the bytes the link carried are verified — absent, already the intent's
//!    own content, or exactly the recorded predecessor — and the predecessor
//!    is dropped only after the new bytes are linked in no-clobber. A cleanup
//!    that fails leaves the intent unsettled instead of marking it applied with
//!    a superseded or deleted body still on disk. Bytes this intent did not
//!    produce are put back where they were, never deleted — for every pathname
//!    state the protocol actually reads. A write landing after such a read and
//!    before the unlink that follows the same name (inside the displacement, or
//!    on a scratch about to be dropped) can still go with it; see § *Who
//!    serializes the file effects* for what that bound covers. Bytes that
//!    already *are* the intent's content are linked back at the target, never
//!    dropped: the intent may only report success with its projection on disk.
//!    A scratch an earlier attempt left behind is checked before the name is
//!    reused — this intent's own or its recorded predecessor is obsolete and
//!    dropped, anything else is preserved and the attempt refuses rather than
//!    replacing it.
//! 4. **Bounded recovery.** A reopen settles every unsettled entry of its
//!    space ([`RECOVERY_BOUND`] entries per pass, rotating across passes so a
//!    persistently conflicting head cannot starve the tail): a staged file
//!    matching the intent digest is published, a scratch left by an interrupted
//!    displacement holding that content completes the intent, a
//!    missing/mismatched staged file is regenerated from the journaled bytes,
//!    and an unsafe path, an I/O failure or a target holding content this
//!    intent never produced quarantines it. An obsolete scratch (the intent's
//!    own content or the recorded predecessor) is removed before the entry
//!    settles, so a superseded or deleted body is never left on disk in a
//!    hidden file; a cleanup that fails keeps the entry unsettled until a later
//!    pass actually removes it. Objects of a subject with an unsettled entry
//!    stay out of recall/list/get until it is resolved; a quarantined entry —
//!    and one a bounded pass left behind, which is marked `deferred` — answers
//!    the terminal `INTERNAL_ERROR` on that subject instead of serving
//!    half-committed content.
//!
//! # Who serializes the file effects, and in what order
//!
//! Every step above is serialized across **processes** by [`PublishFlock`]: one
//! exclusive `flock` on `<file root>/.memory-publish.lock`, held by the whole
//! predecessor-settlement → stage → publish → settle section. Without it the
//! displacement is only ordered within one process: a second provider process
//! could replace the target between the no-clobber link and the unlink that
//! follows it, and the unlink would then delete bytes this intent never
//! produced — the predecessor it displaced still sitting in the scratch.
//!
//! Publications of one workspace **in this process** additionally serialize on
//! the store's workspace gate, and that gate is taken **first**. The one lock
//! order is **gate → flock → transaction**: no path takes a write transaction
//! before the pair, and the flock wait happens inside the gate, so at most one
//! publication per workspace ever occupies a blocking worker waiting for the
//! flock — a crowd of same-process waiters queues on the async gate instead of
//! filling the pool the lock holder needs for its own file work.
//!
//! The gate belongs to the blocking task that runs the wait, not to the awaiting
//! future ([`PublishFlock::acquire`] takes the locked gate by value): a cancelled
//! caller drops only its handle, that task keeps the gate until `flock` has
//! actually returned, and only then releases both. Cancelling a waiter therefore
//! never starts a second one, however often it happens.
//!
//! The file operations of the section run on a blocking thread that **owns the
//! lease** for its whole duration ([`PublishFlock::section`], synchronous
//! `std::fs`): the caller awaits the task's handle and holds nothing. That is
//! the cancellation contract. A dropped request future releases no lock — it
//! drops the handle, the blocking task still runs its ordered operations
//! (link → verify → unlink) to completion and releases the flock itself, which
//! is what the point of no return requires: the effect is already committed, so
//! it finishes even when its caller is gone. A lease owned by the caller future
//! instead could be closed while a queued unlink was still running, and that
//! unlink could then delete a target the next provider had published.
//!
//! That lock is the entire cross-process contract. Files under the file root are
//! the provider's own projections, and an external process that writes them
//! while bypassing the provider API is outside it: such a writer cannot be made
//! to take the lock or to honour the ownership rule, so the rule above preserves
//! the bytes it actually reads and does not coordinate with them. A write that
//! lands after an ownership read and before the unlink that consumes the same
//! name — inside the displacement, or on a scratch about to be dropped — can
//! still be deleted with that name.
//!
//! A quarantined intent keeps its journaled payload: the projection it owes is
//! still recoverable, so an operator who resolves the target lets a later pass
//! re-drive the same intent (`quarantined` → `applied`). Its receipt records
//! the failure terminally rather than reporting an outcome that never settles.
//!
//! The target is never overwritten blindly. [`FileEffect::prior`] carries the
//! digest of the provider's own previous projection of the same row, so
//! publishing a committed new version over its legitimate predecessor is
//! distinguishable from clobbering bytes a foreign writer left there; anything
//! unrecognized is quarantined and left untouched.
//!
//! File effects are the provider's own markdown projection of its
//! provider-owned content (`records/<record_id>.md`,
//! `candidates/<candidate_id>.md`, `profiles/<digest>.md`) under the
//! workspace's `memory-store/` directory. They never touch the legacy bearer
//! layout (`SOUL.md`, `memory/long-term/*.md`), which D11/Global Constraints
//! keep byte-identical, and they are the artifact N4 transfer tooling stages.
//!
//! The digest is the provider's standard `jcs_sha256_hex` over the rendered
//! markdown as a JSON string — the same helper the candidate `content_digest`
//! uses — so journal, publish and recovery all compare one computation.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::storage::{CandidateRow, ProfileRow, RecordRow, StoreError, StoreResult};

/// The provider's file root inside the workspace directory, next to `state.db`.
pub(super) const FILES_DIR: &str = "memory-store";

/// How many unsettled intents one recovery pass settles. Recovery runs at open
/// and must never page an unbounded journal; a pass that leaves work behind
/// marks it [`STATE_DEFERRED`], which reads answer terminally until a later
/// pass reaches it.
pub(super) const RECOVERY_BOUND: i64 = 64;

pub(super) const STATE_PENDING: &str = "pending";
pub(super) const STATE_APPLIED: &str = "applied";
pub(super) const STATE_QUARANTINED: &str = "quarantined";
/// Left over by a bounded pass: still recoverable, but no longer silently
/// withheld — the subject's reads answer the terminal refusal (§5.2 step 5).
pub(super) const STATE_DEFERRED: &str = "deferred";

/// The workspace publication lock file, at the root of the provider's own file
/// tree. Hidden, never removed: unlinking it would let the next holder lock a
/// different inode while an earlier one still holds the old.
pub(super) const PUBLISH_LOCK_FILE: &str = ".memory-publish.lock";

/// The cross-process half of the publish lock (§5.2 step 2): an exclusive
/// `flock` on [`PUBLISH_LOCK_FILE`], held for the whole settle → publish section
/// of one operation (or recovery pass).
///
/// Two provider **processes** on one workspace serialize their file effects
/// through it, which is what closes the displacement window the ownership rule
/// cannot: `hard_link(target, scratch)` followed by `remove_file(target)` leaves
/// a moment where a second process could atomically replace the target with its
/// own content, and the unlink would delete that content while the scratch still
/// held the predecessor. The lock removes the second process from that section
/// entirely. Against **non-provider** writers the ownership rule is all there
/// is: it preserves the bytes it reads, and cannot coordinate with a write that
/// lands between one of those reads and the unlink of the same name — see the
/// module documentation for that boundary.
///
/// Held for the section by whichever blocking task is running it
/// ([`Self::section`]), released by closing the file, never by unlinking it.
pub(super) struct PublishFlock {
    _file: std::fs::File,
}

impl PublishFlock {
    /// Take the exclusive lease, creating the lock file if it is absent.
    ///
    /// `flock` has no async form, so the wait runs on the blocking pool: a
    /// publication waiting for another process must not stall the executor's
    /// worker threads. The caller takes its in-process gate **before** this and
    /// passes it in as `held`; the task that waits owns it, and returns it with
    /// the lease. While the task waits, no other caller can take the gate, so one
    /// workspace has at most one of these waits in flight (module doc, § *Who
    /// serializes the file effects*).
    ///
    /// Cancelling the caller drops only its handle: the task keeps the gate until
    /// `file.lock()` has actually returned (successfully or not) and only then
    /// releases it, so repeated cancellation cannot stack waiters. `report_wait`
    /// runs on that blocking task just before it blocks — a test seam that shows
    /// a wait has really started; production passes a no-op.
    ///
    /// # Errors
    /// The I/O error that prevented opening the lock file or taking the lease.
    /// The gate is released on that path too: the wait is over.
    pub(super) async fn acquire<H, W>(
        root: &Path,
        held: H,
        report_wait: W,
    ) -> std::io::Result<(Self, H)>
    where
        H: Send + 'static,
        W: FnOnce() + Send + 'static,
    {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&root)?;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(root.join(PUBLISH_LOCK_FILE))?;
            report_wait();
            file.lock()?;
            Ok((Self { _file: file }, held))
        })
        .await
        .map_err(|error| std::io::Error::other(format!("publish lock task: {error}")))?
    }

    /// Run one file-effect section on a blocking thread that owns this lease for
    /// its whole duration, then hand the lease back for the next section.
    ///
    /// The closure runs synchronous `std::fs` operations only. Cancelling the
    /// caller drops the task handle, never the lease: the section still runs its
    /// ordered operations to completion and releases the lock itself, which is
    /// what keeps a committed effect from outliving its own lock.
    pub(super) async fn section<T, F>(self, work: F) -> (Self, T)
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // A join error is a panic inside the section, not an I/O outcome: the
        // unwinding task has already released the lease.
        tokio::task::spawn_blocking(move || (work(), self))
            .await
            .map(|(out, lease)| (lease, out))
            .expect("file-effect section task")
    }
}

/// One planned file write: the target relative to the file root, the exact
/// bytes, their digest, and the digest of the projection this intent replaces.
/// The same shape is stored in the journal and replayed by recovery, so an
/// intent and its effect cannot drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileEffect {
    pub path: String,
    pub digest: String,
    /// The provider's own previous projection of this target, or `None` when
    /// the target must not exist yet. Taken from the stored row, never from
    /// whatever bytes happen to sit at the path, so foreign bytes are never
    /// adopted as a legitimate predecessor.
    #[serde(default)]
    pub prior: Option<String>,
    pub content: String,
}

impl FileEffect {
    /// Declare the projection this effect replaces.
    pub(super) fn over(mut self, prior: Option<String>) -> Self {
        self.prior = prior;
        self
    }
}

/// What recovery did with one target.
pub(super) enum Settled {
    Published,
    Quarantined(String),
}

/// The protocol step a deterministic test seam runs at, so a regression can
/// land a foreign write or an I/O failure at the exact instant the step depends
/// on. Production passes [`noop`], which ignores the point and never fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    /// After the scratch ownership read, before the target is displaced.
    BeforeDisplacement,
    /// After the target is displaced, before the new bytes are linked.
    AfterDisplacement,
    /// Immediately before an obsolete scratch is unlinked.
    BeforeScratchRemoval,
}

fn digest_of(content: &str) -> StoreResult<String> {
    match spoke_operations::jcs_sha256_hex(&Value::String(content.to_string())) {
        spoke_operations::SpokeResult::Ok(digest) => Ok(digest),
        spoke_operations::SpokeResult::Reject(reject) => Err(StoreError::Corrupt(reject.message)),
    }
}

fn effect(path: String, content: String) -> StoreResult<FileEffect> {
    let digest = digest_of(&content)?;
    Ok(FileEffect {
        path,
        digest,
        prior: None,
        content,
    })
}

/// A frontmatter value must stay one line; ids come off the wire.
fn line(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

fn body_text(content_json: &str) -> String {
    match serde_json::from_str::<Value>(content_json) {
        Ok(Value::Object(map)) => map
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(|| content_json.to_string(), str::to_string),
        _ => content_json.to_string(),
    }
}

fn document_text(document_json: &str) -> String {
    match serde_json::from_str::<Value>(document_json) {
        Ok(Value::Object(map)) => map
            .get("body")
            .and_then(Value::as_str)
            .map_or_else(|| document_json.to_string(), str::to_string),
        _ => document_json.to_string(),
    }
}

/// A frontmatter line for an optional value, or nothing when it is absent.
fn optional(label: &str, value: Option<&str>) -> String {
    value.map_or_else(String::new, |value| format!("{label}: {}\n", line(value)))
}

/// §6.6 record projection target. The one source of "which file this row owns":
/// the journal writes it and an operation's predecessor check asks for it.
pub(super) fn record_target(record_id: &str) -> String {
    format!("records/{record_id}.md")
}

/// §6.7 consumed-candidate shell target.
pub(super) fn candidate_target(candidate_id: &str) -> String {
    format!("candidates/{candidate_id}.md")
}

/// §6.8 profile target. The file name is the digest of the subject/kind pair, so
/// open-vocabulary ids never become path components.
pub(super) fn profile_target(subject_id: &str, profile_kind: &str) -> StoreResult<String> {
    let name = digest_of(&format!("{subject_id}\u{1f}{profile_kind}"))?;
    Ok(format!("profiles/{name}.md"))
}

/// §6.6 record projection: the stored row as markdown.
pub(super) fn record_effect(row: &RecordRow) -> StoreResult<FileEffect> {
    let out = format!(
        "---\n\
         record_id: {}\n\
         space_id: {}\n\
         subject_id: {}\n\
         kind: {}\n\
         visibility: {}\n\
         {}\
         revision: {}\n\
         created_at: {}\n\
         updated_at: {}\n\
         {}\
         {}\
         ---\n\n\
         {}",
        line(&row.record_id),
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.kind),
        line(&row.scope_visibility),
        optional("context_id", row.scope_context_id.as_deref()),
        row.revision,
        line(&row.created_at),
        line(&row.updated_at),
        optional("occurred_at", row.occurred_at.as_deref()),
        if row.tombstone_json.is_some() {
            "tombstoned: true\n"
        } else {
            ""
        },
        body_text(&row.content_json),
    );
    effect(record_target(&row.record_id), out)
}

/// §6.7 consumed-candidate shell projection: the content-free audit shell.
pub(super) fn candidate_effect(row: &CandidateRow) -> StoreResult<FileEffect> {
    let out = format!(
        "---\n\
         candidate_id: {}\n\
         space_id: {}\n\
         subject_id: {}\n\
         kind: {}\n\
         status: {}\n\
         revision: {}\n\
         created_at: {}\n\
         {}\
         ---\n\n\
         {}",
        line(&row.candidate_id),
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.kind),
        line(&row.status),
        row.revision,
        line(&row.created_at),
        optional("content_digest", row.content_digest.as_deref()),
        row.consumed_by_json.as_deref().unwrap_or(""),
    );
    effect(candidate_target(&row.candidate_id), out)
}

/// §6.8 profile projection. The file name is the digest of the subject/kind
/// pair, so open-vocabulary ids never become path components.
pub(super) fn profile_effect(row: &ProfileRow) -> StoreResult<FileEffect> {
    let target = profile_target(&row.subject_id, &row.profile_kind)?;
    let out = format!(
        "---\n\
         space_id: {}\n\
         subject_id: {}\n\
         profile_kind: {}\n\
         revision: {}\n\
         updated_by: {}\n\
         created_at: {}\n\
         updated_at: {}\n\
         ---\n\n\
         {}",
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.profile_kind),
        row.revision,
        line(&row.updated_by),
        line(&row.created_at),
        line(&row.updated_at),
        document_text(&row.document_json),
    );
    effect(target, out)
}

/// Resolve an intent target below the file root, refusing anything that would
/// leave it (absolute paths, `..`, prefixes).
fn resolve(root: &Path, effect: &FileEffect) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let mut components = Path::new(&effect.path).components();
    match components.next() {
        Some(Component::Normal(part)) => path.push(part),
        _ => return None,
    }
    for component in components {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(path)
}

/// The hidden staged sibling of a target: dot-prefixed and receipt-scoped, so
/// no provider reader lists it and two operations never share a staged file.
fn staged(target: &Path, receipt_id: &str) -> PathBuf {
    hidden(target, &format!("{receipt_id}.staged"))
}

/// The hidden sibling a target is displaced to while it is replaced: dot-prefixed
/// and receipt-scoped, so it is invisible to readers and two intents never share
/// one scratch path.
fn displaced(target: &Path, receipt_id: &str) -> PathBuf {
    hidden(target, &format!("{receipt_id}.prior"))
}

fn hidden(target: &Path, suffix: &str) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("target");
    target.with_file_name(format!(".{name}.{suffix}"))
}

fn write_staged(root: &Path, receipt_id: &str, effect: &FileEffect) -> std::io::Result<PathBuf> {
    let Some(target) = resolve(root, effect) else {
        return Err(std::io::Error::other("unsafe journal target"));
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staged_path = staged(&target, receipt_id);
    std::fs::write(&staged_path, effect.content.as_bytes())?;
    // A-13: the staged bytes — and the directory entry that names them — must be
    // on disk before the journal's recovery bytes are cleared, or a power loss
    // loses the file while recovery still sees the intent as applied.
    sync_path(&staged_path)?;
    Ok(staged_path)
}

/// fsync one file and the directory holding its name, so the write/link/rename
/// that installed it survives a power loss (A-13; the repo's durable-write
/// precedents fsync the file and then its parent directory).
fn sync_path(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn matches_digest(bytes: &[u8], digest: &str) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|content| digest_of(content).ok())
        .is_some_and(|actual| actual == digest)
}

/// Whether these bytes are content this intent may drop: its own projection or
/// the exact predecessor it replaces. Everything else belongs to someone else.
fn obsolete(bytes: &[u8], effect: &FileEffect) -> bool {
    matches_digest(bytes, &effect.digest)
        || effect
            .prior
            .as_deref()
            .is_some_and(|prior| matches_digest(bytes, prior))
}

/// Write the staged file for one intent (step 2's first half).
///
/// Synchronous by design: the caller runs it as part of a
/// [`PublishFlock::section`], which owns the lease for the whole call.
pub(super) fn stage(root: &Path, receipt_id: &str, effect: &FileEffect) -> std::io::Result<()> {
    write_staged(root, receipt_id, effect).map(|_| ())
}

/// Verify the staged bytes against the intent digest and install them at the
/// target without replacing bytes this intent did not produce.
///
/// Synchronous by design: the caller runs it as part of a
/// [`PublishFlock::section`], which owns the lease for the whole call.
pub(super) fn publish(root: &Path, receipt_id: &str, effect: &FileEffect) -> std::io::Result<()> {
    let Some(target) = resolve(root, effect) else {
        return Err(std::io::Error::other("unsafe journal target"));
    };
    let staged_path = staged(&target, receipt_id);
    let bytes = std::fs::read(&staged_path)?;
    if !matches_digest(&bytes, &effect.digest) {
        return Err(std::io::Error::other(
            "staged file does not match the intent digest",
        ));
    }
    install(&staged_path, &target, receipt_id, effect)?;
    // A-13: the settlement that clears the journal's recovery bytes runs only
    // after the installed file and its directory entry are durable.
    sync_path(&target)
}

/// Move the staged bytes into place under the intent's ownership rule.
///
/// The target must be absent, already hold the intent's content (a crash
/// between the install and the applied mark), or hold exactly the recorded
/// predecessor. Nothing here removes a pathname it did not first give a second
/// name: the target is displaced to a receipt-scoped scratch sibling by a
/// no-clobber link followed by an unlink, and only the bytes that link carried
/// away are verified. Bytes this intent did not produce are put back untouched
/// for every state this protocol reads, so a writer landing at any of those
/// checks keeps its file — including between the ownership check and the
/// removal that a check-then-remove protocol would have performed, and between
/// the read of the scratch name and the displacement. The two windows the rule
/// cannot see are the unlinks of a name it has just read (the displacement's
/// link→unlink, the scratch-name retry); the module doc bounds them.
///
/// A scratch already at that name is an earlier attempt's leftover, never a
/// free slot: obsolete content is dropped, anything else is somebody's bytes an
/// attempt could not put back, and this one refuses instead of replacing them.
fn install(
    staged_path: &Path,
    target: &Path,
    receipt_id: &str,
    effect: &FileEffect,
) -> std::io::Result<()> {
    install_verifying(staged_path, target, receipt_id, effect, &noop)
}

/// The seam production passes: every protocol point is a no-op.
// The result type is the seam's contract, not a value being wrapped.
#[allow(clippy::unnecessary_wraps)]
const fn noop(_: Point) -> std::io::Result<()> {
    Ok(())
}

/// [`install`] with the deterministic seam the regressions use to land a foreign
/// write or an I/O failure at the exact point the swap depends on. Plainly a
/// no-op in production paths.
fn install_verifying<F>(
    staged_path: &Path,
    target: &Path,
    receipt_id: &str,
    effect: &FileEffect,
    seam: &F,
) -> std::io::Result<()>
where
    F: Fn(Point) -> std::io::Result<()> + Send + Sync,
{
    let scratch = displaced(target, receipt_id);
    match std::fs::read(&scratch) {
        Ok(bytes) if obsolete(&bytes, effect) => {
            seam(Point::BeforeScratchRemoval)?;
            std::fs::remove_file(&scratch)?;
        }
        Ok(_) => return Err(preserved(effect, &scratch)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let taken = displace(target, &scratch, effect, seam)?;
    if taken {
        let bytes = std::fs::read(&scratch)?;
        if matches_digest(&bytes, &effect.digest) {
            // Crash between the install and the applied mark: the displaced
            // bytes ARE this intent's content, so they go back to the target —
            // dropping them would report success with the projection absent.
            match restore(&scratch, target) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(std::io::Error::other(format!(
                        "{} was replaced while this intent published",
                        effect.path
                    )));
                }
                Err(error) => return Err(error),
            }
            let _ = std::fs::remove_file(staged_path);
            drop_obsolete_scratch(&scratch, effect, seam)?;
            return Ok(());
        }
        if !obsolete(&bytes, effect) {
            // Not ours: restore it where it was, without replacing anything that
            // landed meanwhile. If even that is impossible the bytes stay at the
            // scratch path — preserved, never deleted.
            let _ = restore(&scratch, target);
            return Err(std::io::Error::other(format!(
                "{} holds content this intent did not produce",
                effect.path
            )));
        }
    }
    seam(Point::AfterDisplacement)?;
    match std::fs::hard_link(staged_path, target) {
        Ok(()) => {}
        // A writer created the target after it was displaced: its bytes stay.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(std::io::Error::other(format!(
                "{} was replaced while this intent published",
                effect.path
            )));
        }
        Err(error) => return Err(error),
    }
    let _ = std::fs::remove_file(staged_path);
    // The verified predecessor must not outlive the link, and a scratch a
    // concurrent attempt of this same receipt left inside its own swap is
    // obsolete too. A failure here leaves obsolete bytes on disk, so it is
    // reported and the intent stays unsettled for a later pass to re-attempt
    // instead of being marked applied.
    drop_obsolete_scratch(&scratch, effect, seam)?;
    Ok(())
}

/// Give the target a second name at the scratch path and only then unlink it.
///
/// The link is no-clobber: it fails rather than replacing whatever occupies the
/// name, so the displacement itself can never overwrite bytes that landed there
/// after the ownership read. A collision re-runs the ownership predicate
/// against the CURRENT bytes — this intent's own obsolete leftover is dropped
/// and the link retried once; anything else is preserved and the attempt
/// refuses with the target still in place.
fn displace<F>(
    target: &Path,
    scratch: &Path,
    effect: &FileEffect,
    seam: &F,
) -> std::io::Result<bool>
where
    F: Fn(Point) -> std::io::Result<()> + Send + Sync,
{
    seam(Point::BeforeDisplacement)?;
    match std::fs::hard_link(target, scratch) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes = std::fs::read(scratch)?;
            if !obsolete(&bytes, effect) {
                return Err(preserved(effect, scratch));
            }
            // This intent's own obsolete leftover: drop it and take the name
            // once more. The read above decides; a name re-taken between that
            // read and this unlink would be deleted with it.
            // shortcut: unlink-by-name cannot prove the inode it read; upgrade
            // to an fd-relative unlink if the platform layer ever exposes one.
            std::fs::remove_file(scratch)?;
            match std::fs::hard_link(target, scratch) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    return Err(preserved(effect, scratch));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        Err(error) => return Err(error),
    }
    std::fs::remove_file(target)?;
    Ok(true)
}

/// The refusal for a scratch name holding bytes this intent did not produce.
fn preserved(effect: &FileEffect, scratch: &Path) -> std::io::Error {
    std::io::Error::other(format!(
        "{} preserves bytes at {} that this intent did not produce",
        effect.path,
        scratch.display()
    ))
}

/// Put preserved bytes back where they were, never over anything that appeared
/// meanwhile, and drop the scratch only once they are back.
fn restore(scratch: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::hard_link(scratch, target)?;
    let _ = std::fs::remove_file(scratch);
    Ok(())
}

/// Settle one target from the crash-consistent state on disk: publish an
/// already-consistent target, complete from a scratch a crash left behind,
/// publish a matching staged file, or regenerate the staged file from the
/// journaled bytes. Anything that would overwrite content the intent did not
/// produce is quarantined instead, and obsolete scratch — this intent's own
/// bytes or the predecessor it replaces — never survives the settlement.
///
/// Synchronous by design: the caller runs it as part of a
/// [`PublishFlock::section`], which owns the lease for the whole call.
pub(super) fn settle(root: &Path, receipt_id: &str, effect: &FileEffect) -> Settled {
    settle_verifying(root, receipt_id, effect, &noop)
}

/// [`settle`] with the deterministic seam the cleanup-failure regression uses
/// to fail the obsolete-scratch removal at the exact call. Plainly a no-op in
/// production paths.
fn settle_verifying<F>(root: &Path, receipt_id: &str, effect: &FileEffect, seam: &F) -> Settled
where
    F: Fn(Point) -> std::io::Result<()> + Send + Sync,
{
    let Some(target) = resolve(root, effect) else {
        return Settled::Quarantined("unsafe journal target".to_string());
    };
    let staged_path = staged(&target, receipt_id);
    let scratch = displaced(&target, receipt_id);
    match std::fs::read(&target) {
        Ok(bytes) if matches_digest(&bytes, &effect.digest) => {
            // Crash after publish, before the applied mark: the content is
            // already consistent, so the stale staged copy and the scratch an
            // interrupted displacement left (a superseded predecessor, or a
            // duplicate of this content) are dropped. A cleanup that fails is
            // reported: the deleted body is still on disk, so the intent may
            // not be marked applied — the next pass retries the removal.
            let _ = std::fs::remove_file(&staged_path);
            return match drop_obsolete_scratch(&scratch, effect, seam) {
                Ok(()) => durable_published(&target),
                Err(error) => Settled::Quarantined(format!("scratch cleanup failed: {error}")),
            };
        }
        Ok(bytes) => {
            let owned = effect
                .prior
                .as_deref()
                .is_some_and(|prior| matches_digest(&bytes, prior));
            if !owned {
                return Settled::Quarantined(format!(
                    "target {} already holds different content",
                    effect.path
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Settled::Quarantined(format!("target read failed: {error}"));
        }
    }
    // A scratch holds the previous version this intent displaced and never
    // superseded — or the intent's own bytes, when the crash landed after the
    // link. Verified against the intent's own digests, it completes the intent
    // without rewriting the journaled content.
    if std::fs::read(&scratch).is_ok_and(|bytes| matches_digest(&bytes, &effect.digest)) {
        match restore(&scratch, &target) {
            Ok(()) => {
                let _ = std::fs::remove_file(&staged_path);
                return durable_published(&target);
            }
            // The target was re-created meanwhile: its bytes stay, and the
            // ownership rule below decides.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Settled::Quarantined(format!("scratch restore failed: {error}"));
            }
        }
    }
    let staged_bytes = std::fs::read(&staged_path).ok();
    let staged_path = match staged_bytes {
        Some(bytes) if matches_digest(&bytes, &effect.digest) => staged_path,
        // Missing or mismatched staged content: regenerate it from the
        // journaled bytes, verify what was written, and publish that.
        _ => match write_staged(root, receipt_id, effect) {
            Ok(path) => path,
            Err(error) => return Settled::Quarantined(format!("regeneration failed: {error}")),
        },
    };
    let written = match std::fs::read(&staged_path) {
        Ok(bytes) => bytes,
        Err(error) => return Settled::Quarantined(format!("staged read failed: {error}")),
    };
    if !matches_digest(&written, &effect.digest) {
        return Settled::Quarantined("journaled content does not match its digest".to_string());
    }
    match install_verifying(&staged_path, &target, receipt_id, effect, seam) {
        // The install removes every obsolete scratch before it reports success.
        Ok(()) => durable_published(&target),
        Err(error) => Settled::Quarantined(format!("publish failed: {error}")),
    }
}

/// Report the intent as published, after making the target and its directory
/// entry durable (A-13): the journal's recovery bytes are cleared in the same
/// settlement, so a file a power loss could still revert must keep the intent
/// unsettled. The payload survives, so a later pass retries the sync.
fn durable_published(target: &Path) -> Settled {
    match sync_path(target) {
        Ok(()) => Settled::Published,
        Err(error) => Settled::Quarantined(format!("target sync failed: {error}")),
    }
}

/// Drop a scratch holding content this intent may remove — its own bytes or the
/// predecessor it replaces — and leave anything else exactly where it is. A read
/// or unlink failure is returned: the obsolete bytes are still on disk, so the
/// caller must keep the intent unsettled instead of settling it as published.
fn drop_obsolete_scratch<F>(scratch: &Path, effect: &FileEffect, seam: &F) -> std::io::Result<()>
where
    F: Fn(Point) -> std::io::Result<()> + Send + Sync,
{
    match std::fs::read(scratch) {
        Ok(bytes) if obsolete(&bytes, effect) => {
            seam(Point::BeforeScratchRemoval)?;
            std::fs::remove_file(scratch)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FileEffect {
        effect("records/rec_x.md".to_string(), "body\n".to_string()).unwrap()
    }

    #[test]
    fn targets_cannot_escape_the_file_root() {
        let root = Path::new("/root");
        assert_eq!(
            resolve(root, &sample()),
            Some(PathBuf::from("/root/records/rec_x.md"))
        );
        for path in ["/abs.md", "../out.md", "records/../../out.md", ""] {
            let hostile = FileEffect {
                path: path.to_string(),
                digest: String::new(),
                prior: None,
                content: String::new(),
            };
            assert_eq!(resolve(root, &hostile), None, "{path:?} escapes the root");
        }
    }

    #[test]
    fn staged_names_are_hidden_and_receipt_scoped() {
        let target = Path::new("/root/records/rec_x.md");
        assert_eq!(
            staged(target, "rcp_a"),
            PathBuf::from("/root/records/.rec_x.md.rcp_a.staged")
        );
    }

    /// §5.2 step 2's ownership rule: a version is installed only where there is
    /// nothing, where its own content already is, or where the intent's recorded
    /// predecessor is — never over bytes the intent did not produce.
    #[test]
    fn install_replaces_its_predecessor_and_never_foreign_bytes() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "one\n");

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        publish(root, "rcp_b", &second).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two\n");

        std::fs::write(&target, "foreign\n").unwrap();
        let third = effect("records/rec_x.md".to_string(), "three\n".to_string())
            .unwrap()
            .over(Some(second.digest));
        stage(root, "rcp_c", &third).unwrap();
        let error = publish(root, "rcp_c", &third).unwrap_err();
        assert!(
            error.to_string().contains("did not produce"),
            "an unowned target is refused: {error}"
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "foreign\n");
    }

    /// F1: the ownership check and the predecessor's removal were two steps, so
    /// a writer replacing the target between them had its file deleted by the
    /// provider's `remove_file`, which the no-clobber link could not undo. The
    /// target is now displaced before anything is verified, so a write landing in
    /// that exact window keeps its bytes and the intent refuses.
    #[test]
    fn a_foreign_replacement_during_the_swap_is_never_deleted() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        let staged_path = staged(&target, "rcp_b");

        // The foreign writer lands after the predecessor was displaced and
        // verified, at the point the old protocol removed the target.
        let error = install_verifying(&staged_path, &target, "rcp_b", &second, &|point| {
            if point == Point::AfterDisplacement {
                std::fs::write(&target, "foreign\n").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("replaced while this intent published"),
            "a target re-created during the swap is refused: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "foreign\n",
            "the foreign file survives the refused publication"
        );
        assert_eq!(
            std::fs::read_to_string(displaced(&target, "rcp_b")).unwrap(),
            "one\n",
            "the verified predecessor is preserved, not deleted"
        );
        assert!(
            staged_path.exists(),
            "the staged bytes stay published-from until the intent settles"
        );
    }

    /// NF1: a target that already holds the intent's content is displaced by the
    /// swap and must be linked back. Dropping the displaced copy reported success
    /// with no projection on disk at all.
    #[test]
    fn a_consistent_target_survives_the_install_that_displaces_it() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        // A re-drive of the same intent: the target is already consistent, so the
        // swap that displaces it has to put those bytes back.
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "one\n",
            "the intent's own content is at the target after a successful install"
        );
        assert!(!displaced(&target, "rcp_a").exists());
        assert!(!staged(&target, "rcp_a").exists());
    }

    /// NF2(a): a crash between displacing the predecessor and linking the new
    /// bytes leaves the target absent and the predecessor in scratch. Recovery
    /// must finish the intent and drop the superseded bytes — leaving them is a
    /// deleted body surviving its own tombstone.
    #[test]
    fn settle_supersedes_the_scratch_a_displacement_crash_left() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        std::fs::rename(&target, displaced(&target, "rcp_b")).unwrap();
        assert!(!target.exists(), "the crash window leaves no target");

        assert!(matches!(settle(root, "rcp_b", &second), Settled::Published));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two\n");
        assert!(
            !displaced(&target, "rcp_b").exists(),
            "the superseded predecessor is not left behind in a hidden file"
        );
        assert!(!staged(&target, "rcp_b").exists());
    }

    /// NF2(a): a scratch can also hold the intent's own content — the swap linked
    /// it, then the process died before the applied mark. Recovery completes from
    /// that evidence instead of rewriting the journaled bytes.
    #[test]
    fn settle_completes_from_a_scratch_that_holds_its_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        std::fs::rename(&target, displaced(&target, "rcp_b")).unwrap();
        // The staged copy went with the process; only the scratch has the bytes.
        std::fs::remove_file(staged(&target, "rcp_b")).unwrap();

        assert!(matches!(settle(root, "rcp_b", &second), Settled::Published));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two\n");
        assert!(!displaced(&target, "rcp_b").exists());
    }

    /// NF2(b): bytes an earlier attempt could not put back stay at the scratch
    /// name. Reusing that name on the retry replaced them on any platform with
    /// ordinary rename replacement semantics.
    #[test]
    fn a_preserved_scratch_is_never_replaced_by_a_retry() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        let preserved = displaced(&target, "rcp_b");
        std::fs::write(&preserved, "foreign\n").unwrap();

        let error = publish(root, "rcp_b", &second).unwrap_err();
        assert!(
            error.to_string().contains("preserves bytes"),
            "a scratched name holding someone else's bytes is refused: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(&preserved).unwrap(),
            "foreign\n",
            "the preserved bytes survive the retry"
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "one\n",
            "the refused retry publishes nothing"
        );
    }

    /// F1(a): the verified predecessor's scratch is unlinked after the new bytes
    /// are linked. Ignoring that unlink's failure marked the intent applied while
    /// the deleted body was still on disk in a hidden file no later pass would
    /// select again. The failure is propagated now, so the intent stays
    /// unsettled and the next pass re-attempts the removal.
    #[test]
    fn a_predecessor_scratch_that_cannot_be_removed_keeps_the_intent_unsettled() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        let staged_path = staged(&target, "rcp_b");

        let error = install_verifying(&staged_path, &target, "rcp_b", &second, &|point| {
            if point == Point::BeforeScratchRemoval {
                return Err(std::io::Error::other("injected scratch unlink failure"));
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "two\n",
            "the tombstone is published; only the cleanup failed"
        );
        assert_eq!(
            std::fs::read_to_string(displaced(&target, "rcp_b")).unwrap(),
            "one\n",
            "the deleted body is still on disk, so the intent may not settle"
        );

        // The retry the unsettled intent gets: recovery's consistent branch
        // removes the body and only then settles the intent.
        assert!(matches!(settle(root, "rcp_b", &second), Settled::Published));
        assert!(
            !displaced(&target, "rcp_b").exists(),
            "the retried cleanup removes the deleted body"
        );
    }

    /// F1(b): recovery's consistent-target branch also dropped the obsolete
    /// scratch with its errors ignored and answered `Published`. The deleted body
    /// then stayed on disk under an applied intent. The failure keeps the entry
    /// unsettled, and the next pass that can remove it settles the intent.
    #[test]
    fn settle_keeps_the_intent_recoverable_when_the_cleanup_fails() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        // The state the failed install above leaves: the new content at the
        // target, the deleted body at the receipt's scratch name.
        std::fs::write(&target, "two\n").unwrap();
        let scratch = displaced(&target, "rcp_b");
        std::fs::write(&scratch, "one\n").unwrap();

        match settle_verifying(root, "rcp_b", &second, &|point| {
            if point == Point::BeforeScratchRemoval {
                return Err(std::io::Error::other("injected scratch unlink failure"));
            }
            Ok(())
        }) {
            Settled::Quarantined(reason) => assert!(reason.contains("injected"), "{reason}"),
            Settled::Published => panic!("a failed cleanup may not settle as published"),
        }
        assert_eq!(
            std::fs::read_to_string(&scratch).unwrap(),
            "one\n",
            "the deleted body stays until it is actually removed"
        );

        assert!(matches!(settle(root, "rcp_b", &second), Settled::Published));
        assert!(!scratch.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two\n");
    }

    /// F2: the scratch name was read for ownership and then taken by a
    /// replacement-capable rename, so bytes another attempt or process put there
    /// after the read were overwritten without ever being inspected. The
    /// displacement now hard-links — it replaces nothing — and a collision
    /// re-runs the ownership predicate against the CURRENT bytes: foreign bytes
    /// survive and the intent refuses with the target still in place.
    #[test]
    fn a_scratch_retaken_after_the_ownership_read_is_never_clobbered() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        let staged_path = staged(&target, "rcp_b");
        let scratch = displaced(&target, "rcp_b");

        let error = install_verifying(&staged_path, &target, "rcp_b", &second, &|point| {
            if point == Point::BeforeDisplacement {
                std::fs::write(&scratch, "foreign\n").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
        assert!(
            error.to_string().contains("preserves bytes"),
            "the retaken name is refused, not replaced: {error}"
        );
        assert_eq!(
            std::fs::read_to_string(&scratch).unwrap(),
            "foreign\n",
            "bytes landed after the ownership read are never deleted"
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "one\n",
            "the refused attempt never displaced the target"
        );
        assert!(staged_path.exists());
    }

    /// F2: the same collision with this intent's own obsolete leftover is
    /// dropped and the displacement retried once, so a superseded name does not
    /// block its own publication.
    #[test]
    fn an_obsolete_scratch_retaken_after_the_read_is_dropped_and_retried() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let target = root.join("records/rec_x.md");

        let first = effect("records/rec_x.md".to_string(), "one\n".to_string()).unwrap();
        stage(root, "rcp_a", &first).unwrap();
        publish(root, "rcp_a", &first).unwrap();

        let second = effect("records/rec_x.md".to_string(), "two\n".to_string())
            .unwrap()
            .over(Some(first.digest));
        stage(root, "rcp_b", &second).unwrap();
        let staged_path = staged(&target, "rcp_b");
        let scratch = displaced(&target, "rcp_b");

        install_verifying(&staged_path, &target, "rcp_b", &second, &|point| {
            if point == Point::BeforeDisplacement {
                std::fs::write(&scratch, "one\n").unwrap();
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "two\n");
        assert!(
            !scratch.exists(),
            "the retried displacement leaves no scratch"
        );
        assert!(!staged_path.exists());
    }

    /// F-new-1: the lease belongs to the blocking task that runs the file
    /// operations, never to the caller's future. When the caller is cancelled
    /// while the section is in flight, only its handle goes away — the task
    /// finishes its ordered operations and releases the flock itself. A lease
    /// owned by the cancelled future would be closed under a queued unlink, and
    /// that unlink could then delete the file the next provider had published.
    #[test]
    fn a_cancelled_section_keeps_the_lease_until_its_file_work_finishes() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join(FILES_DIR);
        let target = root.join("records/rec_x.md");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let (lease, ()) = runtime
            .block_on(PublishFlock::acquire(&root, (), || {}))
            .expect("the workspace lease");

        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel::<()>();
        let written = target.clone();
        let section = runtime.spawn(async move {
            lease
                .section(move || {
                    std::fs::create_dir_all(written.parent().unwrap()).unwrap();
                    std::fs::write(&written, "one\n").unwrap();
                    entered_tx.send(()).unwrap();
                    // The operation the cancelled caller can no longer wait for.
                    finish_rx.recv().unwrap();
                    std::fs::write(&written, "two\n").unwrap();
                })
                .await
        });
        entered_rx.recv().expect("the section runs with the lease");

        section.abort();
        assert!(
            matches!(runtime.block_on(section), Err(error) if error.is_cancelled()),
            "the caller's future is dropped"
        );
        let probe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(PUBLISH_LOCK_FILE))
            .unwrap();
        assert!(
            probe.try_lock().is_err(),
            "the lease stays held while the section's file work is in flight"
        );

        finish_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while probe.try_lock().is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "the finished section releases the lease itself"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "two\n",
            "the committed section ran its last file operation past the cancellation"
        );
    }
}
