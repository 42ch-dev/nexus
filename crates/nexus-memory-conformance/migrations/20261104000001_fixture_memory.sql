-- Independent fixture memory provider: durable schema (spoke-memory §6.2/§6.3/
-- §6.5/§6.6/§6.7/§6.8/§6.10).
--
-- This database belongs to the conformance fixture alone. It shares no table,
-- trigger, outbox or writer protocol with the Nexus local provider, and the
-- fixture never imports Nexus storage code: the tables below are the fixture's
-- own durable shape for the protocol surface its private operation slices serve.
--
-- Revision columns are the CAS surface: an operation compares the expected
-- revision and bumps it inside one transaction, so a stale writer is rejected
-- rather than told "matched" by a trigger its own WHERE clause refused.
--
-- JSON columns keep the protocol's open vocabularies (capabilities, context
-- ids, provenance, extensions) as opaque payloads: the fixture validates them in
-- its operation slices, never by schema enums.

-- §6.2: the single space provisioned on first open, carrying the authority pair
-- clients pin. `authority_generation` and `revision` are monotonic.
CREATE TABLE IF NOT EXISTS spaces (
    space_id TEXT PRIMARY KEY,
    display_name TEXT,
    authority_host_id TEXT NOT NULL,
    authority_generation INTEGER NOT NULL CHECK (authority_generation >= 1),
    status TEXT NOT NULL CHECK (status IN ('active', 'frozen')),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}'
) WITHOUT ROWID;

-- §7.0.7/MEM-R026: provider-private snapshot marker. A monotonic per-space
-- counter advanced inside the same transaction as every observable state
-- change; it is deliberately not the public space `revision` and not
-- `authority_generation`, because grant and subject changes must invalidate
-- outstanding cursors without rewriting the public space object. Cursor
-- bindings carry this value plus the engine policy version.
CREATE TABLE IF NOT EXISTS provider_snapshot (
    space_id TEXT PRIMARY KEY,
    marker INTEGER NOT NULL CHECK (marker >= 1)
) WITHOUT ROWID;

-- §6.3: subject identity plus lifecycle fence state. `kind` is one optional
-- pair (a half-populated kind is malformed), and `fenced_contexts_json` holds
-- the context-scoped fence allowlist.
CREATE TABLE IF NOT EXISTS subjects (
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    kind_ns TEXT,
    kind_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('active', 'fenced', 'archived')),
    fenced_contexts_json TEXT NOT NULL DEFAULT '[]',
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (space_id, subject_id),
    CHECK ((kind_ns IS NULL) = (kind_id IS NULL))
) WITHOUT ROWID;

-- One bearer has one subject per kind: a second subject naming the same
-- (space_id, kind_ns, kind_id) is rejected instead of silently splitting
-- ownership.
CREATE UNIQUE INDEX IF NOT EXISTS idx_subjects_kind
    ON subjects (space_id, kind_ns, kind_id)
    WHERE kind_ns IS NOT NULL;

-- §6.5: grants. `subject_id` NULL means the grant covers the whole space.
-- `shared_access` NULL means "unstated" for unrestricted grants; restricted
-- grants always carry a boolean. `revoked_at`/`expires_at` are evaluated at
-- admission, not by deletion.
CREATE TABLE IF NOT EXISTS grants (
    grant_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT,
    principal_id TEXT NOT NULL,
    capabilities_json TEXT NOT NULL,
    context_ids_json TEXT NOT NULL DEFAULT '[]',
    shared_access INTEGER CHECK (shared_access IN (0, 1)),
    expires_at TEXT,
    revoked_at TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}'
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_grants_space_principal
    ON grants (space_id, principal_id);

CREATE INDEX IF NOT EXISTS idx_grants_space_subject
    ON grants (space_id, subject_id);

-- §6.7: candidate review queue. `capture_scope` is split into its two
-- queryable facets and is immutable: review outcomes inherit it. Consumption is
-- one-way and all-or-nothing — the body leaves, the digest and outcome arrive,
-- and the shell keeps its identity (a consumed candidate never resurrects).
CREATE TABLE IF NOT EXISTS candidates (
    candidate_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    capture_visibility TEXT NOT NULL CHECK (capture_visibility IN ('shared', 'context')),
    capture_context_id TEXT,
    kind TEXT NOT NULL,
    content_json TEXT,
    provenance_json TEXT NOT NULL,
    classification_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'consumed')),
    consumed_by_json TEXT,
    content_digest TEXT,
    occurred_at TEXT,
    expires_at TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    CHECK ((capture_visibility = 'context') = (capture_context_id IS NOT NULL)),
    CHECK ((status = 'consumed') = (consumed_by_json IS NOT NULL)),
    CHECK ((status = 'consumed') = (content_json IS NULL)),
    CHECK ((status = 'consumed') = (content_digest IS NOT NULL))
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_candidates_subject_status
    ON candidates (space_id, subject_id, status);

-- Only pending candidates expire; the retention sweep and the list predicate
-- both key on that status.
CREATE INDEX IF NOT EXISTS idx_candidates_pending_expires
    ON candidates (expires_at)
    WHERE status = 'pending';

