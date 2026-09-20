//! RN-ACT-4 multi-World no-copy dogfood — direct-core CLI (v1.193 P0-T10).
//!
//! The whole journey runs the REAL `nexus42` binary against the hermetic
//! direct-core actor fixture ([`direct_actor::DirectActor`]): no daemon
//! fixture, no HTTP client, no Node child. Every KnowledgeEntry is authored
//! through the shipped `creator character knowledge` / `creator world kb`
//! verbs, so the five viewpoints below are the core's own admitted projections
//! — a Character viewpoint stays the strict holder-filtered view, the Creator
//! viewpoint the management review — and the stored rows are read back from
//! the released workspace DB to prove the views share one durable row instead
//! of a copy.

#[path = "common/direct.rs"]
mod direct;
#[path = "common/direct_actor.rs"]
mod direct_actor;

use direct_actor::DirectActor;
use nexus_knowledge::world_kb::knowledge_entry::DISCLOSURE_OWNER_PRIVATE;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Output;

const NAME_W1_PUBLIC: &str = "W1Public";
const NAME_W1_SECRET: &str = "W1Secret";
const NAME_W2_PUBLIC: &str = "W2Public";
const NAME_A_SHARE: &str = "AShare";
const NAME_B_SHARE: &str = "BShare";
const NAME_A_W1_LOCAL: &str = "AW1Local";

/// Character-private arm entry authored through the `creator character
/// knowledge add --audience character-private` CLI surface (v1.191 P1 T9).
const NAME_A_PRIVATE: &str = "APrivate";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Seeded graph ids and `KnowledgeEntry` identities.
struct RnAct4Graph {
    creator_id: String,
    world_w1: String,
    world_w2: String,
    world_w3: String,
    character_a: String,
    character_b: String,
    bind_a_w1: String,
    bind_a_w2: String,
    bind_a_w3: String,
    bind_b_w1: String,
    ke_w1_public: String,
    ke_w1_secret: String,
    ke_w2_public: String,
    ke_a_share: String,
    ke_b_share: String,
    ke_a_w1_local: String,
}

/// Run one CLI invocation and parse its `--json` stdout.
fn cli_json(actor: &DirectActor, args: &[&str]) -> Value {
    let out = actor.cli(args);
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    serde_json::from_str(&stdout(&out))
        .unwrap_or_else(|_| panic!("cli json {args:?}: {}", stdout(&out)))
}

/// The `entry_id` one create response reports.
fn created_entry_id(response: &Value) -> String {
    response["item"]["entry_id"]
        .as_str()
        .expect("created entry_id")
        .to_string()
}

