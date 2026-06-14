//! Hermetic supervisor E2E for the `research` preset (V1.46 P3, R-V139P5-S1).
//!
//! Proves that `ScheduleSupervisor` drives a research preset schedule row
//! through the boot → tick → terminal-done lifecycle **without** network,
//! live ACP, or live LLM calls. The ACP-dependent preset state machine
//! (scanning → extracting → synthesizing, gated by `llm_judge` exits) is
//! stubbed at its boundary: instead of running the real engine, we call
//! `on_schedule_terminal(Completed)` directly — exactly the hook the live
//! engine would invoke once the `synthesizing` `llm_judge` returns GO.
//!
//! ## Scope (R-V139P5-S1)
//! - Research preset loads hermetically from embedded-presets and validates.
//! - Research preset **input contract** (`preset.input.references_dir`,
//!   `preset.input.output_dir`) is documented; the schedule-row seed
//!   (`work_id`, `fl_e_stage`, `creative_brief`, `inspiration_log`,
//!   `work_ref`) is asserted via `build_schedule_for_stage("research", …)`.
//! - Supervisor `tick()` admits a pending research schedule (boot path).
//! - `on_schedule_terminal(Completed)` flips the row to the terminal status.
//! - Boot resume path (`resume_running_as_paused` → `resume_schedule`)
//!   recovers a running research schedule after a simulated daemon restart.
//!
//! ## Out of scope (tracked separately)
//! - R-V139P5-S5: artifact E2E requiring an ACP mock — explicitly deferred.
//! - Live preset state-machine execution (needs ACP/LLM).

use nexus_contracts::local::orchestration::preset::{ExitWhen, PresetKind, RunIntent};
use nexus_contracts::local::orchestration::preset_gate::{Gate, GateOp};
use nexus_contracts::local::schedule::ScheduleStatus;
use nexus_local_db::works::{self, WorkRecord};
use nexus_orchestration::auto_chain;
use nexus_orchestration::capability::CapabilityRegistry;
use nexus_orchestration::preset::load_embedded_preset;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use nexus_orchestration::stage_gates::{build_schedule_for_stage, WorkFields};
use sqlx::SqlitePool;
use std::sync::Arc;

// ── Hermetic DB helpers (modeled on tests/auto_chain.rs) ─────────────────────

/// Open an in-memory-tempfile `SQLite` pool with migrations applied.
///
/// The tempfile is intentionally leaked (`std::mem::forget`) so the file
/// survives for the test lifetime — matches the pattern in `auto_chain.rs`.
async fn test_pool() -> SqlitePool {
    let db = tempfile::Builder::new()
        .prefix("research_sup_e2e_")
        .suffix(".db")
        .tempfile()
        .unwrap();
    let db_path = db.path().to_path_buf();
    std::mem::forget(db);

    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    pool
}

/// Build a Work record whose fields satisfy the research preset gates
/// (`intake_status == complete`, `work_ref` required) and carry a creative
/// brief the preset input contract consumes.
fn research_ready_work(work_id: &str, work_ref: &str) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: "ctr_research_e2e".to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Research-Ready Work".to_string(),
        long_term_goal: "Produce a research report".to_string(),
        initial_idea: "Survey the reference corpus".to_string(),
        creative_brief: Some(
            r#"{"topic":"AI consciousness","scope":"literature survey"}"#.to_string(),
        ),
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: r#"[{"note":"philosophy of mind angle"}]"#.to_string(),
        primary_preset_id: "research".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-15T10:00:00Z".to_string(),
        updated_at: "2026-06-15T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_profile: None,
        work_ref: Some(work_ref.to_string()),
        total_planned_chapters: None,
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// Seed a Work row.
async fn seed_work(pool: &SqlitePool, work: &WorkRecord) {
    works::create_work(pool, work).await.unwrap();
}

