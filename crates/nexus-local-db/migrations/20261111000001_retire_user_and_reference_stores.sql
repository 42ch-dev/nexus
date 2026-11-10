-- v231: Drop the retired User KnowledgeStore and reference-source registry.
--
-- ADR K1 (2026-11-10-retire-user-scope-and-references) §3.1–§3.2: both
-- datasets were inventoried and archived before removal (empty on this host;
-- see the v231 plan Task 1 archive). The tables are dropped, not sealed: no
-- runtime path may recreate them, and no trigger, index or outbox family
-- survives the drop.
--
-- Guarded so a host restored from an older backup replays idempotently.
DROP TABLE IF EXISTS knowledge_entries;
DROP TABLE IF EXISTS reference_sources;
