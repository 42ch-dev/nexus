//! RN-ACT-4 multi-World no-copy fixture (P1 Task 4; reused by P2–P4).
//!
//! Seeds one Creator, Characters A/B, Worlds W1/W2 plus a later W3 binding,
//! and one KE of each owner scope through public HTTP and CLI only.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use super::LiveDaemon;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Output;

pub const NAME_W1_PUBLIC: &str = "W1Public";
pub const NAME_W1_SECRET: &str = "W1Secret";
pub const NAME_W2_PUBLIC: &str = "W2Public";
pub const NAME_A_SHARE: &str = "AShare";
pub const NAME_B_SHARE: &str = "BShare";
pub const NAME_A_W1_LOCAL: &str = "AW1Local";

/// Character-private arm entry authored through the `creator character
/// knowledge add --audience character-private` CLI surface (v1.191 P1 T9).
pub const NAME_A_PRIVATE: &str = "APrivate";

/// Coherent `ctr_…` creator every `rn_act4` journey boots with.
pub const FIXTURE_CREATOR: &str = "ctr_rnact4fixture01";

/// Seeded graph ids and `KnowledgeEntry` identities.
#[derive(Debug, Clone)]
pub struct RnAct4Graph {
    pub creator_id: String,
    pub world_w1: String,
    pub world_w2: String,
    pub world_w3: String,
    pub character_a: String,
    pub character_b: String,
    pub bind_a_w1: String,
    pub bind_a_w2: String,
    pub bind_a_w3: String,
    pub bind_b_w1: String,
    pub ke_w1_public: String,
    pub ke_w1_secret: String,
    pub ke_w2_public: String,
    pub ke_a_share: String,
    pub ke_b_share: String,
    pub ke_a_w1_local: String,
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

async fn http_json(d: &LiveDaemon, method: reqwest::Method, path: &str, body: Value) -> Value {
    let method_label = method.clone();
    let resp = reqwest::Client::new()
        .request(method, format!("{}{path}", d.http_url))
        .json(&body)
        .send()
        .await
        .expect("http send");
    let status = resp.status();
    let text = resp.text().await.expect("http body");
    assert!(
        status.is_success(),
        "{method_label} {path} -> {status}: {text}"
    );
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("json from {path}: {text}"))
}

async fn cli_ok(d: &LiveDaemon, args: &[&str]) -> Output {
    let out = d.cli(args).await;
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    out
}

async fn cli_json(d: &LiveDaemon, args: &[&str]) -> Value {
    let out = cli_ok(d, args).await;
    serde_json::from_str(&stdout(&out))
        .unwrap_or_else(|_| panic!("cli json {args:?}: {}", stdout(&out)))
}

fn entry_id(value: &Value) -> String {
    value["item"]["entry_id"].as_str().unwrap().to_string()
}

/// Exercise `POST /v1/daemon/creators` and return the fixture's active creator.
///
/// The graph is built against the creator the daemon opened at boot
/// (`test_creator` for `LiveDaemon::start()`). Switching selection after boot
/// — via CLI `system config set` or even `PUT /creators/active` — leaves
/// `CoreService`'s open context stale (`verify_selected_context` →
/// `auth_required`), so this helper keeps the coherent boot identity and only
/// smoke-tests public creator registration.
pub async fn activate_creator(d: &LiveDaemon) -> String {
    let created = http_json(
        d,
        reqwest::Method::POST,
        "/v1/daemon/creators",
        json!({ "display_name": "RN-ACT-4 Creator" }),
    )
    .await;
    let registered = created["creator_id"].as_str().unwrap().to_string();
    assert!(
        registered.starts_with("ctr_"),
        "public create must return CreatorId, got {registered}"
    );

    let active = http_json(
        d,
        reqwest::Method::GET,
        "/v1/daemon/creators/active",
        json!({}),
    )
    .await;
    active["creator_id"]
        .as_str()
        .expect("active creator")
        .to_string()
}

