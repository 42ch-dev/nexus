//! Direct-core CLI tests — `creator works findings accept` and
//! `creator works rules reset` write `Works/<work_ref>/AGENTS.md` under the
//! workspace's REGISTERED creative root (inspector BUG-01, v1.235 Phase 5).
//!
//! The Work artifact port pins the root the workspace registers in its
//! operational `meta.json` and refuses any other, so a handler that resolved
//! its `Works/` tree from the operational directory (`~/.nexus42/creators/
//! <id>/workspaces/<slug>`) had every write refused. The fixture therefore
//! registers a creative root that is deliberately NOT the operational
//! directory, and the assertions pin the bytes to it.

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{nexus_root_from_home, operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::findings::{self, Finding};
use nexus_local_db::works;
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// The Work directory name both commands write under.
const WORK_REF: &str = "rules-root-novel";
/// The rule suggestion the accepted finding carries.
const RULE: &str = "Always keep dialogue tags in the past tense.";

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .into_iter()
        .next()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

fn cli(fixture: &DirectFixture, args: &[&str]) -> Output {
    fixture
        .command()
        .args(args)
        .output()
        .expect("spawn nexus42")
}

fn describe(out: &Output) -> String {
    format!(
        "status: {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A hermetic home whose registered creative root is NOT the operational
/// workspace dir, with one Work (carrying `WORK_REF`) and one finding whose
/// `rule_suggestion` is [`RULE`].
struct RulesEnv {
    fixture: DirectFixture,
    op_dir: PathBuf,
    creative_root: PathBuf,
    work_id: String,
    finding_id: String,
}

/// Point the workspace's operational `meta.json` at `creative_root` — a tree
/// distinct from the operational directory, exactly like a workspace
/// registered on a user path. The core resolves every workspace file against
/// this `local_root`.
fn register_creative_root(op_dir: &Path, creator_id: &str, creative_root: &Path) {
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "creator_id": creator_id,
            "workspace_slug": WORKSPACE_SLUG,
            "local_root": creative_root,
            "created_at": "2020-01-01T00:00:00Z"
        }))
        .expect("meta json"),
    )
    .expect("write meta.json");
}

async fn fresh_env() -> RulesEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);
    let op_dir = operational_workspace_dir(fixture.home.path(), &creator_id, WORKSPACE_SLUG);
    let creative_root = fixture.home.path().join("creative");
    std::fs::create_dir_all(&creative_root).expect("materialize creative root");
    register_creative_root(&op_dir, &creator_id, &creative_root);

    // A Work binds to an owned World, so the World comes first.
    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
        memory: None,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({
                "title": "Rules Root World"
            }))
            .expect("world request shape"),
        )
        .await
        .expect("create world")
        .world_id;
    let work_id = core
        .create_work(
            &principal,
            CreateWorkRequest {
                client_request_id: None,
                initial_idea: "A test story".to_string(),
                lineage_from_work_id: None,
                long_term_goal: "Prove the rules write root".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(WORK_REF.to_string()),
                title: "Rules Root Novel".to_string(),
                work_profile: Some("novel".to_string()),
                world_id,
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");

    // The Work directory name a scaffold would claim, plus one open finding
    // carrying the rule suggestion `findings accept` appends.
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("workspace pool")
        .clone_pool();
    works::patch_work(
        &pool,
        &creator_id,
        &work_id,
        &works::WorkPatch {
            work_ref: Some(Some(WORK_REF.to_string())),
            ..works::WorkPatch::default()
        },
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .expect("claim work_ref");
    let finding_id = findings::mint_finding_id();
    let now = chrono::Utc::now().timestamp();
    findings::create_finding(
        &pool,
        &Finding {
            finding_id: finding_id.clone(),
            work_id: work_id.clone(),
            chapter: None,
            severity: "major".to_string(),
            status: "open".to_string(),
            title: "Past-tense tags".to_string(),
            description: "A test finding".to_string(),
            target_executor: "write".to_string(),
            creator_id,
            kind: "craft".to_string(),
            rule_suggestion: Some(RULE.to_string()),
            created_at: now,
            updated_at: now,
        },
    )
    .await
    .expect("seed finding");
    pool.close().await;
    // A CLI child must not be handed a home another writer still holds.
    release_retained_writer_guards(&db_path);

    RulesEnv {
        fixture,
        op_dir,
        creative_root,
        work_id,
        finding_id,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_writes_land_under_the_registered_creative_root() {
    let env = fresh_env().await;
    let agents_md = env
        .creative_root
        .join("Works")
        .join(WORK_REF)
        .join("AGENTS.md");

    // 1. `findings accept` appends the rule under the creative root.
    let out = cli(
        &env.fixture,
        &[
            "creator",
            "works",
            "findings",
            "accept",
            &env.finding_id,
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "findings accept refused: {}",
        describe(&out)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("accept prints json");
    assert_eq!(body["appended"], serde_json::Value::Bool(true));
    let written = std::fs::read_to_string(&agents_md).unwrap_or_else(|e| {
        panic!(
            "the accepted rule must land under the creative root ({}): {e}",
            agents_md.display()
        )
    });
    assert!(
        written.contains(RULE),
        "the accepted rule must be in the creative-root file: {written}"
    );
    assert!(
        !env.op_dir.join("Works").exists(),
        "nothing may land beside the operational workspace dir"
    );

    // 2. `rules reset` restores the scaffold in that same tree.
    let out = cli(
        &env.fixture,
        &[
            "creator",
            "works",
            "rules",
            "reset",
            &env.work_id,
            "--yes",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "rules reset refused: {}",
        describe(&out)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("reset prints json");
    // The executor canonicalizes its root, so the reported path carries the
    // resolved form (on macOS `/var` → `/private/var`).
    let reported = std::fs::canonicalize(&agents_md).expect("canonical agents.md path");
    assert_eq!(
        body["agents_md_path"],
        serde_json::Value::String(reported.to_string_lossy().into_owned()),
        "the reported path must be the creative-root file"
    );
    assert_eq!(
        std::fs::read_to_string(&agents_md).expect("reset rewrites the creative-root file"),
        nexus_orchestration::rules_layers::render_default_agents_md(WORK_REF),
        "reset must restore the default scaffold"
    );
    assert!(
        !env.op_dir.join("Works").exists(),
        "the reset must not create a tree beside the operational workspace dir"
    );
}
