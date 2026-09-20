//! Direct-core CLI tests — `creator works outline|chapter|timeline`
//! (V1.175 P1 Task 3 group 2; direct-core retarget v1.193 P0-T6).
//!
//! The leaves run on the typed core seam (`CoreService::work_outline` /
//! `patch_outline_structure` / `patch_outline_chapter` /
//! `patch_timeline_event`) against a hermetic direct-core home — no daemon,
//! no Node child (`common/direct.rs` precedent).
//!
//! Each test seeds one owned World + Work (with `story_ref` set so the
//! outline file path is deterministic) + a chapter row, writes the
//! revision-0 outline file, releases every seed writer, and then drives the
//! REAL `nexus42` binary. Failure paths: one conflict path per leaf (stale
//! `--base-revision` → the `outline_conflict` family rendering current
//! revision + node + conflicting path + recovery hint, with the durable
//! revision and content left unchanged), the invalid-field path (bad slug →
//! `outline_validation_failed`), and the not-found path for an unknown work.

#![allow(clippy::unwrap_used)]

#[path = "common/direct.rs"]
mod direct;

use assert_cmd::Command;
use direct::DirectFixture;
use nexus_contracts::{CreateWorkRequest, CreateWorldRequest};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_home_layout::{
    nexus_root_from_home, operational_workspace_dir, workspace_state_db_path,
};
use nexus_local_db::work_chapters::{self, InsertChapterParams};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Output;

/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";
/// Deterministic `story_ref` — the core resolves the outline file against it.
const WORK_REF: &str = "outline-test-novel";
/// World title the seeded World is created under.
const WORLD_TITLE: &str = "Outline Test World";

/// The revision-0 work-level outline file: one volume holding chapter 1, the
/// default frontmatter shape the canvas operations read.
const OUTLINE_REVISION_0: &str = "---\n\
     outline_revision: 0\n\
     volumes:\n\
     \x20 - volume_id: 1\n\
     \x20   label: Volume 1\n\
     \x20   chapter_ids: [1]\n\
     timeline_events: []\n\
     foreshadows: []\n\
     chapter_titles: {}\n\
     updated_at: \"2024-01-01T00:00:00Z\"\n\
     ---\nbody\n";

/// A hermetic direct-core home with the seeded Work/chapter and the
/// revision-0 outline file on disk.
struct OutlineEnv {
    fixture: DirectFixture,
    work_id: String,
    creative_root: PathBuf,
}

impl OutlineEnv {
    /// The work-level outline markdown the canvas operations read and write.
    fn outline_path(&self) -> PathBuf {
        self.creative_root
            .join(format!("Works/{WORK_REF}/Outlines/outline.md"))
    }

    /// The per-chapter outline prose path the core derives for chapter 1.
    fn chapter_outline_path(&self) -> PathBuf {
        self.creative_root
            .join(format!("Works/{WORK_REF}/Outlines/chapters/ch01-outline.md"))
    }

    /// The stored work-level outline file (the durable storage truth).
    fn stored_outline(&self) -> String {
        std::fs::read_to_string(self.outline_path()).expect("stored outline file")
    }

