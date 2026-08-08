-- V1.155 P0 (N-C3 multi-host production) — peer bookkeeping for the
-- `list_peer_host_capability_manifests` adapter port (DF-72).
--
-- Peers are recorded ONLY from observed Connect sessions (outbound-dialed
-- peers with a manifest-backed observation; iteration spec §Design lock #1):
-- a row is created when this host successfully dials a peer and reads its
-- `HostCapabilityManifest`. `host_id` is the peer manifest's device id — the
-- honest identity key (a libp2p `PeerId` is not stable per host installation,
-- lock #3).
--
-- Column notes:
--   - `manifest_json`: the serialized spoke `HostCapabilityManifest`, opaque
--     at the storage layer (`nexus-local-db` has no spoke dependency — spec
--     §8 dep-graph reversal); validated as JSON before insert (fail-closed).
--   - `last_seen`: RFC 3339 UTC timestamp of the observation.
--   - `capabilities`: denormalized JSON array (DEFAULT '[]') for query
--     without parsing `manifest_json`; populated by the adapter layer.
CREATE TABLE peer_hosts (
  host_id        TEXT PRIMARY KEY,
  manifest_json  TEXT NOT NULL,
  last_seen      TEXT NOT NULL,
  capabilities   TEXT NOT NULL DEFAULT '[]'
);
