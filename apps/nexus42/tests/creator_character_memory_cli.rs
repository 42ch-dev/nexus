//! Process-level `nexus42 creator character memory` against the direct-core
//! actor fixture (v1.193 P0-T11).
//!
//! The whole lifecycle runs the REAL `nexus42` binary against a hermetic
//! direct-core home: no daemon fixture, no HTTP client, no mock host. Worlds
//! and Characters are seeded through the fixture's authorized core writers and
//! every memory verb is the migrated core bearer operation. Asserts:
//! - deterministic human output and `--json` DTO parity for the
//!   capture → pending → review → fragments → promote lifecycle;
//! - fail-closed denial (foreign/missing/inactive Character, foreign binding)
//!   with zero memory mutation;
//! - the retired `creator character run` / `soul reflect` entrances are
//!   unknown commands while core capture/synthesis behavior stays in place.

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct_actor::DirectActor;
use serde_json::Value;
use std::process::Output;

/// Deterministic marker strings seeded through public operations only.
const SHARED_MEMORY_MARKER: &str =
    "SHAREDMEMORYMARKER the harbor accord holds because Ava keeps it";
const LOCAL_MEMORY_MARKER: &str = "LOCALMEMORYMARKER only W1 saw the lantern signal at dusk";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn cli_ok(actor: &DirectActor, args: &[&str]) -> Output {
    let out = actor.cli(args);
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    out
}

fn json_out(out: &Output) -> Value {
    serde_json::from_str(&stdout(out)).unwrap_or_else(|_| panic!("json: {}", stdout(out)))
}

/// >= 50 chars with research task kind → FragmentOnly.
fn fragment_digest(marker: &str) -> String {
    format!("{marker} — researched background detail for texture and continuity.")
}

/// Count the stored pending-review rows on the released workspace DB.
async fn pending_row_count(actor: &DirectActor) -> i64 {
    let pool = actor.read_only_pool().await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM character_memory_pending_review")
        .fetch_one(&pool)
        .await
        .expect("count character_memory_pending_review");
    pool.close().await;
    count
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)] // single lifecycle parity proof
async fn character_memory_lifecycle_json_and_human_parity() {
    let actor = DirectActor::new().await;
    let world_w1 = actor.create_world("Memory World One").await;
    let seeded = actor.create_character("Ava", &world_w1).await;
    let chr = seeded.character_id.as_str();
    let binding = seeded.binding_id.as_str();

    // capture (json): returns the generated response DTO.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            "pend_cli_1",
            "--session-id",
            "sess_cli_1",
            "--task-kind",
            "research",
            "--digest",
            &fragment_digest(SHARED_MEMORY_MARKER),
            "--json",
        ],
    );
    let payload = json_out(&out);
    assert_eq!(payload["success"], true);
    assert_eq!(payload["pending_id"], "pend_cli_1");

    // capture (human): deterministic labeled lines, not JSON.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            "pend_cli_2",
            "--session-id",
            "sess_cli_2",
            "--binding-id",
            binding,
            "--task-kind",
            "research",
            "--digest",
            &fragment_digest(LOCAL_MEMORY_MARKER),
        ],
    );
    let human = stdout(&out);
    assert!(human.contains("pend_cli_2"));
    assert!(!human.trim_start().starts_with('{'));

    // pending-count, both scopes, both output modes.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            chr,
            "--json",
        ],
    );
    assert_eq!(json_out(&out)["count"], 1);
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            chr,
        ],
    );
    assert!(stdout(&out).contains('1'));
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            chr,
            "--binding-id",
            binding,
            "--json",
        ],
    );
    assert_eq!(json_out(&out)["count"], 1);

    // pending-list: shared scope shows pend_cli_1 only.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "pending-list",
            "--character-id",
            chr,
            "--json",
        ],
    );
    let payload = json_out(&out);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pend_cli_1");

    // review (json): the unscoped drain empties the shared scope only; the
    // binding scope is drained explicitly afterwards.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "review",
            "--character-id",
            chr,
            "--json",
        ],
    );
    let payload = json_out(&out);
    assert_eq!(payload["fragmented"], 1);
    assert_eq!(payload["has_more"], false);

    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "review",
            "--character-id",
            chr,
            "--binding-id",
            binding,
        ],
    );
    assert!(stdout(&out).contains("fragmented=1"));

    // fragments: the binding-local row carries revision 0 and the marker.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "fragments",
            "--character-id",
            chr,
            "--binding-id",
            binding,
            "--json",
        ],
    );
    let payload = json_out(&out);
    let fragments = payload["fragments"].as_array().unwrap();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["revision"], 0);
    assert!(fragments[0]["summary"]
        .as_str()
        .unwrap()
        .contains(LOCAL_MEMORY_MARKER));
    let fragment_id = fragments[0]["fragment_id"].as_str().unwrap().to_string();

    // promote: stale revision is a stable failure with no mutation.
    let out = actor.cli(&[
        "creator",
        "character",
        "memory",
        "promote",
        "--character-id",
        chr,
        "--fragment-id",
        &fragment_id,
        "--expected-revision",
        "9",
    ]);
    assert!(!out.status.success(), "stale promote: {}", stdout(&out));
    assert!(
        stderr(&out).contains("version_mismatch"),
        "stale promote stderr: {}",
        stderr(&out)
    );

    // promote: correct revision clears provenance (shared scope gains it).
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "promote",
            "--character-id",
            chr,
            "--fragment-id",
            &fragment_id,
            "--expected-revision",
            "0",
            "--json",
        ],
    );
    let payload = json_out(&out);
    assert_eq!(payload["fragment"]["fragment_id"], fragment_id);
    assert_eq!(payload["fragment"]["revision"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)] // fail-closed proof
