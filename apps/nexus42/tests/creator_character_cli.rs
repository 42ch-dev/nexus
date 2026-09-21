//! `nexus42 creator character` CLI tests.
//!
//! Identity, binding and knowledge arms (v1.193 P0-T9/T10) drive the REAL
//! binary against a hermetic direct-core home (`common/direct_actor.rs`): no
//! daemon, no Node child, the core's stored actor fences, the caller's
//! explicit `--expected-revision` CAS and the core's own admitted
//! holder-filtered knowledge view. The memory/ToM/run family keeps its own
//! test crates until P0-T11 migrates it.

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct_actor::DirectActor;
use serde_json::Value;
use std::process::Output;

/// `WorldSheet` `KeyBlock` ids the direct-core fixture seeds (stored `kb_<hex>`).
const SHEET_A: &str = "kb_5ee70001";
const SHEET_B: &str = "kb_5ee70002";
const SHEET_RETAINED: &str = "kb_5ee70003";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_bind_remove_journey_human_and_json() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Journey World A").await;
    let world_b = actor.create_world("Journey World B").await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
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

    let human = actor.cli(&["creator", "character", "show", &chr]);
    assert!(human.status.success(), "show human: {}", stderr(&human));
    assert!(stdout(&human).contains(&chr));
    assert!(stdout(&human).contains("Ava"));
    assert!(!stdout(&human).trim_start().starts_with('{'));

    let json_show = actor.cli(&["creator", "character", "show", &chr, "--json"]);
    assert!(
        json_show.status.success(),
        "show json: {}",
        stderr(&json_show)
    );
    let shown: Value = serde_json::from_str(&stdout(&json_show)).unwrap();
    assert_eq!(shown["character"]["character_id"], chr);

    let listed = actor.cli(&["creator", "character", "list", "--json"]);
    assert!(listed.status.success(), "list: {}", stderr(&listed));
    let list_json: Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(list_json["items"].as_array().unwrap().len(), 1);

    let bound = actor.cli(&[
        "creator",
        "character",
        "binding",
        "add",
        "--character-id",
        &chr,
        "--world-id",
        &world_b,
        "--json",
    ]);
    assert!(bound.status.success(), "bind: {}", stderr(&bound));
    let bound_json: Value = serde_json::from_str(&stdout(&bound)).unwrap();
    let second = bound_json["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let last = actor.cli(&[
        "creator",
        "character",
        "binding",
        "remove",
        "--character-id",
        &chr,
        "--binding-id",
        &first_binding,
    ]);
    assert!(last.status.success(), "non-last remove: {}", stderr(&last));

    let fail_closed = actor.cli(&[
        "creator",
        "character",
        "binding",
        "remove",
        "--character-id",
        &chr,
        "--binding-id",
        &second,
    ]);
    assert!(!fail_closed.status.success());
    assert!(
        stderr(&fail_closed).contains("last_active_actor_world_binding"),
        "stderr={}",
        stderr(&fail_closed)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines, clippy::similar_names)] // e2e pagination proof (A/B pages)
async fn binding_list_human_and_json_paginate() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Pagination World A").await;
    let world_b = actor.create_world("Pagination World B").await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_json: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_json["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let bound = actor.cli(&[
        "creator",
        "character",
        "binding",
        "add",
        "--character-id",
        &chr,
        "--world-id",
        &world_b,
        "--json",
    ]);
    assert!(bound.status.success(), "bind: {}", stderr(&bound));

    let json_page = actor.cli(&[
        "creator",
        "character",
        "binding",
        "list",
        "--character-id",
        &chr,
        "--limit",
        "1",
        "--json",
    ]);
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

    let json_page2 = actor.cli(&[
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
    ]);
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

    let human = actor.cli(&[
        "creator",
        "character",
        "binding",
        "list",
        "--character-id",
        &chr,
        "--limit",
        "1",
    ]);
    assert!(human.status.success(), "human list: {}", stderr(&human));
    let human_out = stdout(&human);
    assert!(!human_out.trim_start().starts_with('{'));
    let human_cursor = human_out
        .lines()
        .find_map(|line| line.strip_prefix("next_cursor: "))
        .expect("human next_cursor")
        .to_string();

    let human2 = actor.cli(&[
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
    ]);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn knowledge_add_list_view_json_round_trip() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Knowledge Round Trip World").await;
    let seeded = actor.create_character("Ava", &world_a).await;

    let added = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "add",
        "--owner",
        "character",
        "--character-id",
        &seeded.character_id,
        "--block-type",
        "item",
        "--canonical-name",
        "CharNote",
        "--json",
    ]);
    assert!(added.status.success(), "add: {}", stderr(&added));
    let added_body: Value = serde_json::from_str(&stdout(&added)).unwrap();
    assert_eq!(added_body["item"]["canonical_name"], "CharNote");

    let listed = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "list",
        "--character-id",
        &seeded.character_id,
        "--json",
    ]);
    assert!(listed.status.success(), "list: {}", stderr(&listed));
    let list_body: Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(list_body["items"].as_array().unwrap().len(), 1);

    let viewed = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "view",
        "--actor",
        "character",
        "--character-id",
        &seeded.character_id,
        "--world-id",
        &world_a,
        "--binding-id",
        &seeded.binding_id,
        "--json",
    ]);
    assert!(viewed.status.success(), "view: {}", stderr(&viewed));
    let view_body: Value = serde_json::from_str(&stdout(&viewed)).unwrap();
    assert_eq!(view_body["items"][0]["canonical_name"], "CharNote");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)]
