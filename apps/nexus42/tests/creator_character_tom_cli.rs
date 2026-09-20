//! Process-level `nexus42 creator character tom` against the direct-core actor
//! fixture (v1.193 P0-T11).
//!
//! The whole journey runs the REAL `nexus42` binary against a hermetic
//! direct-core home: no daemon fixture, no HTTP client. Worlds, Characters and
//! their bindings are seeded through the fixture's authorized core writers, the
//! carrier is authored through the shipped `creator character knowledge add`
//! verb, and every belief is recorded/read back through the migrated core arms
//! — carrier admission, the revision CAS and the reviewer's keyset order are
//! the core's own.

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct_actor::DirectActor;
use serde_json::Value;
use std::process::Output;

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

/// Seed one `character`-owned carrier for `character_id` through the shipped
/// `creator character knowledge add` authoring verb.
fn seed_tom_carrier(actor: &DirectActor, character_id: &str) -> String {
    let out = cli_ok(
        actor,
        &[
            "creator",
            "character",
            "knowledge",
            "add",
            "--owner",
            "character",
            "--character-id",
            character_id,
            "--block-type",
            "character",
            "--canonical-name",
            "TomCarrierCli",
            "--json",
        ],
    );
    json_out(&out)["item"]["entry_id"]
        .as_str()
        .expect("created carrier entry_id")
        .to_string()
}

#[allow(clippy::too_many_arguments)] // CLI argv mapping
fn record_argv(
    character_id: &str,
    world_id: &str,
    binding_id: &str,
    carrier_id: &str,
    holder: &str,
    order: i64,
    revision: u64,
    json: bool,
) -> Vec<String> {
    let mut v = vec![
        "creator".to_string(),
        "character".to_string(),
        "tom".to_string(),
        "record".to_string(),
        "--character-id".to_string(),
        character_id.to_string(),
        "--world-id".to_string(),
        world_id.to_string(),
        "--binding-id".to_string(),
        binding_id.to_string(),
        "--carrier-entry-id".to_string(),
        carrier_id.to_string(),
        "--expected-revision".to_string(),
        revision.to_string(),
        "--holder".to_string(),
        holder.to_string(),
        "--proposition".to_string(),
        "CLI belief proposition".to_string(),
        "--order".to_string(),
        order.to_string(),
        "--truth".to_string(),
        "True".to_string(),
        "--access".to_string(),
        "Private".to_string(),
        "--representation".to_string(),
        "Explicit".to_string(),
        "--content-type".to_string(),
        "Location".to_string(),
        "--source".to_string(),
        "Perception".to_string(),
        "--context".to_string(),
        "Neutral".to_string(),
    ];
    if json {
        v.push("--json".to_string());
    }
    v
}

fn as_strs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_tom_record_show_json_and_human_parity() {
    let actor = DirectActor::new().await;
    let world_w1 = actor.create_world("RN-ACT-4 World One").await;
    let character_a = actor.create_character("Ava", &world_w1).await;
    let character_b = actor.create_character("Ben", &world_w1).await;
    let carrier = seed_tom_carrier(&actor, &character_a.character_id);

    // L1: the viewer Character's own belief, `--json`, first CAS step.
    let json_args = record_argv(
        &character_a.character_id,
        &world_w1,
        &character_a.binding_id,
        &carrier,
        &character_a.character_id,
        1,
        0,
        true,
    );
    let out = cli_ok(&actor, &as_strs(&json_args));
    let payload = json_out(&out);
    assert_eq!(payload["revision"], 1);
    assert_eq!(payload["carrier_entry_id"], carrier);

    // L2: a belief about another Character, human output, next CAS step.
    let human_args = record_argv(
        &character_a.character_id,
        &world_w1,
        &character_a.binding_id,
        &carrier,
        &character_b.character_id,
        2,
        1,
        false,
    );
    let out = cli_ok(&actor, &as_strs(&human_args));
    let human = stdout(&out);
    assert!(human.contains("Recorded ToM belief"));
    assert!(!human.trim_start().starts_with('{'));

    let show_json = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "tom",
            "show",
            "--character-id",
            &character_a.character_id,
            "--world-id",
            &world_w1,
            "--binding-id",
            &character_a.binding_id,
            "--json",
        ],
    );
    let page = json_out(&show_json);
    let orders: Vec<i64> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["order"].as_i64().unwrap())
        .collect();
    assert_eq!(orders, vec![1, 2]);

    let show_human = cli_ok(
        &actor,
        &[
            "creator",
            "character",
            "tom",
            "show",
            "--character-id",
            &character_a.character_id,
            "--world-id",
            &world_w1,
            "--binding-id",
            &character_a.binding_id,
        ],
    );
    let text = stdout(&show_human);
    assert!(text.contains("## Character ToM — L1"));
    assert!(text.contains("## Character ToM — L2"));
    assert!(text.contains("CLI belief proposition"));
}

/// Count the stored derivative `MindState` rows on the released workspace DB.
async fn mind_state_count(actor: &DirectActor) -> i64 {
    let pool = actor.read_only_pool().await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mind_states")
        .fetch_one(&pool)
        .await
        .expect("count mind_states");
    pool.close().await;
    count
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_tom_fail_closed_no_mutation() {
    let actor = DirectActor::new().await;
    let world_w1 = actor.create_world("RN-ACT-4 World One").await;
    let character_a = actor.create_character("Ava", &world_w1).await;
    let character_b = actor.create_character("Ben", &world_w1).await;
    let carrier = seed_tom_carrier(&actor, &character_a.character_id);

    let before_ms = mind_state_count(&actor).await;

    let stale = record_argv(
        &character_a.character_id,
        &world_w1,
        &character_a.binding_id,
        &carrier,
        &character_a.character_id,
        1,
        9,
        false,
    );
    let out = actor.cli(&as_strs(&stale));
    assert!(!out.status.success(), "stale revision must fail");

    let foreign = record_argv(
        &character_a.character_id,
        &world_w1,
        &character_b.binding_id,
        &carrier,
        &character_a.character_id,
        1,
        0,
        false,
    );
    let out = actor.cli(&as_strs(&foreign));
    assert!(!out.status.success(), "foreign binding must fail");

    assert_eq!(
        before_ms,
        mind_state_count(&actor).await,
        "deny matrix must not insert MindState"
    );
}
