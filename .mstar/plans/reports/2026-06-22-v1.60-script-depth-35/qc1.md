---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.60-script-depth-35"
verdict: "Approve"
generated_at: "2026-06-23"
---

# QC1 Architecture/Maintainability Review — V1.60 P1 Script Depth 3.5

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: architecture coherence & maintainability risk
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-22-v1.60-script-depth-35
- Review range / Diff basis: `a45e5b8f..e13a06fb` (P1 Track B: 5 commits)
- Working branch (verified): iteration/v1.60
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9 (preset + 5 prompts + spec extension + quality_loop + work_chapters + works + lib + migration)
- Commit range: a45e5b8f..e13a06fb
- Tools run: git diff, git log, grep, read on key sources

## Findings

### 🔴 Critical
_(none)_

### 🟡 Warning

#### W-001: `script.section_status.update` capability referenced in spec and preset but not shipped in `CapabilityRegistry`
**Scope**: `.mstar/knowledge/specs/script-profile.md:198` (§5.1 "Capabilities required"), `:323` (§9.2 "Production Preset"), `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml:9` (header comment) and `:32` (referenced in the design-writing-style capability list)
**Finding**: Three normative documents (the updated Draft spec, the preset header, and the embedded preset's `requires_capabilities` discussion) name `script.section_status.update` as the capability that auto-transitions `section_status` from `draft → reviewed` after a GO review pass. The capability does not exist in `CapabilityRegistry::with_builtins()` — only the 5 P0 DF-46 orchestration capabilities were added in this iteration; the registry count moved 26 → 31 (P0) and stays 31 after P1 (no new orchestration capabilities). The P1 plan's own `requires_capabilities` for `script-writing` (preset.yaml lines 32-34) correctly omits `script.section_status.update`, listing only `creator.inject_prompt`, `acp.prompt`, `judge.llm` — so the preset will load and run, but the spec-authoring team and the preset header comment promise a capability the runtime does not have. The intended resolution (per the QC1 assignment brief) is V1.61, but: (a) the plan's "Out of scope" section does not name this deferral, (b) the plan's acceptance criteria do not require a residual entry, and (c) the spec sections that mention the capability are written in present-tense as if it exists. A maintainer in V1.61 will have to cross-reference the registry and the spec side-by-side to discover the drift.
**Recommendation**: Before merge: register a residual (severity `medium`, source `qc1.md`, decision `defer` to V1.61, owner TBD) explicitly stating "V1.61: ship `script.section_status.update` orchestration capability per spec §5.1 + §9.2. Until shipped, the `draft → reviewed` transition must be invoked manually or via the preset author's CLI workflow." Optionally soft-edit the spec to mark the capability as `Draft — deferred to V1.61` so the next reader does not infer it's shipped.

### 🟢 Suggestion

#### S-001: Every state in `script-writing` preset uses the same `finalize-exit.md` template — unusual but well-justified
**Scope**: `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml:69-72, 88-91, 106-109, 124-127`
**Finding**: All 4 non-terminal states (`outline`, `draft`, `revise`, `finalize`) declare `template_file: prompts/finalize-exit.md` for their `exit_when` LLM judge. Compare to `design-writing` (`embedded-presets/design-writing/preset.yaml:76-77, 84-85`) which also uses a single `design-review-exit.md` template — so this is a consistent pattern across Depth 3.5 profiles, not a one-off deviation. The 五问 rubric is broad enough (dialogue coherence, beat pacing, act structure, character voice, scene economy) to be applicable at every stage. The architectural intent appears to be: every state must pass 五问 to advance, not just the terminal one. If that intent is correct, it is a deliberate simplification. If it is incidental (the author meant only `finalize` to be 五问-gated), the design is ambiguous and would cause every state transition to be reviewed against the full rubric, which may over-constrain the LLM judge.
**Recommendation**: Add a one-line comment in `preset.yaml` near the first `exit_when` block clarifying: "All states exit on 五问 (script rubric); final state additionally enforces KB extraction." This makes the per-state intent explicit so a future author who adds an intermediate state understands whether it should also be 五问-gated or not.

#### S-002: `is_script_complete` is only invoked from `get_work` — completion is observed lazily, not evaluated on a schedule
**Scope**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:776-825` (auto-promote block); `crates/nexus-local-db/src/work_chapters.rs:1446-1530` (`is_script_complete`)
**Finding**: The auto-promotion path mirrors the game-bible pattern (same `get_work` handler, lines 723-774 for game_bible). This is consistent and avoids duplicated work, but the `is_work_completed` short-circuit in `work_chapters.rs:1236` means `is_work_completed` **always** returns `false` for script Works, even when both critical sections are `accepted`. Any consumer that polls `is_work_completed` (e.g. a scheduler or progress dashboard) will report script Works as never complete until something calls `get_work`. For a long-running Work, completion may be delayed indefinitely. The game-bible pattern has the same issue, so this is not a regression — but it is worth noting that this is the V1.60-P1 acceptance-criteria-defined behavior.
**Recommendation**: V1.61 hygiene: add a daemon-side scheduler tick that calls `is_script_complete` (and `is_game_bible_design_complete`) for in-flight Works, similar to the auto-chain resume scheduler. Document the current lazy-observation behavior in the spec section "Completion guardian" so authors are not surprised.

#### S-003: `block_type_to_script_category` silently defaults unknown `block_type`s to `dialogue` (most generic category)
**Scope**: `crates/nexus-orchestration/src/quality_loop.rs:910-912` (default arm); tests at `quality_loop.rs:2052-2063`
**Finding**: The unknown-block_type fallback emits a `tracing::debug!` but the extracted candidate still lands in `attributes.script_category = "dialogue"`. The `ValidationMode::Script` validation downstream may accept "dialogue" as valid (per the spec, `dialogue|beat|act` are the three valid categories), so the unknown BlockType is silently classified as a dialogue entry rather than rejected or held for review. This is a "fail-soft" design that mirrors the `block_type_to_game_bible_category` and `block_type_to_novel_category` fallbacks — same pattern, consistent with the existing behavior. The tracing debug makes it visible to operators, which is the right visibility hook.
**Recommendation**: V1.61+ consider promoting the unknown-block_type default from `dialogue` to a separate `unknown_review` sentinel category that downstream validation rejects. For now, the fail-soft is consistent with the existing cross-domain mapping pattern.

#### S-004: `script-writing` is registered as version `1` in `preset_version_for_id`; bump versioning discipline is implicit
**Scope**: `crates/nexus-orchestration/src/auto_chain.rs:1481-1482` (`"script-writing" => 1`)
**Finding**: The `preset_version_for_id` SSOT extension is correct, and the sync test (`preset_version_mapping_matches_yaml_includes_cron_presets`) at `auto_chain.rs:2162-2166` includes `"script-writing"` in the `known_ids` array. Both are aligned with `preset.yaml` (`version: 1` on line 28). The pattern closes R-V150P1CRONBW-01 for the third non-novel profile, mirroring the game-bible extension. However, the SSOT has no machine-checked link to the YAML — a future author who bumps `preset.yaml`'s `version` field without also updating `preset_version_for_id` would not trip a build error (the sync test compares `mapping_version` against `preset_version_for_id`, not against the YAML's own version). The same caveat applies to `design-writing` and the novel presets — pre-existing pattern, not a P1 regression.
**Recommendation**: V1.61+ hygiene plan: extend `preset_version_mapping_matches_yaml_includes_cron_presets` to also read the YAML and assert `preset.yaml.version == preset_version_for_id(id)`. This would catch drift before merge.

## Cross-cutting Observations
- **Spec depth**: `script-profile.md` Depth 3.5 extension mirrors `game-bible-profile.md` §4 structure: preset chain, 五问 rubric, section completion, KB extraction contract are all present in the spec body and in the implementation. The 4 components are cleanly distinguished in §5.1, §5.2, §8, §7.1.
- **Preset structure**: `script-writing/preset.yaml` follows `design-writing/preset.yaml` patterns — `kind: creator`, `requires_capabilities`, `run_intents`, `initial`/`terminal`, `gates` (work_profile + work_ref + intake_status + filesystem), `states[]` with `enter` (creator.inject_prompt) + `exit_when` (llm_judge). Stage chain `outline → draft → revise → finalize → done` matches the spec. Template variables use the `{{preset.input.*}}` syntax.
- **Quality rubric**: The 5 script-specific 五问 dimensions are well-defined in `finalize-exit.md` (dialogue coherence, beat pacing, act structure, character voice, scene economy) with YES/NO criteria for each. Clear, script-domain-appropriate, mirrors the game-bible design rubric structure.
- **KB extraction**: `block_type_to_script_category` adds 13 typed mappings (3 direct + 10 cross-domain). The function is correctly invoked from `candidate_from_llm_json_for_profile` when `work_profile == "script"`. Output payload has `script_category` (not `novel_category` or `game_bible_category`) and `tags = ["script", "llm-extracted"]`. Tests at `quality_loop.rs:1960-2067` cover direct mappings, cross-domain mappings, and the unknown-block_type fallback. Adopt-time `ValidationMode::Script` will validate the proposed categories.
- **Section completion**: `is_script_complete` correctly mirrors `is_game_bible_design_complete` semantics — checks both critical sections (`Scripts/script.md` + `Beats/beat-sheet.md`), requires `intake_status == "complete"`, returns `false` for missing files or non-accepted `section_status`. 4 test vectors (all_accepted, one_draft, missing_files, intake_pending) cover the edge cases including partially-accepted acts.
- **Preset version SSOT**: `script-writing` is wired into both `preset_version_for_id` (returns `1`) and the sync test's `known_ids` array. Cross-track collision point per compass Q-risks; the integration merge (`a45e5b8f`) resolved cleanly.
- **DB migration**: `202606230001_work_profile_script.sql` extends the `works.work_profile` CHECK constraint to include `'script'` (alongside `'novel'`, `'essay'`, `'game_bible'`). Uses table-rebuild pattern (DROP/CREATE/RENAME + recreate indexes) because SQLite cannot ALTER a CHECK constraint in place. All 12 indexes are explicitly recreated. QC2 has separately flagged the missing `PRAGMA foreign_key_check` post-migration validation — that is QC2's concern (correctness/safety), but architecturally the table-rebuild is acceptable per SQLite best-practice when the CHECK must change.
- **Daemon auto-promote**: `works.rs:776-825` correctly mirrors the game_bible auto-promote pattern (lines 723-774). Same lazy-evaluation semantics: completion is only checked when `get_work` is called.
- **Profile helper**: `is_script_profile` is exported at `nexus_local_db::is_script_profile` and used as the gate in `is_work_completed`. Mirrors `is_novel_profile` and `is_game_bible_profile` cleanly.
- **Cross-track surface disjointness**: P1 touches `embedded-presets/script-writing/` (new), `quality_loop.rs` (script branch of `candidate_from_llm_json_for_profile` + `block_type_to_script_category`), `work_chapters.rs` (script profile gate + `is_script_complete`), `works.rs` (`is_script_profile` helper), `lib.rs` (re-exports), `daemon-runtime/.../works.rs` (auto-promote block), and one migration. P0 touched `capability/builtins/{world,timeline,fork}.rs` + spec + registry. No overlap except `preset_version_for_id` (compass-flagged, resolved cleanly).
- **Dead code**: None observed. Imports are used.
- **Naming**: New symbols (`is_script_complete`, `is_script_profile`, `block_type_to_script_category`) follow the existing per-profile helper naming convention.
- **Documentation**: Public APIs have doc-comments. `is_script_complete` has a 22-line doc-comment explaining the contract and edge cases; `block_type_to_script_category` documents the cross-domain mapping rationale.

## Source Trace
- Finding W-001: grep `script.section_status.update` returned 1 hit in `script-writing/preset.yaml:9` and 0 hits in `crates/nexus-orchestration/src/`. Spec lines 198, 323 explicitly list the capability. Confidence: High.
- Finding S-001: `preset.yaml` lines 69-72, 88-91, 106-109, 124-127 all reference `template_file: prompts/finalize-exit.md`. Confidence: High.
- Finding S-002: grep `is_script_complete` returned 1 caller (works.rs:782), no scheduler-side caller. The `is_work_completed` gate at work_chapters.rs:1236 returns `false` unconditionally for script profile. Confidence: High.
- Finding S-003: `quality_loop.rs:912` default arm returns `"dialogue"`; same pattern as `block_type_to_novel_category` / `block_type_to_game_bible_category`. Confidence: High.
- Finding S-004: `auto_chain.rs:1481-1482` defines `preset_version_for_id`; `preset.yaml:28` defines `version: 1`. The sync test does not compare against the YAML version. Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

**Rationale**: P1 ships a faithful Depth 3.5 promotion for the script profile: spec extension mirrors `game-bible-profile.md` §4, the `script-writing` preset follows the `design-writing` pattern (gates, stage chain, 五问 exit judge), KB extraction extends `candidate_from_llm_json_for_profile` with 13 typed mappings and 7 unit tests, and section completion (`is_script_complete`) cleanly mirrors `is_game_bible_design_complete`. The DB migration uses the accepted table-rebuild pattern for SQLite CHECK-constraint changes. The single Warning (W-001) is a contract drift: the spec and preset header reference `script.section_status.update` as a required capability, but that capability does not exist in `CapabilityRegistry` and is not in the plan's `requires_capabilities`. The intended resolution is V1.61; the gap is registered here so PM can record the deferral as a residual rather than leaving it to be discovered in a future iteration. The 4 Suggestions are non-blocking improvements (clarify per-state exit intent, document lazy completion observation, promote unknown-block_type handling, machine-check preset YAML version) that can ship in a V1.61 hygiene pass. **Plan is ready to merge to main** after PM records W-001 as a residual in `.mstar/status.json`.