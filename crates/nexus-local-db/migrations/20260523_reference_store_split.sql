-- V1.26 reference store: registry + MD body split.
--
-- Adds `content_path` (relative pointer to body.md) and `source_mutability`
-- (static | refreshable) to `reference_sources`. Pre-1.0 wipe: existing
-- inline `content` is set to NULL since local persistence may be wiped
-- rather than migrated (compass §9).

ALTER TABLE reference_sources ADD COLUMN content_path TEXT;
ALTER TABLE reference_sources ADD COLUMN source_mutability TEXT NOT NULL DEFAULT 'static';

-- Pre-release wipe: null out legacy inline body content.
UPDATE reference_sources SET content = NULL;
