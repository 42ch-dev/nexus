//! Process-level `nexus42 creator character tom` against a live daemon (v1.184 P4 T3).

mod common;

use common::rn_act4::{seed, stderr, stdout};
use common::LiveDaemon;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::store::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use serde_json::{json, Value};
use std::process::Output;

async fn cli_ok(d: &LiveDaemon, args: &[&str]) -> Output {
    let out = d.cli(args).await;
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    out
}

fn json_out(out: &Output) -> Value {
    serde_json::from_str(&stdout(out)).unwrap_or_else(|_| panic!("json: {}", stdout(out)))
}

async fn seed_tom_carrier(d: &LiveDaemon, character_id: &str) -> String {
    let store = SqliteKbStore::new(d.pool.clone());
    let mut kb =
        KnowledgeEntryRecord::for_character(character_id, BlockType::Character, "TomCarrierCli");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
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
    let d = LiveDaemon::start().await;
    let g = seed(&d).await;
    let carrier = seed_tom_carrier(&d, &g.character_a).await;

    let json_args = record_argv(
        &g.character_a,
        &g.world_w1,
        &g.bind_a_w1,
        &carrier,
        &g.character_a,
        1,
        0,
        true,
    );
    let out = cli_ok(&d, &as_strs(&json_args)).await;
    let payload = json_out(&out);
    assert_eq!(payload["revision"], 1);
    assert_eq!(payload["carrier_entry_id"], carrier);

    let human_args = record_argv(
        &g.character_a,
        &g.world_w1,
        &g.bind_a_w1,
        &carrier,
        &g.character_b,
        2,
        1,
        false,
    );
    let out = cli_ok(&d, &as_strs(&human_args)).await;
    let human = stdout(&out);
    assert!(human.contains("Recorded ToM belief"));
    assert!(!human.trim_start().starts_with('{'));

    let show_json = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "tom",
            "show",
            "--character-id",
            &g.character_a,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
            "--json",
        ],
    )
    .await;
    let page = json_out(&show_json);
    let orders: Vec<i64> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["order"].as_i64().unwrap())
        .collect();
    assert_eq!(orders, vec![1, 2]);

    let show_human = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "tom",
            "show",
            "--character-id",
            &g.character_a,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
        ],
    )
    .await;
    let text = stdout(&show_human);
    assert!(text.contains("## Character ToM — L1"));
    assert!(text.contains("## Character ToM — L2"));
    assert!(text.contains("CLI belief proposition"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_tom_fail_closed_no_mutation() {
    let d = LiveDaemon::start().await;
    let g = seed(&d).await;
    let carrier = seed_tom_carrier(&d, &g.character_a).await;

    let before_ms: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mind_states")
        .fetch_one(&d.pool)
        .await
        .unwrap();

    let stale = record_argv(
        &g.character_a,
        &g.world_w1,
        &g.bind_a_w1,
        &carrier,
        &g.character_a,
        1,
        9,
        false,
    );
    let out = d.cli(&as_strs(&stale)).await;
    assert!(!out.status.success(), "stale revision must fail");

    let foreign = record_argv(
        &g.character_a,
        &g.world_w1,
        &g.bind_b_w1,
        &carrier,
        &g.character_a,
        1,
        0,
        false,
    );
    let out = d.cli(&as_strs(&foreign)).await;
    assert!(!out.status.success(), "foreign binding must fail");

    let after_ms: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mind_states")
        .fetch_one(&d.pool)
        .await
        .unwrap();
    assert_eq!(
        before_ms.0, after_ms.0,
        "deny matrix must not insert MindState"
    );
}
