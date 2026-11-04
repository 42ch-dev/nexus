-- v1.229 N3 P4 (binding spec §5.1, D18): content-free Nexus dependency
-- metadata for remote binding-local memory.
--
-- The owner selected conservative fail-closed continuation: an earlier remote
-- attempt that cannot be proved settled may keep its binding non-removable
-- indefinitely. These tables carry only the identity of that dependency, never
-- memory text, request bodies, credentials, seeds, tokens, raw invokes or
-- payload digests.
--
--   remote_binding_memory_admissions  one row per actual Core-admitted wire
--       attempt. Each attempt (including a redelivery of the same pending_id)
--       gets its own admission; it is committed in the same short Nexus writer
--       transaction as the provenance check and before the RPC. Only the
--       definite result of that exact attempt may retire it. Cancellation,
--       transport/commit uncertainty or local outcome-persistence failure
--       leaves it, and no later attempt, receipt or shared counter may clear
--       another admission.
--
--   remote_binding_memory_objects  monotone known provider-object facts.
--       `state` moves live → consumed → released and never back, so an
--       acknowledgement that arrives before the capture result is published
--       (a dismissal or review of an already-known candidate) is durable and a
--       later result publication cannot resurrect a consumed candidate or
--       overwrite review-created record references. Only `live` facts block
--       binding removal; terminal facts are evidence the final removal
--       transaction clears with the binding.
--
-- Namespace: provider authority host/generation, exact space, the loaded
-- public client identity (Ed25519 public peer id, not a credential id or a
-- payload principal) and the Nexus Character/binding. A stale or foreign
-- namespace can neither read nor erase another namespace's facts.

CREATE TABLE IF NOT EXISTS remote_binding_memory_admissions (
    admission_id TEXT NOT NULL PRIMARY KEY
        CHECK (length(admission_id) BETWEEN 1 AND 64),
    owner_creator_id TEXT NOT NULL,
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    binding_id TEXT NOT NULL
        REFERENCES actor_world_bindings (binding_id) ON DELETE RESTRICT,
    authority_host_id TEXT NOT NULL,
    authority_generation INTEGER NOT NULL
        CHECK (authority_generation >= 1),
    space_id TEXT NOT NULL,
    client_identity TEXT NOT NULL
        CHECK (length(client_identity) BETWEEN 1 AND 256),
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_remote_binding_memory_admissions_binding
    ON remote_binding_memory_admissions (binding_id);

CREATE TABLE IF NOT EXISTS remote_binding_memory_objects (
    authority_host_id TEXT NOT NULL,
    authority_generation INTEGER NOT NULL
        CHECK (authority_generation >= 1),
    space_id TEXT NOT NULL,
    client_identity TEXT NOT NULL
        CHECK (length(client_identity) BETWEEN 1 AND 256),
    character_id TEXT NOT NULL
        REFERENCES characters (character_id) ON DELETE RESTRICT,
    binding_id TEXT NOT NULL
        REFERENCES actor_world_bindings (binding_id) ON DELETE RESTRICT,
    object_kind TEXT NOT NULL
        CHECK (object_kind IN ('candidate', 'record')),
    object_id TEXT NOT NULL,
    state TEXT NOT NULL
        CHECK (state IN ('live', 'consumed', 'released')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (
        authority_host_id,
        authority_generation,
        space_id,
        client_identity,
        character_id,
        binding_id,
        object_kind,
        object_id
    )
);

-- The removal gate asks for this binding's live references; the terminal
-- cleanup deletes by binding alone.
CREATE INDEX IF NOT EXISTS idx_remote_binding_memory_objects_binding_state
    ON remote_binding_memory_objects (binding_id, state);
