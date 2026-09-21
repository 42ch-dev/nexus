//! Direct-core CLI tests — `creator inspector` (V1.175 P1 Task 1, group 6;
//! direct-core retarget v1.193 P0-T8): hidden debug group; `moment` prints the
//! observe-only inspector packet / `--json` DTO against a hermetic
//! direct-core home — no daemon, no Node child (`common/direct.rs` precedent).
//! Also pins PL-6: absent from `creator --help`.

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_contracts::CreateWorkRequest;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{nexus_root_from_home, operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::Path;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// Deterministic `story_ref` for the seeded Work.
const STORY_REF: &str = "inspector-test-novel";
/// Lore row the packet must place (constant activation → always fires).
const LORE_ROW: &str = "Harbor Master";

/// A hermetic direct-core home with one owned World (+ one lore row) and,
/// optionally, one Work bound to that World.
struct InspectorEnv {
    fixture: DirectFixture,
    world_id: String,
}

impl InspectorEnv {
    /// Run the real `nexus42` binary against this fixture's hermetic `HOME`.
    fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seed one owned World plus one `constant`-activation lore row (always fires
/// → non-empty placement), then release every seed writer.
async fn fresh_env() -> InspectorEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(fixture.home.path());
    write_workspace_meta(fixture.home.path(), &creator_id);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);

    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let world_id = core
        .create_world(
            &principal,
            serde_json::from_value(serde_json::json!({
                "title": "Inspector Test World"
            }))
            .expect("world request shape"),
        )
        .await
        .expect("seed world")
        .world_id;
    core.close().await.expect("seed core closes");
    release_retained_writer_guards(&db_path);

    // SAFETY: test-only seed against the known kb_key_blocks schema (the same
    // fixture shape the retired `nexus-daemon-runtime/tests/inspector_api.rs`
    // used); the CLI child re-admits the writer after the seed pool closes.
    let pool = nexus_local_db::open_pool(&db_path)
        .await
        .expect("seed pool");
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, created_at, modules_json) \
           VALUES (?, ?, 'character', ?, 'confirmed', datetime('now'), \
             '{\"activation\":{\"keys\":[],\"constant\":true}}')",
    )
    .bind("kbl_inspector_lore")
    .bind(&world_id)
    .bind(LORE_ROW)
    .execute(&pool)
    .await
    .expect("seed inspector lore");
    pool.close().await;
    release_retained_writer_guards(&db_path);

    InspectorEnv { fixture, world_id }
}

/// Seed one extra owned Work bound to this fixture's World.
async fn seed_bound_work(env: &InspectorEnv) -> String {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("seed core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    let work_id = core
        .create_work(
            &principal,
            CreateWorkRequest {
                client_request_id: None,
                initial_idea: "A test story".to_string(),
                lineage_from_work_id: None,
                long_term_goal: "Test inspector moment".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(STORY_REF.to_string()),
                title: "Inspector Test Novel".to_string(),
                work_profile: Some("novel".to_string()),
                world_id: Some(env.world_id.clone()),
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");
    release_retained_writer_guards(&workspace_state_db_path(
        env.fixture.home.path(),
        &fixture_creator_id(env.fixture.home.path()),
        WORKSPACE_SLUG,
    ));
    work_id
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under the nexus root's `creators/`.
fn fixture_creator_id(home: &Path) -> String {
    let creators_root = nexus_root_from_home(home).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

/// The core resolves workspace files against the operational `meta.json`
/// `local_root` — the same key the CLI's own workspace registration writes.
fn write_workspace_meta(home: &Path, creator_id: &str) {
    let creative_root = home.join("creative");
    std::fs::create_dir_all(&creative_root).expect("materialize creative root");
    std::fs::write(
        operational_workspace_dir(home, creator_id, WORKSPACE_SLUG).join("meta.json"),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_prints_human_packet() {
    let env = fresh_env().await;

    let out = env.cli(&["creator", "inspector", "moment", &env.world_id]);
    assert!(
        out.status.success(),
        "inspector moment failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("Inspector moment"), "{text}");
    assert!(text.contains("modules: placement="), "{text}");
    assert!(text.contains("budget: primary="), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_json_emits_dto() {
    let env = fresh_env().await;

    let out = env.cli(&["creator", "inspector", "moment", &env.world_id, "--json"]);
    assert!(
        out.status.success(),
        "inspector moment --json failed: {}",
        stderr(&out)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("json packet");
    assert!(json["budget"].is_object(), "{json}");
    assert!(json["modules"].is_object(), "{json}");
    assert!(json["moment_directive"].is_object(), "{json}");
    assert!(json["slot_map"].is_array(), "{json}");
    assert_eq!(json["modules"]["placement"][0]["canonical_name"], LORE_ROW);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_moment_supports_work_and_stage_flags() {
    let env = fresh_env().await;
    let work_id = seed_bound_work(&env).await;

    let out = env.cli(&[
        "creator",
        "inspector",
        "moment",
        &env.world_id,
        "--work",
        &work_id,
        "--stage",
        "produce",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "inspector with --work/--stage failed: {}",
        stderr(&out)
    );
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("json packet");
    assert!(json["modules"].is_object(), "{json}");
    // No directive is seeded, so the packet's moment_directive is the "none"
    // status shape — success itself proves the core accepted the work→world
    // binding (a mismatch surfaces 400).
    assert_eq!(json["moment_directive"]["status"], "none", "{json}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_invalid_stage_rejected() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "inspector",
        "moment",
        &env.world_id,
        "--stage",
        "bogus",
    ]);
    assert!(!out.status.success(), "invalid --stage must fail");
    assert!(
        stderr(&out).contains("--stage"),
        "stderr should name --stage: {}",
        stderr(&out)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_foreign_world_rejected_403() {
    let env = fresh_env().await;
    // Seed a World owned by a *different* creator (ownership-gate fixture).
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    let pool = nexus_local_db::open_pool(&workspace_state_db_path(
        env.fixture.home.path(),
        &fixture_creator_id(env.fixture.home.path()),
        WORKSPACE_SLUG,
    ))
    .await
    .expect("seed pool");
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(&pool)
    .await
    .expect("seed other creator");
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES ('wld_foreign', 'ws', 'other_creator', 'Foreign', 'foreign', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .execute(&pool)
    .await
    .expect("seed foreign world");
    pool.close().await;

    let out = env.cli(&["creator", "inspector", "moment", "wld_foreign"]);
    assert!(!out.status.success(), "foreign world must fail");
    let err = stderr(&out);
    assert!(
        err.contains("403") || err.to_lowercase().contains("forbidden"),
        "stderr should surface the 403: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspector_hidden_from_creator_help_but_usable() {
    let env = fresh_env().await;

    // PL-6: `creator --help` must NOT list the hidden inspector group.
    let help = env.cli(&["creator", "--help"]);
    assert!(help.status.success());
    let text = stdout(&help);
    assert!(
        !text.to_lowercase().contains("inspector"),
        "inspector must be hidden from creator --help:\n{text}"
    );
    assert!(
        text.contains("reading"),
        "reading should be visible:\n{text}"
    );

    // But the hidden group still resolves its own help.
    let hidden = env.cli(&["creator", "inspector", "--help"]);
    assert!(
        hidden.status.success(),
        "creator inspector --help must work"
    );
    assert!(stdout(&hidden).contains("moment"));
}