    /// Run the real `nexus42` binary against this fixture's hermetic `HOME`.
    fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
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

/// Seed one owned World, one Work bound to it (`story_ref = WORK_REF`), one
/// chapter row and the revision-0 outline file — then release every seed
/// writer so the CLI child can admit its own.
async fn fresh_env() -> OutlineEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, WORKSPACE_SLUG);
    let creative_root = fixture.home.path().join("creative");

    // The core resolves workspace files against the operational `meta.json`
    // `local_root` — the same key the CLI's own workspace registration writes.
    std::fs::create_dir_all(&creative_root).expect("materialize creative root");
    std::fs::write(
        operational_workspace_dir(fixture.home.path(), &creator_id, WORKSPACE_SLUG)
            .join("meta.json"),
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

    // A Work must bind to an owned World, so the World comes first. Both go
    // through the core seam, which owns the ids, the ownership rows and the
    // work directory.
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
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({
                "title": WORLD_TITLE
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
                long_term_goal: "Test the outline canvas".to_string(),
                primary_preset_id: None,
                set_pool_active: None,
                story_ref: Some(WORK_REF.to_string()),
                title: "Outline Test Novel".to_string(),
                work_profile: Some("novel".to_string()),
                world_id: Some(world_id),
            },
        )
        .await
        .expect("seed work")
        .work_id;
    core.close().await.expect("seed core closes");

    // The chapter row is the canvas's SSOT for the node's existence; it goes
    // in after the Work row (the row carries the Work foreign key).
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("workspace pool")
        .clone_pool();
    let now = chrono::Utc::now().to_rfc3339();
    work_chapters::insert_chapter(
        &pool,
        &InsertChapterParams {
            work_id: &work_id,
            chapter: 1,
            volume: Some(1),
            slug: Some("ch01"),
            planned_word_count: 4000,
            outline_path: None,
            body_path: None,
            now: &now,
        },
    )
    .await
    .expect("seed chapter");
    pool.close().await;
    // A CLI child must not be handed a home another writer still holds.
    release_retained_writer_guards(&db_path);

    let env = OutlineEnv {
        fixture,
        work_id,
        creative_root,
    };
    std::fs::create_dir_all(env.outline_path().parent().expect("outline parent"))
        .expect("create outline dirs");
    std::fs::write(env.outline_path(), OUTLINE_REVISION_0).expect("write outline file");
    env
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── outline show ───────────────────────────────────────────────────────────

#[tokio::test]
async fn outline_show_prints_revision_and_volumes() {
    let env = fresh_env().await;

    let out = env.cli(&["creator", "works", "outline", "show", &env.work_id]);
    assert!(out.status.success(), "show failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("revision 0"), "{text}");
    assert!(text.contains("Volume 1"), "{text}");
    assert!(text.contains(&env.work_id), "{text}");
}

#[tokio::test]
async fn outline_show_json_emits_dto_verbatim() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "outline",
        "show",
        &env.work_id,
        "--json",
    ]);
    assert!(out.status.success(), "show --json failed: {}", stderr(&out));
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["work_id"], env.work_id);
    assert_eq!(json["outline_revision"], 0);
    assert!(json["volumes"].is_array());
    assert!(json["timeline_events"].is_array());
    assert!(json["foreshadows"].is_array());
    assert!(json["chapter_titles"].is_object());
}

#[tokio::test]
async fn outline_show_unknown_work_surfaces_404() {
    let env = fresh_env().await;

    let out = env.cli(&["creator", "works", "outline", "show", "wrk_does_not_exist"]);
    assert!(!out.status.success(), "unknown work must fail");
    let err = stderr(&out);
    assert!(err.contains("404"), "stderr should carry status 404: {err}");
    assert!(
        err.contains("wrk_does_not_exist"),
        "stderr should name the unknown Work: {err}"
    );
}

// ── outline patch (structure) ──────────────────────────────────────────────

#[tokio::test]
async fn outline_patch_move_chapter_bumps_revision() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "outline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "move_chapter",
        "--chapter",
        "1",
        "--volume",
        "2",
    ]);
    assert!(out.status.success(), "move_chapter failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("new_revision: 1"), "{text}");

    // The outline file on disk now has the chapter in volume 2.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("volume_id: 2"), "{on_disk}");
}

#[tokio::test]
async fn outline_patch_json_emits_dto_verbatim() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "outline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "move_chapter",
        "--chapter",
        "1",
        "--volume",
        "2",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "outline patch --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["new_revision"], 1);
    assert!(json["validation_summary"]["errors"].is_array());
    assert!(json["validation_summary"]["warnings"].is_array());
}

