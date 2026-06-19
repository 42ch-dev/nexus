-- V1.52 T-A P2: Work→KeyBlock provenance linkage (R-V150KBED-02)
-- Adds source_work_id, source_chapter, source_provenance_kind to kb_key_blocks
-- so author gates can prefer Work ownership over World ownership when
-- provenance is present.
--
-- entity-scope-model.md §5.5.7 (Draft V1.52)

ALTER TABLE kb_key_blocks ADD COLUMN source_work_id TEXT;

ALTER TABLE kb_key_blocks ADD COLUMN source_chapter INTEGER;

ALTER TABLE kb_key_blocks ADD COLUMN source_provenance_kind TEXT
    CHECK (source_provenance_kind IS NULL
        OR source_provenance_kind IN (
            'manual',
            'review_time_extract',
            'finalize_time_extract',
            'cross_chapter_rescan',
            'author_explicit'
        ));

CREATE INDEX IF NOT EXISTS idx_kb_key_blocks_source_work_id
    ON kb_key_blocks (source_work_id);