/// Insert a research schedule row directly (bypasses `insert_pending`
/// validation so the test controls `work_id` + status precisely).
///
/// Mirrors `insert_driver_schedule` in `tests/auto_chain.rs`, pinned to the
/// research preset (`preset_version = 2`, matching the embedded manifest).
async fn insert_research_schedule(
    pool: &SqlitePool,
    schedule_id: &str,
    creator_id: &str,
    status: &str,
    work_id: &str,
) {
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper inserting a research schedule row with
    // the preset_version that matches embedded-presets/research/preset.yaml.
    sqlx::query(
        r"INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version,
            label, created_at, updated_at, work_id)
           VALUES (?, ?, 'research', 2, ?, 'serial', 0, ?, ?, ?, ?)",
    )
    .bind(schedule_id)
    .bind(creator_id)
    .bind(status)
    .bind(format!("FL-E stage: research (work: {work_id})"))
    .bind(now)
    .bind(now)
    .bind(work_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Read a schedule row's status straight from the DB.
async fn schedule_status(pool: &SqlitePool, schedule_id: &str) -> String {
    // SAFETY: test-only scalar lookup.
    sqlx::query_scalar::<_, String>("SELECT status FROM creator_schedules WHERE schedule_id = ?")
        .bind(schedule_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Build the `WorkFields` used to construct a research stage schedule request.
fn research_work_fields(work_id: &str, work_ref: &str) -> WorkFields {
    WorkFields {
        work_id: work_id.to_string(),
        fl_e_stage: "research".to_string(),
        creative_brief: r#"{"topic":"AI consciousness"}"#.to_string(),
        inspiration_log: "[]".to_string(),
        work_ref: Some(work_ref.to_string()),
        chapter: None,
        chapter_label: None,
        outline_path: None,
        body_path: None,
        slug: None,
        research_artifacts_dir: None,
        workspace_dir: None,
        world_kb_block: None,
        world_id: None,
        volume: None,
    }
}

// ── T1: research preset loads hermetically + structure validates ────────────

/// The embedded `research` preset must load against the builtin capability
/// registry and expose the documented state machine + gate contract.
///
/// This is the hermetic entry check: no network, no ACP — the loader only
/// resolves capability *names* against the registry; it never invokes them.
#[tokio::test]
async fn research_preset_loads_and_structurally_valid() {
    let caps = CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset("research", &caps).expect("research preset must load");

    // Header contract.
    assert_eq!(loaded.id, "research", "preset id");
    assert_eq!(
        loaded.version, 2,
        "preset version (bump on breaking changes)"
    );
    assert_eq!(
        loaded.manifest.preset.kind,
        PresetKind::Creator,
        "research is a creator-facing preset",
    );

    // Initial → terminal wiring.
    assert_eq!(loaded.manifest.preset.initial, "scanning", "initial state");
    assert_eq!(loaded.manifest.preset.terminal, "done", "terminal state");

    // Capabilities required by the research workflow are all registered.
    let required = &loaded.manifest.preset.requires_capabilities;
    assert!(
        required.contains(&"creator.inject_prompt".to_string()),
        "research requires creator.inject_prompt (scanning + synthesizing enter)",
    );
    assert!(
        required.contains(&"acp.prompt".to_string()),
        "research requires acp.prompt (extraction inner graph)",
    );
    assert!(
        required.contains(&"judge.llm".to_string()),
        "research requires judge.llm (scanning + synthesizing llm_judge exits)",
    );

    // Run intents (drive daemon context-assembly strategy).
    let intents = &loaded.manifest.preset.run_intents;
    assert!(
        intents.contains(&RunIntent::KnowledgeIngest),
        "research declares knowledge_ingest intent",
    );
    assert!(
        intents.contains(&RunIntent::WorkContinue),
        "research declares work_continue intent",
    );

    // State machine: scanning → extracting → synthesizing → done.
    let state_ids: Vec<&str> = loaded
        .manifest
        .states
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(
        state_ids,
        vec!["scanning", "extracting", "synthesizing", "done"],
        "research state machine must be the documented 4-state chain",
    );

    // Exit-when contract: llm_judge bookends the workflow (scanning + synthesizing),
    // graph_complete gates the inner extraction graph. This is the contract the
    // hermetic lifecycle test below stubs at its boundary.
    let scanning = &loaded.manifest.states[0];
    assert!(
        matches!(scanning.exit_when, Some(ExitWhen::LlmJudge { .. })),
        "scanning exit must be llm_judge",
    );
    let extracting = &loaded.manifest.states[1];
    assert!(
        matches!(extracting.exit_when, Some(ExitWhen::GraphComplete)),
        "extracting exit must be graph_complete",
    );
    let synthesizing = &loaded.manifest.states[2];
    assert!(
        matches!(synthesizing.exit_when, Some(ExitWhen::LlmJudge { .. })),
        "synthesizing exit must be llm_judge (where the hermetic stub boundary is)",
    );
    assert!(loaded.manifest.states[3].terminal, "done is terminal");

    // Preset gates — evaluated at enqueue time; the test Work below satisfies them.
    // Asserted via typed pattern matching on the generated `Gate` / `GateOp` wire
    // types (Debug formatting is not a stable contract; see qc3 W-1). The two
    // semantic checks below must hold:
    //   - one `WorkField { field: "intake_status", op: Equals("complete") }`
    //   - one `WorkField { field: "work_ref", op: Required }`
    let gates = &loaded.manifest.preset.gates;
    let mut has_intake_status_complete = false;
    let mut has_work_ref_required = false;
    for g in gates {
        match g {
            Gate::WorkField {
                field,
                op: GateOp::Equals { value },
            } if field.as_str() == "intake_status" && value.as_str() == Some("complete") => {
                has_intake_status_complete = true;
            }
            Gate::WorkField {
                field,
                op: GateOp::Required,
            } if field.as_str() == "work_ref" => {
                has_work_ref_required = true;
            }
            _ => {}
        }
    }
    assert!(
        has_intake_status_complete,
        "research gate: intake_status == \"complete\"",
    );
    assert!(has_work_ref_required, "research gate: work_ref required");
}

// ── T1: research schedule request + preset input contract ───────────────────

/// `build_schedule_for_stage("research", …)` produces the canonical schedule
/// request. The preset-input seed carries the Work fields the research
/// prompts consume via `{{preset.input.*}}`.
///
/// NOTE: the research prompt templates additionally consume
/// `{{preset.input.references_dir}}` (scanning) and
/// `{{preset.input.output_dir}}` (synthesizing). Those paths are resolved
/// from the Work's workspace at session-run time and are NOT part of the
/// schedule-row seed produced here; the assertions below document the
/// schedule-row contract, not the runtime-resolved paths.
#[tokio::test]
async fn research_schedule_request_preset_input_contract() {
    let fields = research_work_fields("wrk_contract", "ai-consciousness-survey");
    let req = build_schedule_for_stage("research", "ctr_research_e2e", &fields)
        .expect("research stage must map to a preset");

    // Preset + label contract (must match auto_chain enqueue_auto_chain_schedule).
    assert_eq!(req.preset_id, "research", "preset_id");
    assert_eq!(
        req.label.as_deref(),
        Some("FL-E stage: research (work: wrk_contract)"),
        "schedule label",
    );

    // Preset input seed (consumed via {{preset.input.*}} template vars).
    let input = req
        .input
        .as_ref()
        .expect("research schedule request must carry a preset input seed");
    assert_eq!(input["work_id"], "wrk_contract", "work_id seeded");
    assert_eq!(input["fl_e_stage"], "research", "fl_e_stage seeded");
    assert_eq!(
        input["work_ref"], "ai-consciousness-survey",
        "work_ref seeded (research gate requires it on the Work)",
    );
    assert!(
        input.get("creative_brief").is_some(),
        "creative_brief seeded (research scanning prompt consumes work context)",
    );
    assert!(
        input.get("inspiration_log").is_some(),
        "inspiration_log seeded",
    );

    // `references_dir` / `output_dir` are intentionally absent from the seed:
    // they are resolved at session-run time from the Work workspace, not
    // stored on the schedule row. This documents the boundary between the
    // hermetic schedule-row contract and the runtime ACP-dependent paths.
    assert!(
        input.get("references_dir").is_none() && input.get("output_dir").is_none(),
        "references_dir/output_dir are runtime-resolved, not schedule-row seed fields",
    );
}

// ── T2: supervisor tick drives research schedule boot → running ─────────────

/// Boot admission: a pending research schedule is admitted by `tick()` and
/// transitions to `running`. No live ACP is invoked — `tick()` only touches
/// the DB and the admission gate (pure function over running/completed sets).
#[tokio::test]
async fn research_supervisor_tick_admits_pending_schedule() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Work satisfies the research gates (intake complete, work_ref set).
    let work = research_ready_work("wrk_boot", "boot-survey");
    seed_work(&pool, &work).await;

    // Pending research schedule wired to the work via work_id.
    insert_research_schedule(
        &pool,
        "sch_research_boot_001",
        "ctr_research_e2e",
        "pending",
        "wrk_boot",
    )
    .await;
    assert_eq!(
        schedule_status(&pool, "sch_research_boot_001").await,
        "pending",
        "precondition: schedule starts pending",
    );

    // Boot admission.
    sup.tick()
        .await
        .expect("tick must admit the research schedule");

    assert_eq!(
        schedule_status(&pool, "sch_research_boot_001").await,
        "running",
        "tick() must transition pending research schedule to running",
    );
    assert_eq!(
        sup.status_of("sch_research_boot_001").await.unwrap(),
        ScheduleStatus::Running,
        "supervisor status_of agrees with the DB row",
    );
}

// ── T2: headline E2E — boot → tick → (stubbed synthesizing) → terminal done ─

/// Headline hermetic E2E: the supervisor drives a research schedule row from
/// boot admission all the way to terminal `done`, **without** live ACP.
///
/// In a live system the flow after `tick()` admits the schedule is:
///   1. Engine runs the preset state machine: `scanning` → `extracting` →
///      `synthesizing`.
///   2. `llm_judge` gates the `scanning` and `synthesizing` exits (calls the
///      configured `judge.llm` capability, which needs a live LLM/ACP).
///   3. On `synthesizing` GO, the engine reaches `done` and the daemon calls
///      `on_schedule_terminal(schedule, Completed)`.
///
/// Hermetic stub boundary: we skip steps 1–3 (they are the ACP/LLM-dependent
/// surface tracked separately under R-V139P5-S5) and invoke
/// `on_schedule_terminal(Completed)` directly. This proves the
/// supervisor-owned lifecycle — boot admission, terminal transition, runtime
/// lock release — is correct for the research preset end to end.
#[tokio::test]
async fn research_supervisor_tick_drives_boot_to_terminal_done() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    // Seed a research-ready Work and wire the schedule as its driver so the
    // terminal hook exercises the real `find_work_for_driver` path.
    let mut work = research_ready_work("wrk_e2e", "e2e-survey");
    work.current_stage = "research".to_string();
    work.stage_status = "active".to_string();
    seed_work(&pool, &work).await;

    insert_research_schedule(
        &pool,
        "sch_research_e2e_001",
        "ctr_research_e2e",
        "pending",
        "wrk_e2e",
    )
    .await;
    auto_chain::set_driver(
        &pool,
        "ctr_research_e2e",
        "wrk_e2e",
        "sch_research_e2e_001",
        "research",
    )
    .await
    .unwrap();

    // ── Boot: tick admits the pending research schedule. ───────────────────
    sup.tick().await.expect("boot tick admits research");
    assert_eq!(
        schedule_status(&pool, "sch_research_e2e_001").await,
        "running",
        "boot: research schedule is running",
    );

    // ── Stubbed preset execution boundary ──────────────────────────────────
    // Live engine would drive scanning → extracting → synthesizing here,
    // gated by llm_judge. We skip straight to the terminal hook the daemon
    // invokes once the synthesizing llm_judge returns GO. No ACP is called.
    sup.on_schedule_terminal("sch_research_e2e_001", ScheduleStatus::Completed)
        .await
        .expect("terminal transition to Completed must succeed");

    // ── Terminal: schedule row reached `done`. ─────────────────────────────
    assert_eq!(
        schedule_status(&pool, "sch_research_e2e_001").await,
        "completed",
        "research schedule must reach terminal status `completed` (= preset `done`)",
    );
    assert_eq!(
        sup.status_of("sch_research_e2e_001").await.unwrap(),
        ScheduleStatus::Completed,
        "supervisor reports the research schedule as Completed",
    );

    // The terminated row carries a `terminated_at` timestamp — proves the
    // terminal path ran the full UPDATE, not just a status flag flip.
    let terminated_at: Option<i64> =
        sqlx::query_scalar("SELECT terminated_at FROM creator_schedules WHERE schedule_id = ?")
            .bind("sch_research_e2e_001")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        terminated_at.is_some(),
        "terminal transition must stamp terminated_at",
    );
}