/// Build the full RN-ACT-4 graph through public HTTP routes and CLI verbs.
#[allow(clippy::too_many_lines, clippy::similar_names)] // single full-graph fixture (A/B roles)
pub async fn seed(d: &LiveDaemon) -> RnAct4Graph {
    let creator_id = activate_creator(d).await;

    let w1 = http_json(
        d,
        reqwest::Method::POST,
        "/v1/daemon/worlds",
        json!({ "title": "RN-ACT-4 World One" }),
    )
    .await;
    let w2 = http_json(
        d,
        reqwest::Method::POST,
        "/v1/daemon/worlds",
        json!({ "title": "RN-ACT-4 World Two" }),
    )
    .await;
    let world_w1 = w1["world_id"].as_str().unwrap().to_string();
    let world_w2 = w2["world_id"].as_str().unwrap().to_string();

    let created_a = cli_json(
        d,
        &[
            "creator",
            "character",
            "create",
            "--display-name",
            "Ava",
            "--world-id",
            &world_w1,
            "--json",
        ],
    )
    .await;
    let character_a = created_a["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let bind_a_w1 = created_a["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_b = http_json(
        d,
        reqwest::Method::POST,
        "/v1/daemon/characters",
        json!({ "display_name": "Ben", "world_id": world_w1 }),
    )
    .await;
    let character_b = created_b["character"]["character_id"]
        .as_str()
        .unwrap()
        .to_string();
    let bind_b_w1 = created_b["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let bound_a_w2 = cli_json(
        d,
        &[
            "creator",
            "character",
            "binding",
            "add",
            "--character-id",
            &character_a,
            "--world-id",
            &world_w2,
            "--json",
        ],
    )
    .await;
    let bind_a_w2 = bound_a_w2["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    let ke_w1_public = entry_id(
        &cli_json(
            d,
            &[
                "creator",
                "character",
                "knowledge",
                "add",
                "--owner",
                "world",
                "--world-id",
                &world_w1,
                "--block-type",
                "item",
                "--canonical-name",
                NAME_W1_PUBLIC,
                "--json",
            ],
        )
        .await,
    );

    let ke_w1_secret = entry_id(
        &http_json(
            d,
            reqwest::Method::POST,
            "/v1/daemon/actor-knowledge/entries",
            json!({
                "owner_kind": "world",
                "world_id": world_w1,
                "block_type": "item",
                "canonical_name": NAME_W1_SECRET,
                "audience": { "kind": "author-only" }
            }),
        )
        .await,
    );

    let ke_w2_public = entry_id(
        &cli_json(
            d,
            &[
                "creator",
                "character",
                "knowledge",
                "add",
                "--owner",
                "world",
                "--world-id",
                &world_w2,
                "--block-type",
                "item",
                "--canonical-name",
                NAME_W2_PUBLIC,
                "--json",
            ],
        )
        .await,
    );

    let ke_a_share = entry_id(
        &cli_json(
            d,
            &[
                "creator",
                "character",
                "knowledge",
                "add",
                "--owner",
                "character",
                "--character-id",
                &character_a,
                "--block-type",
                "item",
                "--canonical-name",
                NAME_A_SHARE,
                "--json",
            ],
        )
        .await,
    );

    let ke_b_share = entry_id(
        &http_json(
            d,
            reqwest::Method::POST,
            "/v1/daemon/actor-knowledge/entries",
            json!({
                "owner_kind": "character",
                "character_id": character_b,
                "block_type": "item",
                "canonical_name": NAME_B_SHARE
            }),
        )
        .await,
    );

    let ke_a_w1_local = entry_id(
        &cli_json(
            d,
            &[
                "creator",
                "character",
                "knowledge",
                "add",
                "--owner",
                "binding",
                "--character-id",
                &character_a,
                "--binding-id",
                &bind_a_w1,
                "--world-id",
                &world_w1,
                "--block-type",
                "item",
                "--canonical-name",
                NAME_A_W1_LOCAL,
                "--json",
            ],
        )
        .await,
    );

    let w3 = http_json(
        d,
        reqwest::Method::POST,
        "/v1/daemon/worlds",
        json!({ "title": "RN-ACT-4 World Three Later" }),
    )
    .await;
    let world_w3 = w3["world_id"].as_str().unwrap().to_string();
    let bound_a_w3 = cli_json(
        d,
        &[
            "creator",
            "character",
            "binding",
            "add",
            "--character-id",
            &character_a,
            "--world-id",
            &world_w3,
            "--json",
        ],
    )
    .await;
    let bind_a_w3 = bound_a_w3["binding"]["binding_id"]
        .as_str()
        .unwrap()
        .to_string();

    RnAct4Graph {
        creator_id,
        world_w1,
        world_w2,
        world_w3,
        character_a,
        character_b,
        bind_a_w1,
        bind_a_w2,
        bind_a_w3,
        bind_b_w1,
        ke_w1_public,
        ke_w1_secret,
        ke_w2_public,
        ke_a_share,
        ke_b_share,
        ke_a_w1_local,
    }
}

pub async fn view_character_cli(
    d: &LiveDaemon,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> Value {
    cli_json(
        d,
        &[
            "creator",
            "character",
            "knowledge",
            "view",
            "--actor",
            "character",
            "--character-id",
            character_id,
            "--world-id",
            world_id,
            "--binding-id",
            binding_id,
            "--json",
        ],
    )
    .await
}

pub async fn view_creator_cli(d: &LiveDaemon, creator_id: &str, world_id: &str) -> Value {
    cli_json(
        d,
        &[
            "creator",
            "character",
            "knowledge",
            "view",
            "--actor",
            "creator",
            "--creator-id",
            creator_id,
            "--world-id",
            world_id,
            "--json",
        ],
    )
    .await
}

/// The `entry_id` set one Character reads for a (world, binding) viewpoint.
pub async fn view_character_ids(
    d: &LiveDaemon,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> BTreeSet<String> {
    entry_ids(&view_character_cli(d, character_id, world_id, binding_id).await)
}

/// Index a view page by stable `entry_id`. Duplicate ids mean a copied row.
pub fn page_index(page: &Value) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for item in page["items"].as_array().unwrap() {
        let id = item["entry_id"].as_str().unwrap().to_string();
        assert!(
            map.insert(id.clone(), item.clone()).is_none(),
            "duplicate entry_id {id} (copied KnowledgeEntry row)"
        );
    }
    map
}

pub fn entry_ids(page: &Value) -> BTreeSet<String> {
    page_index(page).into_keys().collect()
}

pub fn expected_ids(ids: &[String]) -> BTreeSet<String> {
    ids.iter().cloned().collect()
}

/// Fixture-name lookup. Owner-scoped duplicate names are allowed; this helper
/// is only for this fixture's unique display names.
pub fn named_item<'a>(index: &'a BTreeMap<String, Value>, canonical_name: &str) -> &'a Value {
    let matches: Vec<&Value> = index
        .values()
        .filter(|item| item["canonical_name"] == canonical_name)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {canonical_name} in this fixture page"
    );
    matches[0]
}