-- §6.6: retained records. Placement lives outside the content, provenance is
-- independent of placement, and a tombstone empties the content in place while
-- the row keeps its identity for audit (MEM-R014).
CREATE TABLE IF NOT EXISTS records (
    record_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    content_json TEXT NOT NULL,
    scope_visibility TEXT NOT NULL CHECK (scope_visibility IN ('shared', 'context')),
    scope_context_id TEXT,
    provenance_json TEXT NOT NULL,
    import_ref_json TEXT,
    occurred_at TEXT,
    tombstone_json TEXT,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    CHECK ((scope_visibility = 'context') = (scope_context_id IS NOT NULL)),
    CHECK ((scope_visibility = 'shared') = (scope_context_id IS NULL))
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_records_subject_created
    ON records (space_id, subject_id, created_at DESC, record_id);

CREATE INDEX IF NOT EXISTS idx_records_scope
    ON records (space_id, subject_id, scope_visibility, scope_context_id);

-- §6.8: trusted authored subject documents, one row per (space, subject, kind).
CREATE TABLE IF NOT EXISTS profiles (
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    profile_kind TEXT NOT NULL,
    document_json TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (space_id, subject_id, profile_kind)
) WITHOUT ROWID;

-- §6.10: operation receipt bodies. A pending receipt carries no payload and no
-- terminal stamp; a settled receipt carries exactly one payload. Receipt
-- results stay content-free for consumed candidates.
CREATE TABLE IF NOT EXISTS receipts (
    receipt_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    op TEXT NOT NULL,
    idempotency_key TEXT,
    payload_digest TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('committed', 'pending', 'failed')),
    result_json TEXT,
    error_json TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    extensions_json TEXT NOT NULL DEFAULT '{}',
    CHECK (
        (status = 'pending' AND result_json IS NULL AND error_json IS NULL AND completed_at IS NULL)
        OR (
            status IN ('committed', 'failed')
            AND completed_at IS NOT NULL
            AND ((result_json IS NULL) <> (error_json IS NULL))
        )
    )
) WITHOUT ROWID;

-- §7.0.5/§6.10: the idempotency namespace `(space_id, requester_principal_id,
-- op, idempotency_key)` is its own durable table, not an index over receipts:
-- the row keeps the payload binding and the receipt reference after the public
-- receipt body is evicted, so the dedup identity outlives its receipt for the
-- longer dedup retention window and a replay within that window can answer
-- `RECEIPT_EXPIRED` instead of silently re-executing. `last_known_status` is
-- what that refusal reports. `receipt_id` is deliberately not a foreign key —
-- the receipt row is the part that gets deleted. The keyless
-- `memory.subjects.ensure` exemption (§7.0.5) binds no namespace row at all.
CREATE TABLE IF NOT EXISTS idempotency_namespace (
    space_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    op TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    receipt_id TEXT NOT NULL,
    last_known_status TEXT NOT NULL CHECK (last_known_status IN ('committed', 'pending', 'failed')),
    created_at TEXT NOT NULL,
    PRIMARY KEY (space_id, requester_principal_id, op, idempotency_key)
) WITHOUT ROWID;

-- §6.5/§7.28: the content-free authorization target of the operation that
-- produced a receipt — the capabilities it required plus the subject and
-- placement/selection facets any current grant must actually cover. It is kept
-- beside the receipt but independent of the receipt body, so a read of an
-- evicted body's tombstone rechecks the original target instead of guessing it
-- from whichever grant survives. It never carries payload, content, digest,
-- seed or proof material, and its row lives for the longer of the receipt-body
-- and dedup windows (the retention sweep owns that deletion).
CREATE TABLE IF NOT EXISTS receipt_targets (
    receipt_id TEXT PRIMARY KEY,
    capabilities_json TEXT NOT NULL,
    subject_id TEXT,
    context_ids_json TEXT NOT NULL DEFAULT '[]',
    reads_shared INTEGER NOT NULL CHECK (reads_shared IN (0, 1)),
    subject_all INTEGER NOT NULL CHECK (subject_all IN (0, 1)),
    CHECK (json_valid(capabilities_json) AND json_valid(context_ids_json))
) WITHOUT ROWID;

-- Retention sweep (§6.10): the dedup window is a range predicate on the row's
-- age across every namespace.
CREATE INDEX IF NOT EXISTS idx_idempotency_namespace_created
    ON idempotency_namespace (created_at);

CREATE INDEX IF NOT EXISTS idx_receipts_space_status
    ON receipts (space_id, status);

-- §7.0.7: opaque pagination cursors. A cursor is bound to the principal, the
-- operation and a digest of the selection/filter plus the state marker it was
-- issued against, so any grant or state change invalidates it instead of
-- silently resuming over a different result set.
CREATE TABLE IF NOT EXISTS cursors (
    cursor_token TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    op TEXT NOT NULL,
    binding_digest TEXT NOT NULL,
    state_marker TEXT NOT NULL,
    created_at TEXT NOT NULL
) WITHOUT ROWID;