// ── T2: boot resume path (daemon restart recovery for research) ─────────────

/// Boot resume path: a research schedule that was `running` when the daemon
/// crashed is recovered as `paused` on boot, then smart-resumed back to
/// `running` once the gates still pass. This is the second leg of
/// R-V139P5-S1 ("boot resume paths").
#[tokio::test]
async fn research_schedule_boot_resume_running_to_paused_to_resumed() {
    let pool = test_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(Arc::new(pool.clone())));

    let work = research_ready_work("wrk_resume", "resume-survey");
    seed_work(&pool, &work).await;

    // Simulate a pre-crash state: the research schedule was mid-flight.
    insert_research_schedule(
        &pool,
        "sch_research_resume_001",
        "ctr_research_e2e",
        "running",
        "wrk_resume",
    )
    .await;
    assert_eq!(
        schedule_status(&pool, "sch_research_resume_001").await,
        "running",
        "precondition: schedule was running before the crash",
    );

    // ── Daemon boot: running → paused. ─────────────────────────────────────
    let paused_count = sup
        .resume_running_as_paused("daemon_restart_research_e2e")
        .await
        .unwrap();
    assert_eq!(
        paused_count, 1,
        "exactly one running research schedule must be paused on boot",
    );
    assert_eq!(
        schedule_status(&pool, "sch_research_resume_001").await,
        "paused",
        "boot recovery: research schedule is paused",
    );

    // Idempotent: a second boot-recovery pass is a no-op.
    let again = sup
        .resume_running_as_paused("daemon_restart_research_e2e")
        .await
        .unwrap();
    assert_eq!(again, 0, "second boot-recovery pass finds nothing to pause");

    // ── Smart resume: paused → running (gates still satisfied). ────────────
    let new_status = sup
        .resume_schedule("sch_research_resume_001")
        .await
        .expect("smart resume must succeed for a research schedule whose gates still pass");
    assert_eq!(
        new_status, "running",
        "smart resume transitions the research schedule straight back to running",
    );
    assert_eq!(
        schedule_status(&pool, "sch_research_resume_001").await,
        "running",
        "DB row agrees with the supervisor after smart resume",
    );
}
