-- V1.50 T-A P3 (T1): per-Work auto-chronology opt-in flag.
-- Spec: .mstar/knowledge/specs/novel-writing/auto-chronology.md §2.1.
--
-- Adds `works.auto_chronology BOOLEAN NOT NULL DEFAULT 0`. When true, the
-- daemon `auto_chronology_tick` task (5-min interval) checks the Work for
-- finish detection and, if eligible, auto-creates the next volume outline +
-- seeds chapter rows + writes a chronology log entry (spec §3 / §4).
--
-- Default 0 (false): no Work opts in until the author explicitly runs
-- `creator works chronology set <work> --auto true`. This preserves the
-- existing V1.42 manual-only advancement behavior for every shipped Work.

ALTER TABLE works ADD COLUMN auto_chronology BOOLEAN NOT NULL DEFAULT 0;
