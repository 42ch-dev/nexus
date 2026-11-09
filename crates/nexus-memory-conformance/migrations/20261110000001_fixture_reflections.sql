-- Independent fixture memory provider: reflections (spoke-memory §7.25/§7.26).
--
-- Extends the fixture-owned schema of 20261104000001_fixture_memory.sql. This
-- table belongs to the conformance fixture alone; it shares nothing with the
-- Nexus local provider and the fixture never imports Nexus storage code.
--
-- Reflection rows are immutable and never deleted: the pinned selection,
-- filter, basis and body are the audit trail of what was generated (MEM-R026
-- cursors and C-077 tombstone exclusion are read-time decisions over these
-- rows, not rewrites of them).
--
-- JSON columns keep the protocol's published shapes as opaque payloads: the
-- fixture validates them in its operation slices, never by schema enums. Absent
-- optional filters store NULL and round-trip as absent — never a null value.
CREATE TABLE IF NOT EXISTS reflections (
    reflection_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    requester_principal_id TEXT NOT NULL,
    selection_json TEXT NOT NULL,
    filter_json TEXT,
    cognition_filter_json TEXT,
    basis_json TEXT NOT NULL,
    body_text TEXT NOT NULL,
    body_format TEXT NOT NULL CHECK (body_format IN ('markdown', 'json')),
    cost_unit_kind TEXT,
    cost_units INTEGER,
    engine_policy_version TEXT NOT NULL,
    model TEXT,
    schema_version INTEGER NOT NULL CHECK (schema_version >= 1),
    revision INTEGER NOT NULL CHECK (revision >= 1),
    extensions_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    -- Cost is all-or-nothing: one unit kind with its unit count.
    CHECK ((cost_unit_kind IS NULL) = (cost_units IS NULL)),
    CHECK (json_valid(selection_json) AND json_valid(basis_json) AND json_valid(extensions_json))
) WITHOUT ROWID;

-- Paging order for list_reflections (oldest first within a subject).
CREATE INDEX IF NOT EXISTS idx_reflections_subject_created
    ON reflections (space_id, subject_id, created_at, reflection_id);

-- Direct lookup of one reflection within its subject.
CREATE INDEX IF NOT EXISTS idx_reflections_subject_reflection
    ON reflections (space_id, subject_id, reflection_id);

-- Generation liveness (§7.0.3, inspector round 2): a *pending* generate
-- receipt carries the provider instance that is running its generation and the
-- instant its lease expires, so the reopen recovery terminalizes only crash
-- orphans (lease expired) and never a generation another instance is executing
-- right now (lease live). Both stay NULL on terminal and non-generate receipts.
ALTER TABLE receipts ADD COLUMN owner_id TEXT;
ALTER TABLE receipts ADD COLUMN lease_expires_at TEXT;