#[tokio::test]
async fn outline_patch_stale_revision_surfaces_conflict() {
    let env = fresh_env().await;

    // base_revision 5 vs current 0 → the outline_conflict CAS family.
    let out = env.cli(&[
        "creator",
        "works",
        "outline",
        "patch",
        &env.work_id,
        "--base-revision",
        "5",
        "--op",
        "move_chapter",
        "--chapter",
        "1",
        "--volume",
        "2",
    ]);
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry status 409: {err}");
    // All four conflict fields render: current revision, conflicting node,
    // conflicting path, recovery hint.
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("node_id"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");

    // The refusal left the durable revision and the chapter's volume binding
    // exactly as the rejected write found them.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 0"), "{on_disk}");
    assert!(on_disk.contains("volume_id: 1"), "{on_disk}");
}

#[tokio::test]
async fn outline_patch_missing_required_flag_fails_fast() {
    let env = fresh_env().await;

    // move_chapter without --volume → CLI fail-fast.
    let out = env.cli(&[
        "creator",
        "works",
        "outline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "move_chapter",
        "--chapter",
        "1",
    ]);
    assert!(!out.status.success(), "missing --volume must fail");
    assert!(
        stderr(&out).contains("--volume"),
        "stderr should name --volume: {}",
        stderr(&out)
    );
    // The rejected invocation wrote nothing.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 0"), "{on_disk}");
}

// ── chapter patch (outline node) ───────────────────────────────────────────

#[tokio::test]
async fn chapter_patch_updates_metadata_and_bumps_revision() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "0",
        "--title",
        "Chapter One",
        "--slug",
        "ch01",
        "--planned-word-count",
        "5000",
        "--status",
        "outlined",
    ]);
    assert!(out.status.success(), "chapter patch failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("new_revision: 1"), "{text}");

    // The outline file on disk carries the title + revision bump.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("Chapter One"), "{on_disk}");
}

#[tokio::test]
async fn chapter_patch_json_emits_dto_verbatim() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "0",
        "--title",
        "JSON Chapter",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "chapter patch --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["new_revision"], 1);
    assert!(json["validation_summary"]["errors"].is_array());
    assert!(json["validation_summary"]["warnings"].is_array());
}

#[tokio::test]
async fn chapter_patch_stale_revision_surfaces_conflict() {
    let env = fresh_env().await;

    // base_revision 3 vs current 0 → the outline_conflict CAS family.
    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "3",
        "--title",
        "stale",
    ]);
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry status 409: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("node_id"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");

    // The rejected write left the durable revision and the stored title
    // untouched.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 0"), "{on_disk}");
    assert!(!on_disk.contains("stale"), "{on_disk}");
}

#[tokio::test]
async fn chapter_patch_invalid_slug_surfaces_validation_failed() {
    let env = fresh_env().await;

    // Uppercase slug violates the kebab-case rule → the
    // outline_validation_failed family (the invalid-field path).
    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "0",
        "--slug",
        "Bad Slug!",
    ]);
    assert!(!out.status.success(), "invalid slug must fail");
    let err = stderr(&out);
    assert!(
        err.contains("outline_validation_failed"),
        "stderr should name the family: {err}"
    );
    assert!(err.contains("422"), "stderr should carry status 422: {err}");
    assert!(
        err.contains("kebab-case"),
        "stderr should name the rule: {err}"
    );
}

#[tokio::test]
async fn chapter_patch_no_set_field_fails_fast() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "0",
    ]);
    assert!(!out.status.success(), "empty set must fail");
    assert!(
        stderr(&out).contains("--title"),
        "stderr should list the set flags: {}",
        stderr(&out)
    );
}