/// Author one `KnowledgeEntry` through `creator character knowledge add`,
/// returning its new `entry_id`.
fn add_entry(
    actor: &DirectActor,
    owner_args: &[&str],
    canonical_name: &str,
    audience: Option<&str>,
) -> String {
    let mut args: Vec<String> = ["creator", "character", "knowledge", "add"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    args.extend(owner_args.iter().map(|arg| (*arg).to_string()));
    args.extend(
        [
            "--block-type",
            "item",
            "--canonical-name",
            canonical_name,
            "--json",
        ]
        .iter()
        .map(|arg| (*arg).to_string()),
    );
    if let Some(audience) = audience {
        args.push("--audience".to_string());
        args.push(audience.to_string());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    created_entry_id(&cli_json(actor, &refs))
}

/// Build the full RN-ACT-4 graph through the shipped CLI verbs and the
/// fixture's authorized core seeds.
async fn seed(actor: &DirectActor) -> RnAct4Graph {
    let creator_id = actor.creator_id().to_string();
    let world_w1 = actor.create_world("RN-ACT-4 World One").await;
    let world_w2 = actor.create_world("RN-ACT-4 World Two").await;

    let seeded_a = actor.create_character("Ava", &world_w1).await;
    let character_a = seeded_a.character_id;
    let bind_a_w1 = seeded_a.binding_id;
    let seeded_b = actor.create_character("Ben", &world_w1).await;
    let character_b = seeded_b.character_id;
    let bind_b_w1 = seeded_b.binding_id;

    let bind_a_w2 = actor.add_binding(&character_a, &world_w2).await;

    let ke_w1_public = add_entry(
        actor,
        &["--owner", "world", "--world-id", &world_w1],
        NAME_W1_PUBLIC,
        None,
    );
    let ke_w1_secret = add_entry(
        actor,
        &["--owner", "world", "--world-id", &world_w1],
        NAME_W1_SECRET,
        Some("author-only"),
    );
    let ke_w2_public = add_entry(
        actor,
        &["--owner", "world", "--world-id", &world_w2],
        NAME_W2_PUBLIC,
        None,
    );
    let ke_a_share = add_entry(
        actor,
        &["--owner", "character", "--character-id", &character_a],
        NAME_A_SHARE,
        None,
    );
    let ke_b_share = add_entry(
        actor,
        &["--owner", "character", "--character-id", &character_b],
        NAME_B_SHARE,
        None,
    );
    let ke_a_w1_local = add_entry(
        actor,
        &[
            "--owner",
            "binding",
            "--character-id",
            &character_a,
            "--binding-id",
            &bind_a_w1,
            "--world-id",
            &world_w1,
        ],
        NAME_A_W1_LOCAL,
        None,
    );

    let world_w3 = actor.create_world("RN-ACT-4 World Three Later").await;
    // The later binding is created last: a Character-owned entry is shared
    // across the Character's active bindings, so W3 must see it without a copy.
    let bind_a_w3 = actor.add_binding(&character_a, &world_w3).await;

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

/// One Character viewpoint through the shipped CLI (`--actor character`).
fn view_character_cli(
    actor: &DirectActor,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> Value {
    cli_json(
        actor,
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
}

/// The Creator management review through the shipped CLI (`--actor creator`).
fn view_creator_cli(actor: &DirectActor, creator_id: &str, world_id: &str) -> Value {
    cli_json(
        actor,
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
}

/// The `entry_id` set one Character reads for a (world, binding) viewpoint.
fn view_character_ids(
    actor: &DirectActor,
    character_id: &str,
    world_id: &str,
    binding_id: &str,
) -> BTreeSet<String> {
    entry_ids(&view_character_cli(actor, character_id, world_id, binding_id))
}

/// Index a view page by stable `entry_id`. Duplicate ids mean a copied row.
fn page_index(page: &Value) -> BTreeMap<String, Value> {
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

fn entry_ids(page: &Value) -> BTreeSet<String> {
    page_index(page).into_keys().collect()
}

fn expected_ids(ids: &[String]) -> BTreeSet<String> {
    ids.iter().cloned().collect()
}

/// Fixture-name lookup. Owner-scoped duplicate names are allowed; this helper
/// is only for this fixture's unique display names.
fn named_item<'a>(index: &'a BTreeMap<String, Value>, canonical_name: &str) -> &'a Value {
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

// ── stored-row reads (the released workspace DB) ────────────────────────────
//
// Every seed writer is released and each CLI child exits before these run, so
// the assertions read the same durable rows the CLI just wrote.

async fn read_only_pool(actor: &DirectActor) -> sqlx::SqlitePool {
    nexus_local_db::open_pool_read_only(&actor.state_db_path())
        .await
        .expect("open fixture state db read-only")
}

/// The `knowledge_holders` registry row id of one stored Creator.
async fn creator_holder(actor: &DirectActor, creator_id: &str) -> String {
    let pool = read_only_pool(actor).await;
    let holder = sqlx::query_scalar("SELECT holder_entry_id FROM knowledge_holders WHERE creator_id = ?")
        .bind(creator_id)
        .fetch_one(&pool)
        .await
        .expect("stored Creator holder");
    pool.close().await;
    holder
}

/// The `knowledge_holders` registry row id of one stored Character.
async fn character_holder(actor: &DirectActor, character_id: &str) -> String {
    let pool = read_only_pool(actor).await;
    let holder = sqlx::query_scalar(
        "SELECT holder_entry_id FROM knowledge_holders WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_one(&pool)
    .await
    .expect("stored Character holder");
    pool.close().await;
    holder
}

/// Author one Character-owned entry with a `character-private` audience through
/// `creator character knowledge add --audience character-private`, returning the
/// new `entry_id`.
fn add_character_private(actor: &DirectActor, character_id: &str, canonical_name: &str) -> String {
    created_entry_id(&cli_json(
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
            "item",
            "--canonical-name",
            canonical_name,
            "--audience",
            "character-private",
            "--audience-character",
            character_id,
            "--json",
        ],
    ))
}

/// Move one entry's audience through `creator character knowledge edit
/// --audience <…>` under the caller's revision, returning the detail page.
fn edit_audience(
    actor: &DirectActor,
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
    cli_json(actor, &args)
}

/// Author one World KB entity's audience through `creator world kb entity patch
/// --audience <…>` under the caller's version, returning the patch response.
fn patch_entity_audience(
    actor: &DirectActor,
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
    cli_json(actor, &args)
}

/// One entity of `creator world kb graph --json` by `key_block_id`.
fn graph_entity<'a>(graph: &'a Value, entity_id: &str) -> &'a Value {
    graph["entities"]
        .as_array()
        .expect("graph entities")
        .iter()
        .find(|entity| entity["key_block_id"] == entity_id)
        .unwrap_or_else(|| panic!("{entity_id} missing from the world KB graph"))
}

/// `creator world kb graph --world-id <…> --json` for one World.
fn world_kb_graph(actor: &DirectActor, world_id: &str) -> Value {
    cli_json(
        actor,
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
}

/// The stored per-row version of one World KB entity, read through the shipped
/// graph projection (the governance half of an authoring transaction owns its
/// own bump, so the patch response's version is not the CAS preimage).
fn entity_version(actor: &DirectActor, world_id: &str, entity_id: &str) -> u64 {
    graph_entity(&world_kb_graph(actor, world_id), entity_id)["version"]
        .as_u64()
        .expect("entity version")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)] // dogfood proof
async fn rn_act4_five_views_share_row_identity_without_copies() {
    let actor = DirectActor::new().await;
    let g = seed(&actor).await;

    let a_w1 = view_character_cli(&actor, &g.character_a, &g.world_w1, &g.bind_a_w1);
    let a_w2 = view_character_cli(&actor, &g.character_a, &g.world_w2, &g.bind_a_w2);
    let b_w1 = view_character_cli(&actor, &g.character_b, &g.world_w1, &g.bind_b_w1);
    // The later World is reached through a binding created last: a
    // Character-owned row is shared across the Character's bindings, never
    // copied into the new World's container.
    let later = view_character_cli(&actor, &g.character_a, &g.world_w3, &g.bind_a_w3);
    let creator = view_creator_cli(&actor, &g.creator_id, &g.world_w1);

    assert_eq!(
        entry_ids(&a_w1),
        expected_ids(&[
            g.ke_w1_public.clone(),
            g.ke_a_share.clone(),
            g.ke_a_w1_local.clone()
        ])
    );
    assert_eq!(
        entry_ids(&a_w2),
        expected_ids(&[g.ke_w2_public.clone(), g.ke_a_share.clone()])
    );
    assert_eq!(
        entry_ids(&b_w1),
        expected_ids(&[g.ke_w1_public.clone(), g.ke_b_share.clone()])
    );
    assert_eq!(
        entry_ids(&later),
        expected_ids(std::slice::from_ref(&g.ke_a_share))
    );
    assert_eq!(
        entry_ids(&creator),
        expected_ids(&[
            g.ke_w1_public.clone(),
            g.ke_w1_secret.clone(),
            g.ke_a_share.clone(),
            g.ke_b_share.clone(),
            g.ke_a_w1_local.clone()
        ])
    );

    let a_w1_i = page_index(&a_w1);
    let a_w2_i = page_index(&a_w2);
    let creator_i = page_index(&creator);

    assert_eq!(a_w1_i[&g.ke_a_share]["canonical_name"], NAME_A_SHARE);
    assert_eq!(a_w2_i[&g.ke_a_share]["canonical_name"], NAME_A_SHARE);
    assert_eq!(
        named_item(&creator_i, NAME_W1_SECRET)["entry_id"],
        g.ke_w1_secret
    );
    // v1.191 P1 T9 (durable §7): the projection carries the native governance
    // pair. `W1Secret` is authored `author-only`, i.e. the Creator's own holder
    // with an owner-private disclosure; the retired boolean is gone.
    assert_eq!(
        named_item(&creator_i, NAME_W1_SECRET)["disclosure"],
        DISCLOSURE_OWNER_PRIVATE
    );
    assert_eq!(
        named_item(&creator_i, NAME_W1_SECRET)["holder_entry_id"],
        json!(creator_holder(&actor, &g.creator_id).await)
    );
    assert!(
        named_item(&creator_i, NAME_W1_SECRET)
            .get("creator_only")
            .is_none(),
        "the retired creator_only boolean must not be projected"
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_W1_PUBLIC)["entry_id"],
        g.ke_w1_public
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_A_W1_LOCAL)["owner"]["kind"],
        "actor_world_binding"
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_A_W1_LOCAL)["owner"]["id"],
        g.bind_a_w1
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_A_SHARE)["owner"]["kind"],
        "character"
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_A_SHARE)["owner"]["id"],
        g.character_a
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_W1_PUBLIC)["owner"]["kind"],
        "world"
    );
    assert_eq!(
        named_item(&a_w1_i, NAME_W1_PUBLIC)["owner"]["id"],
        g.world_w1
    );
    assert_eq!(
        named_item(&page_index(&a_w2), NAME_W2_PUBLIC)["entry_id"],
        g.ke_w2_public
    );
    assert_eq!(
        named_item(&creator_i, NAME_B_SHARE)["entry_id"],
        g.ke_b_share
    );
    // The same durable row is addressed by both Character viewpoint and
    // management review: the revision and status the CLI projects are one row's.
    assert_eq!(
        named_item(&a_w1_i, NAME_A_SHARE)["revision"],
        named_item(&creator_i, NAME_A_SHARE)["revision"]
    );

    let pool = read_only_pool(&actor).await;
    let character_owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE owner_kind = 'character' AND character_id = ?",
    )
    .bind(&g.character_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(character_owned, 1, "Character A KE must not be copied");

    let stored: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id IN (?, ?, ?, ?, ?, ?)",
    )
    .bind(&g.ke_w1_public)
    .bind(&g.ke_w1_secret)
    .bind(&g.ke_w2_public)
    .bind(&g.ke_a_share)
    .bind(&g.ke_b_share)
    .bind(&g.ke_a_w1_local)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, 6, "captured entry_ids must each exist once");
    pool.close().await;

    // ── v1.191 P1 T9: authored audience round trip (durable §3) ─────────────
    //
    // Every arm below drives the shipped CLI verbs, so the journey covers the
    // public authoring path end to end: `add --audience character-private`,
    // `edit --audience shared`, and the World KB `entity patch --audience` round
    // trip read back through `kb graph` and the Actor KnowledgeView.

    let character_holder_a = character_holder(&actor, &g.character_a).await;
    let creator_holder_id = creator_holder(&actor, &g.creator_id).await;
    let view_a = || view_character_ids(&actor, &g.character_a, &g.world_w1, &g.bind_a_w1);
    let view_b = || view_character_ids(&actor, &g.character_b, &g.world_w1, &g.bind_b_w1);

    // Character-private, authored through the CLI: visible to the owning
    // Character and to the Creator's management review, hidden from every other
    // Character.
    let private_entry = add_character_private(&actor, &g.character_a, NAME_A_PRIVATE);
    let a_private =
        page_index(&view_character_cli(&actor, &g.character_a, &g.world_w1, &g.bind_a_w1));
    let creator_private = page_index(&view_creator_cli(&actor, &g.creator_id, &g.world_w1));
    let private_item = named_item(&a_private, NAME_A_PRIVATE);
    assert_eq!(private_item["entry_id"], private_entry);
    assert_eq!(private_item["disclosure"], DISCLOSURE_OWNER_PRIVATE);
    assert_eq!(private_item["holder_entry_id"], json!(character_holder_a));
    assert!(
        !view_b().contains(&private_entry),
        "another Character must not see a character-private row"
    );
    assert_eq!(
        named_item(&creator_private, NAME_A_PRIVATE)["entry_id"],
        private_entry,
        "the Creator's management review keeps an owned private row"
    );

    // The CLI `edit --audience shared` clears the pair under the expected
    // revision CAS. The row is Character-owned, so clearing its disclosure is
    // not a container move: it stays in its owner's container only.
    let shared = edit_audience(&actor, &g.character_a, &private_entry, 0, "shared", None);
    assert!(shared["item"]["holder_entry_id"].is_null());
    assert!(shared["item"]["disclosure"].is_null());
    let a_shared =
        page_index(&view_character_cli(&actor, &g.character_a, &g.world_w1, &g.bind_a_w1));
    assert_eq!(
        named_item(&a_shared, NAME_A_PRIVATE)["entry_id"],
        private_entry
    );
    assert!(a_shared[&private_entry].get("disclosure").is_none());
    assert!(
        !view_b().contains(&private_entry),
        "clearing a disclosure must not leak an owner-scoped row to another Character"
    );

    // World KB `entity patch --audience` on a World-owned row: author-only →
    // character-private → shared, each move under the graph-read CAS preimage,
    // with the Actor KnowledgeView following every one of them.
    let author_only = patch_entity_audience(
        &actor,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&actor, &g.world_w1, &g.ke_w1_public),
        "author-only",
        None,
    );
    assert_eq!(
        author_only["entity"]["holder_entry_id"],
        json!(creator_holder_id)
    );
    assert_eq!(
        author_only["entity"]["disclosure"],
        DISCLOSURE_OWNER_PRIVATE
    );
    // The Creator's holder is not a Character's holder: both bound Characters
    // lose the row, and the management review keeps it.
    assert!(!view_a().contains(&g.ke_w1_public));
    assert!(!view_b().contains(&g.ke_w1_public));
    let creator_author_only = page_index(&view_creator_cli(&actor, &g.creator_id, &g.world_w1));
    assert_eq!(
        creator_author_only[&g.ke_w1_public]["holder_entry_id"],
        json!(creator_holder_id)
    );

    let character_private = patch_entity_audience(
        &actor,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&actor, &g.world_w1, &g.ke_w1_public),
        "character-private",
        Some(&g.character_a),
    );
    assert_eq!(
        character_private["entity"]["holder_entry_id"],
        json!(character_holder_a)
    );
    assert!(view_a().contains(&g.ke_w1_public));
    assert!(!view_b().contains(&g.ke_w1_public));
    assert_eq!(
        graph_entity(&world_kb_graph(&actor, &g.world_w1), &g.ke_w1_public)["holder_entry_id"],
        json!(character_holder_a)
    );

    let cleared = patch_entity_audience(
        &actor,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&actor, &g.world_w1, &g.ke_w1_public),
        "shared",
        None,
    );
    assert!(cleared["entity"]["holder_entry_id"].is_null());
    assert!(cleared["entity"]["disclosure"].is_null());
    assert!(
        view_a().contains(&g.ke_w1_public) && view_b().contains(&g.ke_w1_public),
        "an explicit shared World row returns to every bound Character"
    );
    let cleared_graph = world_kb_graph(&actor, &g.world_w1);
    let graph_row = graph_entity(&cleared_graph, &g.ke_w1_public);
    assert!(graph_row["holder_entry_id"].is_null());
    assert!(graph_row["disclosure"].is_null());
}