async fn character_memory_fail_closed_no_mutation() {
    let actor = DirectActor::new().await;
    let world_w1 = actor.create_world("Memory World One").await;
    let character_a = actor.create_character("Ava", &world_w1).await;
    let character_b = actor.create_character("Ben", &world_w1).await;

    // Foreign (missing) character id: every memory verb fails.
    let missing = "chr_ffffffffffffffffffffffffffffffff";
    for args in [
        vec![
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            missing,
            "--pending-id",
            "pend_x",
            "--session-id",
            "sess_x",
            "--digest",
            "irrelevant digest text",
        ],
        vec![
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            missing,
        ],
        vec![
            "creator",
            "character",
            "memory",
            "review",
            "--character-id",
            missing,
        ],
        vec![
            "creator",
            "character",
            "memory",
            "fragments",
            "--character-id",
            missing,
        ],
        vec![
            "creator",
            "character",
            "memory",
            "pending-dismiss",
            "--character-id",
            missing,
            "--pending-id",
            "pend_x",
        ],
    ] {
        let out = actor.cli(&args);
        assert!(
            !out.status.success(),
            "{args:?} must fail: {}",
            stdout(&out)
        );
    }

    // Cross-character binding: A's memory verbs must not accept B's binding.
    let out = actor.cli(&[
        "creator",
        "character",
        "memory",
        "capture",
        "--character-id",
        &character_a.character_id,
        "--pending-id",
        "pend_xb",
        "--session-id",
        "sess_xb",
        "--binding-id",
        &character_b.binding_id,
        "--digest",
        "irrelevant digest text that is long enough to matter",
    ]);
    assert!(!out.status.success(), "cross-character binding accepted");

    // Inactive character: archive B through the core's own lifecycle
    // transition, then its write verbs deny (reads may retain).
    cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "archive",
            &character_b.character_id,
            "--expected-revision",
            "0",
        ],
    );
    let out = actor.cli(&[
        "creator",
        "character",
        "memory",
        "capture",
        "--character-id",
        &character_b.character_id,
        "--pending-id",
        "pend_archived",
        "--session-id",
        "sess_archived",
        "--digest",
        "irrelevant digest text that is long enough to matter",
    ]);
    assert!(!out.status.success(), "inactive character write must deny");

    // Zero mutation proof: A's queues stay empty, and no pending row exists.
    let out = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            &character_a.character_id,
            "--json",
        ],
    );
    assert_eq!(json_out(&out)["count"], 0);
    assert_eq!(
        pending_row_count(&actor).await,
        0,
        "deny matrix must not write pending rows"
    );
}

/// The retired entrances stay retired: `creator character run` (daemon-stream
/// plus durable owner-outcome observation) and `creator character soul
/// reflect` (registry-backed forced synthesis) have no complete direct CLI
/// closure, so both are unknown commands while core host capture/session and
/// soul synthesis behavior remain library/core operations.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retired_character_run_and_reflect_entrances_are_unknown_commands() {
    let actor = DirectActor::new().await;
    for args in [
        vec![
            "creator",
            "character",
            "run",
            "--character-id",
            "chr_x",
            "--world-id",
            "wld_x",
            "--binding-id",
            "awb_x",
            "--prompt",
            "Act now.",
        ],
        vec![
            "creator",
            "character",
            "soul",
            "reflect",
            "--character-id",
            "chr_x",
            "--force",
        ],
    ] {
        let out = actor.cli(&args);
        let combined = format!("{}{}", stdout(&out), stderr(&out));
        assert!(
            !out.status.success(),
            "{args:?} must not be a command: {combined}"
        );
        assert!(
            combined.contains("unrecognized subcommand"),
            "retired leaf `{args:?}` must be an unknown subcommand: {combined}"
        );
    }
}
