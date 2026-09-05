//! Process-level `nexus42 creator character` journey against a live daemon.

mod common;

use common::LiveDaemon;
use serde_json::Value;
use std::process::Output;

const OWNER: &str = "ctr_localabcdef123456";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

async fn activate_owner(d: &LiveDaemon) {
    nexus_local_db::ensure_creator_row(&d.pool, OWNER, "Owner")
        .await
        .unwrap();
    for (world_id, slug) in [(WORLD_A, "world-a"), (WORLD_B, "world-b")] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(OWNER)
        .bind(world_id)
        .bind(slug)
        .execute(&d.pool)
        .await
        .unwrap();
    }

    let config_path = d.home.path().join(".nexus42").join("config.toml");
    let existing = std::fs::read_to_string(&config_path).unwrap();
    let daemon_url = existing
        .lines()
        .find_map(|l| l.strip_prefix("daemon_url = "))
        .map(str::to_string)
        .expect("daemon_url");
    std::fs::write(
        &config_path,
        format!(
            "active_creator_id = \"{OWNER}\"\n\
             daemon_url = {daemon_url}\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"{OWNER}\" = \"default\"\n"
        ),
    )
    .unwrap();
}

#[tokio::test]
async fn create_bind_remove_journey_human_and_json() {
    let d = LiveDaemon::start().await;
    activate_owner(&d).await;

    let created = d
        .cli(&[
            "creator",
            "character",
            "create",
            "--display-name",
            "Ava",
            "--world-id",
            WORLD_A,
            "--json",
        ])
        .await;
    assert!(
        created.status.success(),
        "create json: {}",
        stderr(&created)
    );
    let created_json: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_json["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_binding = created_json["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(created_json["character"]["display_name"], "Ava");

    let human = d.cli(&["creator", "character", "show", &chr]).await;
    assert!(human.status.success(), "show human: {}", stderr(&human));
    assert!(stdout(&human).contains(&chr));
    assert!(stdout(&human).contains("Ava"));
    assert!(!stdout(&human).trim_start().starts_with('{'));

    let json_show = d
        .cli(&["creator", "character", "show", &chr, "--json"])
        .await;
    assert!(json_show.status.success());
    let shown: Value = serde_json::from_str(&stdout(&json_show)).unwrap();
    assert_eq!(shown["character"]["character_id"], chr);

    let listed = d.cli(&["creator", "character", "list", "--json"]).await;
    assert!(listed.status.success());
    let list_json: Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(list_json["items"].as_array().unwrap().len(), 1);

    let bound = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "add",
            "--character-id",
            &chr,
            "--world-id",
            WORLD_B,
            "--json",
        ])
        .await;
    assert!(bound.status.success(), "bind: {}", stderr(&bound));
    let bound_json: Value = serde_json::from_str(&stdout(&bound)).unwrap();
    let second = bound_json["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let last = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "remove",
            "--character-id",
            &chr,
            "--binding-id",
            &first_binding,
        ])
        .await;
    assert!(last.status.success(), "non-last remove: {}", stderr(&last));

    let fail_closed = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "remove",
            "--character-id",
            &chr,
            "--binding-id",
            &second,
        ])
        .await;
    assert!(!fail_closed.status.success());
    assert!(
        stderr(&fail_closed).contains("last_active_actor_world_binding"),
        "stderr={}",
        stderr(&fail_closed)
    );
}


#[tokio::test]
async fn binding_list_human_and_json_paginate() {
    let d = LiveDaemon::start().await;
    activate_owner(&d).await;

    let created = d
        .cli(&[
            "creator",
            "character",
            "create",
            "--display-name",
            "Ava",
            "--world-id",
            WORLD_A,
            "--json",
        ])
        .await;
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_json: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_json["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let bound = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "add",
            "--character-id",
            &chr,
            "--world-id",
            WORLD_B,
            "--json",
        ])
        .await;
    assert!(bound.status.success(), "bind: {}", stderr(&bound));

    let json_page = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "list",
            "--character-id",
            &chr,
            "--limit",
            "1",
            "--json",
        ])
        .await;
    assert!(
        json_page.status.success(),
        "json list: {}",
        stderr(&json_page)
    );
    let page: Value = serde_json::from_str(&stdout(&json_page)).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["pagination"]["has_more"], true);
    let cursor = page["pagination"]["next_cursor"].as_str().unwrap().to_string();

    let json_page2 = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "list",
            "--character-id",
            &chr,
            "--limit",
            "1",
            "--cursor",
            &cursor,
            "--json",
        ])
        .await;
    assert!(json_page2.status.success(), "json page2: {}", stderr(&json_page2));
    let page2: Value = serde_json::from_str(&stdout(&json_page2)).unwrap();
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert_ne!(page["items"][0]["binding_id"], page2["items"][0]["binding_id"]);

    let human = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "list",
            "--character-id",
            &chr,
            "--limit",
            "1",
        ])
        .await;
    assert!(human.status.success(), "human list: {}", stderr(&human));
    let human_out = stdout(&human);
    assert!(!human_out.trim_start().starts_with('{'));
    let human_cursor = human_out
        .lines()
        .find_map(|line| line.strip_prefix("next_cursor: "))
        .expect("human next_cursor")
        .to_string();

    let human2 = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "list",
            "--character-id",
            &chr,
            "--limit",
            "1",
            "--cursor",
            &human_cursor,
        ])
        .await;
    assert!(human2.status.success(), "human page2: {}", stderr(&human2));
    let human2_out = stdout(&human2);
    let first_id = page["items"][0]["binding_id"].as_str().unwrap();
    let second_id = page2["items"][0]["binding_id"].as_str().unwrap();
    assert!(human_out.contains(first_id) || human_out.contains(second_id));
    assert!(human2_out.contains(first_id) || human2_out.contains(second_id));
    assert_ne!(
        human_out.lines().next().unwrap_or(""),
        human2_out.lines().next().unwrap_or("")
    );
}

#[tokio::test]
async fn knowledge_add_list_view_json_round_trip() {
    let d = LiveDaemon::start().await;
    activate_owner(&d).await;

    let created = d
        .cli(&[
            "creator",
            "character",
            "create",
            "--display-name",
            "Ava",
            "--world-id",
            WORLD_A,
            "--json",
        ])
        .await;
    assert!(created.status.success(), "create: {}", stderr(&created));
    let body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = body["character"]["character_id"].as_str().unwrap().to_string();
    let bind = body["binding"]["binding_id"].as_str().unwrap().to_string();

    let added = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "add",
            "--owner",
            "character",
            "--character-id",
            &chr,
            "--block-type",
            "item",
            "--canonical-name",
            "CharNote",
            "--json",
        ])
        .await;
    assert!(added.status.success(), "add: {}", stderr(&added));
    let added_body: Value = serde_json::from_str(&stdout(&added)).unwrap();
    assert_eq!(added_body["item"]["canonical_name"], "CharNote");

    let listed = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "list",
            "--character-id",
            &chr,
            "--json",
        ])
        .await;
    assert!(listed.status.success(), "list: {}", stderr(&listed));
    let list_body: Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(list_body["items"].as_array().unwrap().len(), 1);

    let viewed = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "view",
            "--actor",
            "character",
            "--character-id",
            &chr,
            "--world-id",
            WORLD_A,
            "--binding-id",
            &bind,
            "--json",
        ])
        .await;
    assert!(viewed.status.success(), "view: {}", stderr(&viewed));
    let view_body: Value = serde_json::from_str(&stdout(&viewed)).unwrap();
    assert_eq!(view_body["items"][0]["canonical_name"], "CharNote");
}
