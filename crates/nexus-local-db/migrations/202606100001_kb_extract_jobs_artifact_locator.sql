-- V1.40 P3: Add artifact locator fields to kb_extract_jobs.
--
-- Extends the V1.29-era schema to support artifact-based extraction
-- (chapter body, screenplay scene, essay section, etc.) instead of
-- work-entry-only semantics.
--
-- V1.40 ships: source_kind='work_chapter', profile_hint='novel' only.
-- Schema and CLI accept other kinds as reserved values.

-- Artifact type discriminator (work_chapter | work_section | work_artifact | reference_doc).
ALTER TABLE kb_extract_jobs ADD COLUMN source_kind TEXT;

-- Artifact locator: relative path within work tree, artifact ID, or reference ID.
ALTER TABLE kb_extract_jobs ADD COLUMN source_locator TEXT;

-- Profile hint selects extract prompt template (novel | screenplay | essay | generic).
ALTER TABLE kb_extract_jobs ADD COLUMN profile_hint TEXT;

-- Work ID for the source work (chapter's parent work).
ALTER TABLE kb_extract_jobs ADD COLUMN work_id TEXT;
