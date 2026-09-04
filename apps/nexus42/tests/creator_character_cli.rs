//! Process-level `nexus42 creator character` journey against a live daemon.

mod common;

use common::LiveDaemon;
use serde_json::Value;
use std::process::Output;

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
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