async fn edit_archive_restore_cli_honors_explicit_revision_cas() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Edit World A").await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--image-uri",
        "https://example.test/ava.png",
        "--persona",
        "{\"role\":\"scout\"}",
        "--json",
    ]);
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let edited = actor.cli(&[
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
    ]);
    assert!(edited.status.success(), "edit: {}", stderr(&edited));
    let edit_body: Value = serde_json::from_str(&stdout(&edited)).unwrap();
    assert_eq!(edit_body["character"]["display_name"], "Ada");
    assert_eq!(edit_body["character"]["revision"], 1);
    assert!(
        edit_body["character"]["image_uri"].is_null(),
        "cleared image_uri should be null or omitted: {}",
        edit_body["character"]["image_uri"]
    );
    assert_eq!(edit_body["character"]["persona"]["role"], "scout");

    let stale = actor.cli(&[
        "creator",
        "character",
        "edit",
        &chr,
        "--expected-revision",
        "0",
        "--display-name",
        "Stale",
    ]);
    assert!(!stale.status.success(), "stale edit must fail");
    assert!(stderr(&stale).contains("character_revision_conflict"));

    let archived = actor.cli(&[
        "creator",
        "character",
        "archive",
        &chr,
        "--expected-revision",
        "1",
        "--json",
    ]);
    assert!(archived.status.success(), "archive: {}", stderr(&archived));
    let arch_body: Value = serde_json::from_str(&stdout(&archived)).unwrap();
    assert_eq!(arch_body["character"]["status"], "archived");

    let write_denied = actor.cli(&[
        "creator",
        "character",
        "edit",
        &chr,
        "--expected-revision",
        "2",
        "--display-name",
        "Nope",
    ]);
    assert!(!write_denied.status.success());
    assert!(stderr(&write_denied).contains("character_inactive"));

    let restored = actor.cli(&[
        "creator",
        "character",
        "restore",
        &chr,
        "--expected-revision",
        "2",
        "--json",
    ]);
    assert!(restored.status.success(), "restore: {}", stderr(&restored));
    let restore_body: Value = serde_json::from_str(&stdout(&restored)).unwrap();
    assert_eq!(restore_body["character"]["status"], "active");
    assert_eq!(restore_body["character"]["character_id"], chr);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edit_without_mutable_fields_is_invalid_input() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Empty Edit World").await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    let empty = actor.cli(&[
        "creator",
        "character",
        "edit",
        &chr,
        "--expected-revision",
        "0",
    ]);
    assert!(!empty.status.success(), "empty edit must fail");
    let err = stderr(&empty);
    assert!(
        err.contains("at least one mutable field") || err.contains("invalid_input"),
        "unexpected stderr: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unrepresentable_expected_revision_is_refused_not_wrapped() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Revision Bound World").await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
    assert!(created.status.success(), "create: {}", stderr(&created));
    let created_body: Value = serde_json::from_str(&stdout(&created)).unwrap();
    let chr = created_body["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();

    // `u64::MAX` exceeds every stored `i64` revision: the leaf must refuse it
    // rather than cast it into a negative revision that addresses a different
    // row revision.
    let overflow = actor.cli(&[
        "creator",
        "character",
        "edit",
        &chr,
        "--expected-revision",
        "18446744073709551615",
        "--display-name",
        "Wrapped",
    ]);
    assert!(!overflow.status.success(), "overflow must be refused");
    assert!(
        stderr(&overflow).contains("out of range for i64"),
        "stderr={}",
        stderr(&overflow)
    );

    let unchanged = actor.cli(&["creator", "character", "show", &chr, "--json"]);
    assert!(unchanged.status.success(), "show: {}", stderr(&unchanged));
    let shown: Value = serde_json::from_str(&stdout(&unchanged)).unwrap();
    assert_eq!(shown["character"]["display_name"], "Ava");
    assert_eq!(shown["character"]["revision"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)]
async fn binding_detail_edit_link_relink_clear_and_refusals() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Binding Detail World A").await;
    let world_b = actor.create_world("Binding Detail World B").await;
    actor
        .seed_world_character_sheet(SHEET_A, &world_a, "sheet_a")
        .await;
    actor
        .seed_world_character_sheet(SHEET_B, &world_a, "sheet_b")
        .await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
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

    let shown = actor.cli(&[
        "creator",
        "character",
        "binding",
        "show",
        "--character-id",
        &chr,
        "--binding-id",
        &binding_id,
        "--json",
    ]);
    assert!(shown.status.success(), "show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["binding"]["revision"], 0);

    let linked = actor.cli(&[
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
        SHEET_A,
        "--json",
    ]);
    assert!(linked.status.success(), "link: {}", stderr(&linked));
    let linked_body: Value = serde_json::from_str(&stdout(&linked)).unwrap();
    assert_eq!(linked_body["binding"]["revision"], 1);
    assert_eq!(linked_body["binding"]["world_sheet_entry_id"], SHEET_A);

    let relinked = actor.cli(&[
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
        SHEET_B,
        "--json",
    ]);
    assert!(relinked.status.success(), "relink: {}", stderr(&relinked));
    let relink_body: Value = serde_json::from_str(&stdout(&relinked)).unwrap();
    assert_eq!(relink_body["binding"]["revision"], 2);

    let cleared = actor.cli(&[
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
    ]);
    assert!(cleared.status.success(), "clear: {}", stderr(&cleared));
    let clear_body: Value = serde_json::from_str(&stdout(&cleared)).unwrap();
    assert_eq!(clear_body["binding"]["revision"], 3);
    assert!(clear_body["binding"]["world_sheet_entry_id"].is_null());

    let stale = actor.cli(&[
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
    ]);
    assert!(!stale.status.success());
    assert!(
        stderr(&stale).contains("binding_revision_conflict"),
        "stderr={}",
        stderr(&stale)
    );

    let second = actor.cli(&[
        "creator",
        "character",
        "binding",
        "add",
        "--character-id",
        &chr,
        "--world-id",
        &world_b,
        "--json",
    ]);
    assert!(second.status.success(), "add second: {}", stderr(&second));
    let second_body: Value = serde_json::from_str(&stdout(&second)).unwrap();
    let second_binding = second_body["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let removed = actor.cli(&[
        "creator",
        "character",
        "binding",
        "remove",
        "--character-id",
        &chr,
        "--binding-id",
        &second_binding,
    ]);
    assert!(removed.status.success(), "remove: {}", stderr(&removed));
    assert!(stdout(&removed).is_empty(), "remove stdout must be empty");

    let last = actor.cli(&[
        "creator",
        "character",
        "binding",
        "remove",
        "--character-id",
        &chr,
        "--binding-id",
        &binding_id,
    ]);
    assert!(!last.status.success());
    assert!(
        stderr(&last).contains("last_active_actor_world_binding"),
        "stderr={}",
        stderr(&last)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binding_show_retained_after_archive() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Retained Binding World").await;
    actor
        .seed_world_character_sheet(SHEET_RETAINED, &world_a, "retained")
        .await;

    let created = actor.cli(&[
        "creator",
        "character",
        "create",
        "--display-name",
        "Ava",
        "--world-id",
        &world_a,
        "--json",
    ]);
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

    let linked = actor.cli(&[
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
        SHEET_RETAINED,
        "--json",
    ]);
    assert!(linked.status.success(), "link: {}", stderr(&linked));

    let archived = actor.cli(&[
        "creator",
        "character",
        "archive",
        &chr,
        "--expected-revision",
        "0",
        "--json",
    ]);
    assert!(archived.status.success(), "archive: {}", stderr(&archived));

    let shown = actor.cli(&[
        "creator",
        "character",
        "binding",
        "show",
        "--character-id",
        &chr,
        "--binding-id",
        &binding_id,
        "--json",
    ]);
    assert!(shown.status.success(), "retained show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["binding"]["world_sheet_entry_id"], SHEET_RETAINED);

    let denied = actor.cli(&[
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
    ]);
    assert!(!denied.status.success());
    assert!(
        stderr(&denied).contains("character_inactive"),
        "stderr={}",
        stderr(&denied)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)]
async fn knowledge_show_edit_remove_summary_journey() {
    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Summary Journey World").await;
    let seeded = actor.create_character("Ava", &world_a).await;
    let chr = seeded.character_id.as_str();

    let added = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "add",
        "--owner",
        "character",
        "--character-id",
        chr,
        "--block-type",
        "item",
        "--canonical-name",
        "CliFact",
        "--summary",
        "alpha summary",
        "--json",
    ]);
    assert!(added.status.success(), "add: {}", stderr(&added));
    let added_body: Value = serde_json::from_str(&stdout(&added)).unwrap();
    let entry_id = added_body["item"]["entry_id"].as_str().unwrap().to_string();
    let revision = added_body["item"]["revision"].as_u64().unwrap();

    let shown = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "show",
        "--character-id",
        chr,
        "--entry-id",
        &entry_id,
        "--json",
    ]);
    assert!(shown.status.success(), "show: {}", stderr(&shown));
    let show_body: Value = serde_json::from_str(&stdout(&shown)).unwrap();
    assert_eq!(show_body["summary"], "alpha summary");

    let edited = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "edit",
        "--character-id",
        chr,
        "--entry-id",
        &entry_id,
        "--expected-revision",
        &revision.to_string(),
        "--summary",
        "beta summary",
        "--json",
    ]);
    assert!(edited.status.success(), "edit: {}", stderr(&edited));
    let edit_body: Value = serde_json::from_str(&stdout(&edited)).unwrap();
    assert_eq!(edit_body["summary"], "beta summary");
    let rev2 = edit_body["item"]["revision"].as_u64().unwrap();

    let cleared = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "edit",
        "--character-id",
        chr,
        "--entry-id",
        &entry_id,
        "--expected-revision",
        &rev2.to_string(),
        "--clear-summary",
        "--json",
    ]);
    assert!(cleared.status.success(), "clear: {}", stderr(&cleared));
    let clear_body: Value = serde_json::from_str(&stdout(&cleared)).unwrap();
    assert!(clear_body["summary"].is_null());
    let rev3 = clear_body["item"]["revision"].as_u64().unwrap();

    // The stored revision advanced with each write, so the pre-edit revision is
    // now stale: the CAS must refuse it instead of deleting the row.
    let stale = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "remove",
        "--character-id",
        chr,
        "--entry-id",
        &entry_id,
        "--expected-revision",
        &revision.to_string(),
    ]);
    assert!(!stale.status.success(), "stale remove must be refused");
    assert!(
        stderr(&stale).contains("knowledge_revision_conflict"),
        "stderr={}",
        stderr(&stale)
    );

    let removed = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "remove",
        "--character-id",
        chr,
        "--entry-id",
        &entry_id,
        "--expected-revision",
        &rev3.to_string(),
    ]);
    assert!(removed.status.success(), "remove: {}", stderr(&removed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn summary_file_over_byte_limit_rejects_via_metadata_precheck() {
    use nexus_local_db::ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES;
    use std::io::Write;

    let actor = DirectActor::new().await;
    let world_a = actor.create_world("Oversized Summary World").await;
    let seeded = actor.create_character("Ava", &world_a).await;

    let oversized = actor.home().join("oversized-summary.txt");
    {
        let mut file = std::fs::File::create(&oversized).unwrap();
        file.write_all(b"x").unwrap();
        file.set_len((ACTOR_KNOWLEDGE_SUMMARY_MAX_UTF8_BYTES + 1) as u64)
            .unwrap();
    }

    let out = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "add",
        "--owner",
        "character",
        "--character-id",
        &seeded.character_id,
        "--block-type",
        "item",
        "--canonical-name",
        "OversizedSummary",
        "--summary-file",
        oversized.to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "expected failure: {}", stderr(&out));
    // The precheck wording is emitted **only** by the metadata precheck in
    // `read_file_bounded` (`cannot read …` is the read failure), so naming the
    // metadata size is direct evidence of the precheck path. The v1.193
    // direct-core seam admits the writer before this leaf validates, so the
    // refusal is no longer observable as a start-to-error elapsed time.
    let err = stderr(&out);
    assert!(
        err.contains("exceeding the 65536-byte limit"),
        "stderr={err}"
    );
    assert!(
        err.contains("is 65537 bytes"),
        "the refusal must name the metadata size, not a post-read length: {err}"
    );

    // The refusal is zero-mutation: nothing reached the stored rows.
    let listed = actor.cli(&[
        "creator",
        "character",
        "knowledge",
        "list",
        "--character-id",
        &seeded.character_id,
        "--json",
    ]);
    assert!(listed.status.success(), "list: {}", stderr(&listed));
    let listed_body: Value = serde_json::from_str(&stdout(&listed)).unwrap();
    assert_eq!(
        listed_body["items"].as_array().unwrap().len(),
        0,
        "an oversized summary-file must not store a row"
    );
}