// ── v1.191 P1 T9: authored audience (holder governance) ──────────────────────
//
// The retired World-only `creator_only` boolean is gone: an audience is the
// closed `{kind}` pair the CLI maps onto the frozen wire object, and the
// governance pair (`holder_entry_id` / `disclosure`) is what the read surfaces
// project. These helpers drive the shipped CLI verbs so the dogfood journey
// exercises the public authoring path, never a request-shaped shortcut.

/// The `knowledge_holders` registry row id of one stored Creator.
pub async fn creator_holder(d: &LiveDaemon, creator_id: &str) -> String {
    sqlx::query_scalar("SELECT holder_entry_id FROM knowledge_holders WHERE creator_id = ?")
        .bind(creator_id)
        .fetch_one(&d.pool)
        .await
        .expect("stored Creator holder")
}

/// The `knowledge_holders` registry row id of one stored Character.
pub async fn character_holder(d: &LiveDaemon, character_id: &str) -> String {
    sqlx::query_scalar("SELECT holder_entry_id FROM knowledge_holders WHERE character_id = ?")
        .bind(character_id)
        .fetch_one(&d.pool)
        .await
        .expect("stored Character holder")
}

/// Author one Character-owned entry with a `character-private` audience through
/// `creator character knowledge add --audience character-private`, returning the
/// new `entry_id`.
pub async fn add_character_private(
    d: &LiveDaemon,
    character_id: &str,
    canonical_name: &str,
) -> String {
    entry_id(
        &cli_json(
            d,
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
                "item",
                "--canonical-name",
                canonical_name,
                "--audience",
                "character-private",
                "--audience-character",
                character_id,
                "--json",
            ],
        )
        .await,
    )
}

