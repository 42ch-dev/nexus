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
#[allow(clippy::too_many_lines, clippy::similar_names)] // e2e pagination proof (A/B pages)

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
    let cursor = page["pagination"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_string();

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
    assert!(
        json_page2.status.success(),
        "json page2: {}",
        stderr(&json_page2)
    );
    let page2: Value = serde_json::from_str(&stdout(&json_page2)).unwrap();
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        page["items"][0]["binding_id"],
        page2["items"][0]["binding_id"]
    );

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
    let chr = body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
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

#[tokio::test]
async fn edit_archive_restore_cli_honors_explicit_revision_cas() {
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
            "--image-uri",
            "https://example.test/ava.png",
            "--persona",
            "{\"role\":\"scout\"}",
            "--json",
        ])
        .await;
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let edited = d
        .cli(&[
            "creator",
            "character",
            "edit",
            &chr,
            "--expected-revision",
            "0",
            "--display-name",
            "Ada",
            "--clear-image-uri",
            "--json",
        ])
        .await;
    assert!(edited.status.success(), "edit: {}", stderr(&edited));
    let edit_body: Value = serde_json::from_str(&stdout(&edited)).unwrap();
    assert_eq!(edit_body["character"]["display_name"], "Ada");
    assert_eq!(edit_body["character"]["revision"], 1);
    assert!(
        edit_body["character"].get("image_uri").map_or(true, |v| v.is_null()),
        "cleared image_uri should be null or omitted: {}",
        edit_body["character"]["image_uri"]
    );
    assert_eq!(edit_body["character"]["persona"]["role"], "scout");

    let stale = d
        .cli(&[
            "creator",
            "character",
            "edit",
            &chr,
            "--expected-revision",
            "0",
            "--display-name",
            "Stale",
        ])
        .await;
    assert!(!stale.status.success(), "stale edit must fail");
    assert!(stderr(&stale).contains("character_revision_conflict"));

    let archived = d
        .cli(&[
            "creator",
            "character",
            "archive",
            &chr,
            "--expected-revision",
            "1",
            "--json",
        ])
        .await;
    assert!(archived.status.success(), "archive: {}", stderr(&archived));
    let arch_body: Value = serde_json::from_str(&stdout(&archived)).unwrap();
    assert_eq!(arch_body["character"]["status"], "archived");

    let write_denied = d
        .cli(&[
            "creator",
            "character",
            "edit",
            &chr,
            "--expected-revision",
            "2",
            "--display-name",
            "Nope",
        ])
        .await;
    assert!(!write_denied.status.success());
    assert!(stderr(&write_denied).contains("character_inactive"));

    let restored = d
        .cli(&[
            "creator",
            "character",
            "restore",
            &chr,
            "--expected-revision",
            "2",
            "--json",
        ])
        .await;
    assert!(restored.status.success(), "restore: {}", stderr(&restored));
    let restore_body: Value = serde_json::from_str(&stdout(&restored)).unwrap();
    assert_eq!(restore_body["character"]["status"], "active");
    assert_eq!(restore_body["character"]["character_id"], chr);
}


#[tokio::test]
async fn edit_without_mutable_fields_is_invalid_input() {
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
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let empty = d
        .cli(&[
            "creator",
            "character",
            "edit",
            &chr,
            "--expected-revision",
            "0",
        ])
        .await;
    assert!(!empty.status.success(), "empty edit must fail");
    let err = stderr(&empty);
    assert!(
        err.contains("at least one mutable field") || err.contains("invalid_input"),
        "unexpected stderr: {err}"
    );
}
async fn seed_character_sheet(
    d: &LiveDaemon,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at)          VALUES (?, ?, 'character', ?, 'confirmed', '{}', datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .execute(&d.pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn binding_detail_edit_link_relink_clear_and_refusals() {
    let d = LiveDaemon::start().await;
    activate_owner(&d).await;
    seed_character_sheet(&d, "kb_sheet_a", WORLD_A, "sheet_a").await;
    seed_character_sheet(&d, "kb_sheet_b", WORLD_A, "sheet_b").await;

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
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let binding_id = created_body["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let shown = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "show",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--json",
        ])
        .await;
    assert!(shown.status.success(), "show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["binding"]["revision"], 0);

    let linked = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "0",
            "--world-sheet-entry-id",
            "kb_sheet_a",
            "--json",
        ])
        .await;
    assert!(linked.status.success(), "link: {}", stderr(&linked));
    let linked_body: Value = serde_json::from_str(&stdout(&linked)).unwrap();
    assert_eq!(linked_body["binding"]["revision"], 1);
    assert_eq!(linked_body["binding"]["world_sheet_entry_id"], "kb_sheet_a");

    let relinked = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "1",
            "--world-sheet-entry-id",
            "kb_sheet_b",
            "--json",
        ])
        .await;
    assert!(relinked.status.success(), "relink: {}", stderr(&relinked));
    let relink_body: Value = serde_json::from_str(&stdout(&relinked)).unwrap();
    assert_eq!(relink_body["binding"]["revision"], 2);

    let cleared = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "2",
            "--clear-world-sheet",
            "--json",
        ])
        .await;
    assert!(cleared.status.success(), "clear: {}", stderr(&cleared));
    let clear_body: Value = serde_json::from_str(&stdout(&cleared)).unwrap();
    assert_eq!(clear_body["binding"]["revision"], 3);
    assert!(clear_body["binding"]["world_sheet_entry_id"].is_null());

    let stale = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "2",
            "--clear-world-sheet",
        ])
        .await;
    assert!(!stale.status.success());
    assert!(
        stderr(&stale).contains("binding_revision_conflict"),
        "stderr={}",
        stderr(&stale)
    );

    let second = d
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
    assert!(second.status.success(), "add second: {}", stderr(&second));
    let second_body: Value = serde_json::from_str(&stdout(&second)).unwrap();
    let second_binding = second_body["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let removed = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "remove",
            "--character-id",
            &chr,
            "--binding-id",
            &second_binding,
        ])
        .await;
    assert!(removed.status.success(), "remove: {}", stderr(&removed));
    assert!(stdout(&removed).is_empty(), "remove stdout must be empty");

    let last = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "remove",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
        ])
        .await;
    assert!(!last.status.success());
    assert!(
        stderr(&last).contains("last_active_actor_world_binding"),
        "stderr={}",
        stderr(&last)
    );
}

