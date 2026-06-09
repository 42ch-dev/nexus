-- R-V139P1-W-1: Add CHECK constraints for findings enum columns.
-- These enforce valid values at the DB level, catching typos from
-- non-CLI callers (future API, tests) before they corrupt data.

-- severity: info | minor | major | blocker
ALTER TABLE findings ADD CONSTRAINT chk_findings_severity
    CHECK (severity IN ('info', 'minor', 'major', 'blocker'));

-- status: open | resolved | wont_fix
ALTER TABLE findings ADD CONSTRAINT chk_findings_status
    CHECK (status IN ('open', 'resolved', 'wont_fix'));

-- target_executor: write | brainstorm | none | master
ALTER TABLE findings ADD CONSTRAINT chk_findings_target_executor
    CHECK (target_executor IN ('write', 'brainstorm', 'none', 'master'));