/// Move one entry's audience through `creator character knowledge edit
/// --audience <…>` under the caller's revision, returning the detail page.
pub async fn edit_audience(
    d: &LiveDaemon,
    character_id: &str,
    entry_id: &str,
    expected_revision: u64,
    audience: &str,
    audience_character: Option<&str>,
) -> Value {
    let expected_revision = expected_revision.to_string();
    let mut args = vec![
        "creator",
        "character",
        "knowledge",
        "edit",
        "--character-id",
        character_id,
        "--entry-id",
        entry_id,
        "--expected-revision",
        &expected_revision,
        "--audience",
        audience,
    ];
    if let Some(target) = audience_character {
        args.push("--audience-character");
        args.push(target);
    }
    args.push("--json");
    cli_json(d, &args).await
}

/// Author one World KB entity's audience through `creator world kb entity patch
/// --audience <…>` under the caller's version, returning the patch response.
pub async fn patch_entity_audience(
    d: &LiveDaemon,
    world_id: &str,
    entity_id: &str,
    expected_version: u64,
    audience: &str,
    audience_character: Option<&str>,
) -> Value {
    let expected_version = expected_version.to_string();
    let mut args = vec![
        "creator",
        "world",
        "kb",
        "entity",
        "patch",
        "--world-id",
        world_id,
        "--entity-id",
        entity_id,
        "--expected-version",
        &expected_version,
        "--audience",
        audience,
    ];
    if let Some(target) = audience_character {
        args.push("--audience-character");
        args.push(target);
    }
    args.push("--json");
    cli_json(d, &args).await
}

/// One entity of `creator world kb graph --json` by `key_block_id`.
pub fn graph_entity<'a>(graph: &'a Value, entity_id: &str) -> &'a Value {
    graph["entities"]
        .as_array()
        .expect("graph entities")
        .iter()
        .find(|entity| entity["key_block_id"] == entity_id)
        .unwrap_or_else(|| panic!("{entity_id} missing from the world KB graph"))
}

/// `creator world kb graph --world-id <…> --json` for one World.
pub async fn world_kb_graph(d: &LiveDaemon, world_id: &str) -> Value {
    cli_json(
        d,
        &[
            "creator",
            "world",
            "kb",
            "graph",
            "--world-id",
            world_id,
            "--json",
        ],
    )
    .await
}

/// The stored per-row version of one World KB entity, read through the shipped
/// graph projection (the governance half of an authoring transaction owns its
/// own bump, so the patch response's version is not the CAS preimage).
pub async fn entity_version(d: &LiveDaemon, world_id: &str, entity_id: &str) -> u64 {
    graph_entity(&world_kb_graph(d, world_id).await, entity_id)["version"]
        .as_u64()
        .expect("entity version")
}