#[tokio::test]
async fn binding_show_retained_after_archive() {
    let d = LiveDaemon::start().await;
    activate_owner(&d).await;
    seed_character_sheet(&d, "kb_retained", WORLD_A, "retained").await;

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
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let binding_id = created_body["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let linked = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "0",
            "--world-sheet-entry-id",
            "kb_retained",
            "--json",
        ])
        .await;
    assert!(linked.status.success(), "link: {}", stderr(&linked));

    let archived = d
        .cli(&[
            "creator",
            "character",
            "archive",
            &chr,
            "--expected-revision",
            "0",
            "--json",
        ])
        .await;
    assert!(archived.status.success(), "archive: {}", stderr(&archived));

    let shown = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "show",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--json",
        ])
        .await;
    assert!(shown.status.success(), "retained show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["binding"]["world_sheet_entry_id"], "kb_retained");

    let denied = d
        .cli(&[
            "creator",
            "character",
            "binding",
            "edit",
            "--character-id",
            &chr,
            "--binding-id",
            &binding_id,
            "--expected-revision",
            "1",
            "--clear-world-sheet",
        ])
        .await;
    assert!(!denied.status.success());
    assert!(
        stderr(&denied).contains("character_inactive"),
        "stderr={}",
        stderr(&denied)
    );
}


#[tokio::test]
async fn knowledge_show_edit_remove_summary_journey() {
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
    let chr = body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

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
            "CliFact",
            "--summary",
            "alpha summary",
            "--json",
        ])
        .await;
    assert!(added.status.success(), "add: {}", stderr(&added));
    let added_body: Value = serde_json::from_str(&stdout(&added)).unwrap();
    let entry_id = added_body["item"]["entry_id"]
        .as_str()
        .unwrap()
        .to_string();
    let revision = added_body["item"]["revision"].as_u64().unwrap();

    let shown = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "show",
            "--character-id",
            &chr,
            "--entry-id",
            &entry_id,
            "--json",
        ])
        .await;
    assert!(shown.status.success(), "show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["summary"], "alpha summary");

    let edited = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "edit",
            "--character-id",
            &chr,
            "--entry-id",
            &entry_id,
            "--expected-revision",
            &revision.to_string(),
            "--summary",
            "beta summary",
            "--json",
        ])
        .await;
    assert!(edited.status.success(), "edit: {}", stderr(&edited));
    let edit_body: Value = serde_json::from_str(&stdout(&edited)).unwrap();
    assert_eq!(edit_body["summary"], "beta summary");
    let rev2 = edit_body["item"]["revision"].as_u64().unwrap();

    let cleared = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "edit",
            "--character-id",
            &chr,
            "--entry-id",
            &entry_id,
            "--expected-revision",
            &rev2.to_string(),
            "--clear-summary",
            "--json",
        ])
        .await;
    assert!(cleared.status.success(), "clear: {}", stderr(&cleared));
    let clear_body: Value = serde_json::from_str(&stdout(&cleared)).unwrap();
    assert!(clear_body["summary"].is_null());
    let rev3 = clear_body["item"]["revision"].as_u64().unwrap();

    let removed = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "remove",
            "--character-id",
            &chr,
            "--entry-id",
            &entry_id,
            "--expected-revision",
            &rev3.to_string(),
        ])
        .await;
    assert!(removed.status.success(), "remove: {}", stderr(&removed));
}

#[tokio::test]
async fn summary_file_over_byte_limit_rejects_via_metadata_precheck() {
    use nexus_local_db::ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES;
    use std::io::Write;

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
    let chr = body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let oversized = d.home.path().join("oversized-summary.txt");
    {
        let mut file = std::fs::File::create(&oversized).unwrap();
        file.write_all(b"x").unwrap();
        file.set_len((ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES + 1) as u64)
            .unwrap();
    }

    let started = std::time::Instant::now();
    let out = d
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
            "OversizedSummary",
            "--summary-file",
            oversized.to_str().unwrap(),
        ])
        .await;
    assert!(!out.status.success(), "expected failure: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("exceeding the 65536-byte limit"),
        "stderr={err}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "oversized summary-file must fail fast via metadata precheck"
    );
}