#[tokio::test]
async fn chapter_patch_content_file_writes_outline_prose() {
    let env = fresh_env().await;

    // Seed an existing per-chapter outline file so the content patch has a
    // target (the core derives `Works/<ref>/Outlines/chapters/ch01-outline.md`
    // for a chapter row with no stored `outline_path`).
    let chapter_outline = env.chapter_outline_path();
    std::fs::create_dir_all(chapter_outline.parent().expect("chapter outline parent"))
        .expect("create chapter outline dirs");
    std::fs::write(&chapter_outline, "# Old outline\n").expect("write chapter outline");

    let content_file = env.fixture.home.path().join("content.md");
    std::fs::write(&content_file, "## Scene beats\n\n- Open on the harbor\n")
        .expect("write content file");

    let out = env.cli(&[
        "creator",
        "works",
        "chapter",
        "patch",
        &env.work_id,
        "--n",
        "1",
        "--base-revision",
        "0",
        "--content-file",
        content_file.to_str().expect("content file path"),
    ]);
    assert!(out.status.success(), "content patch failed: {}", stderr(&out));
    assert!(stdout(&out).contains("new_revision: 1"), "{}", stdout(&out));

    let on_disk = std::fs::read_to_string(&chapter_outline).unwrap();
    assert!(
        on_disk.contains("## Scene beats"),
        "chapter outline should hold patched prose; got: {on_disk}"
    );
    assert!(
        !on_disk.contains("# Old outline"),
        "chapter outline should not hold stale prose; got: {on_disk}"
    );
}

// ── timeline patch ─────────────────────────────────────────────────────────

#[tokio::test]
async fn timeline_patch_add_event_bumps_revision() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "timeline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "add_event",
        "--title",
        "The storm",
        "--description",
        "A storm hits the harbor",
        "--realizes-chapter",
        "1",
    ]);
    assert!(out.status.success(), "add_event failed: {}", stderr(&out));
    assert!(stdout(&out).contains("new_revision: 1"), "{}", stdout(&out));

    // The outline file on disk now carries the event.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 1"), "{on_disk}");
    assert!(on_disk.contains("The storm"), "{on_disk}");
}

#[tokio::test]
async fn timeline_patch_json_emits_dto_verbatim() {
    let env = fresh_env().await;

    let out = env.cli(&[
        "creator",
        "works",
        "timeline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "add_event",
        "--title",
        "The storm",
        "--json",
    ]);
    assert!(
        out.status.success(),
        "timeline patch --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["new_revision"], 1);
    assert!(json["validation_summary"]["errors"].is_array());
    assert!(json["validation_summary"]["warnings"].is_array());
}

#[tokio::test]
async fn timeline_patch_stale_revision_surfaces_conflict() {
    let env = fresh_env().await;

    // base_revision 2 vs current 0 → the outline_conflict CAS family.
    let out = env.cli(&[
        "creator",
        "works",
        "timeline",
        "patch",
        &env.work_id,
        "--base-revision",
        "2",
        "--op",
        "add_event",
        "--title",
        "stale",
    ]);
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("outline_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry status 409: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("node_id"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");

    // No event landed: the durable revision and event list are untouched.
    let on_disk = env.stored_outline();
    assert!(on_disk.contains("outline_revision: 0"), "{on_disk}");
    assert!(on_disk.contains("timeline_events: []"), "{on_disk}");
    assert!(!on_disk.contains("stale"), "{on_disk}");
}

#[tokio::test]
async fn timeline_patch_missing_required_flag_fails_fast() {
    let env = fresh_env().await;

    // add_event without --title → CLI fail-fast.
    let out = env.cli(&[
        "creator",
        "works",
        "timeline",
        "patch",
        &env.work_id,
        "--base-revision",
        "0",
        "--op",
        "add_event",
    ]);
    assert!(!out.status.success(), "missing --title must fail");
    assert!(
        stderr(&out).contains("--title"),
        "stderr should name --title: {}",
        stderr(&out)
    );
}

// ── help ───────────────────────────────────────────────────────────────────

#[test]
fn outline_help_documents_route_family_and_retry() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "outline", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("show"), "{text}");
    assert!(text.contains("patch"), "{text}");
    assert!(text.contains("outline_conflict"), "{text}");
    assert!(text.contains("reapply"), "{text}");
}

#[test]
fn chapter_help_pins_outline_node_route_distinction() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "works", "chapter", "patch", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    // The route-family guard: the leaf names the outline node patch and the
    // V1.65 chapter-content distinction (AR-84).
    assert!(text.contains("chapters/:n/patch"), "{text}");
    assert!(text.contains("V1.65"), "{text}");
    assert!(text.contains("outline"), "{text}");
    assert!(text.contains("outline_conflict"), "{text}");
    assert!(text.contains("reapply"), "{text}");
}
