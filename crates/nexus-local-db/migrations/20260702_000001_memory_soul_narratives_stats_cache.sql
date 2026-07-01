-- V1.81 QC3: fingerprint-cached distinct-keyword count.
--
-- Adds stats_fingerprint and distinct_keyword_count_cache to
-- memory_soul_narratives so the read path can skip keyword JSON
-- decoding when fragments haven't changed since the last compute.
--
-- Fingerprint format: "{fragment_count}:{max_created_at}" (cheap
-- SQL aggregates). When the fingerprint matches, the cached
-- distinct_keyword_count is returned directly — no streaming scan.
--
-- Existing rows get NULL fingerprint → forces one recompute on
-- next read; after that the cache is warm.

ALTER TABLE memory_soul_narratives ADD COLUMN distinct_keyword_count_cache INTEGER NOT NULL DEFAULT 0;
ALTER TABLE memory_soul_narratives ADD COLUMN stats_fingerprint TEXT;
