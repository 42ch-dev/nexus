//! RN-ACT-4 multi-World no-copy dogfood against live daemon HTTP + CLI.

mod common;

use common::rn_act4::{
    add_character_private, character_holder, creator_holder, edit_audience, entity_version,
    entry_ids, expected_ids, graph_entity, named_item, page_index, patch_entity_audience, seed,
    view_character_cli, view_character_ids, view_creator_cli, world_kb_graph,
    NAME_A_PRIVATE, NAME_A_SHARE, NAME_A_W1_LOCAL, NAME_B_SHARE, NAME_W1_PUBLIC, NAME_W1_SECRET,
    NAME_W2_PUBLIC,
};
use common::LiveDaemon;
use nexus_knowledge::world_kb::knowledge_entry::DISCLOSURE_OWNER_PRIVATE;
use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)] // dogfood proof
async fn rn_act4_five_views_share_row_identity_without_copies() {
    let d = LiveDaemon::start_for_creator(common::rn_act4::FIXTURE_CREATOR, "default").await;
    let g = seed(&d).await;

    let a_w1 = view_character_cli(&d, &g.character_a, &g.world_w1, &g.bind_a_w1).await;
    let a_w2 = view_character_cli(&d, &g.character_a, &g.world_w2, &g.bind_a_w2).await;
    let b_w1 = view_character_cli(&d, &g.character_b, &g.world_w1, &g.bind_b_w1).await;
    let later = view_character_cli(&d, &g.character_a, &g.world_w3, &g.bind_a_w3).await;
    let creator = view_creator_cli(&d, &g.creator_id, &g.world_w1).await;

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
        serde_json::json!(creator_holder(&d, &g.creator_id).await)
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
        named_item(&a_w2_i, NAME_W2_PUBLIC)["entry_id"],
        g.ke_w2_public
    );
    assert_eq!(
        named_item(&creator_i, NAME_B_SHARE)["entry_id"],
        g.ke_b_share
    );

    let character_owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE owner_kind = 'character' AND character_id = ?",
    )
    .bind(&g.character_a)
    .fetch_one(&d.pool)
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
    .fetch_one(&d.pool)
    .await
    .unwrap();
    assert_eq!(stored, 6, "captured entry_ids must each exist once");

    // ── v1.191 P1 T9: authored audience round trip (durable §3) ─────────────
    //
    // Every arm below drives the shipped CLI verbs, so the journey covers the
    // public authoring path end to end: `add --audience character-private`,
    // `edit --audience shared`, and the World KB `entity patch --audience` round
    // trip read back through `kb graph` and the Actor KnowledgeView.

    let character_holder_a = character_holder(&d, &g.character_a).await;
    let creator_holder_id = creator_holder(&d, &g.creator_id).await;
    let view_a = || view_character_ids(&d, &g.character_a, &g.world_w1, &g.bind_a_w1);
    let view_b = || view_character_ids(&d, &g.character_b, &g.world_w1, &g.bind_b_w1);

    // Character-private, authored through the CLI: visible to the owning
    // Character and to the Creator's management review, hidden from every other
    // Character.
    let private_entry = add_character_private(&d, &g.character_a, NAME_A_PRIVATE).await;
    let a_private = page_index(&view_character_cli(&d, &g.character_a, &g.world_w1, &g.bind_a_w1).await);
    let creator_private = page_index(&view_creator_cli(&d, &g.creator_id, &g.world_w1).await);
    let private_item = named_item(&a_private, NAME_A_PRIVATE);
    assert_eq!(private_item["entry_id"], private_entry);
    assert_eq!(private_item["disclosure"], DISCLOSURE_OWNER_PRIVATE);
    assert_eq!(private_item["holder_entry_id"], json!(character_holder_a));
    assert!(
        !view_b().await.contains(&private_entry),
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
    let shared = edit_audience(&d, &g.character_a, &private_entry, 0, "shared", None).await;
    assert!(shared["item"]["holder_entry_id"].is_null());
    assert!(shared["item"]["disclosure"].is_null());
    let a_shared = page_index(&view_character_cli(&d, &g.character_a, &g.world_w1, &g.bind_a_w1).await);
    assert_eq!(
        named_item(&a_shared, NAME_A_PRIVATE)["entry_id"],
        private_entry
    );
    assert!(a_shared[&private_entry].get("disclosure").is_none());
    assert!(
        !view_b().await.contains(&private_entry),
        "clearing a disclosure must not leak an owner-scoped row to another Character"
    );

    // World KB `entity patch --audience` on a World-owned row: author-only →
    // character-private → shared, each move under the graph-read CAS preimage,
    // with the Actor KnowledgeView following every one of them.
    let author_only = patch_entity_audience(
        &d,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&d, &g.world_w1, &g.ke_w1_public).await,
        "author-only",
        None,
    )
    .await;
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
    assert!(!view_a().await.contains(&g.ke_w1_public));
    assert!(!view_b().await.contains(&g.ke_w1_public));
    let creator_author_only = page_index(&view_creator_cli(&d, &g.creator_id, &g.world_w1).await);
    assert_eq!(
        creator_author_only[&g.ke_w1_public]["holder_entry_id"],
        json!(creator_holder_id)
    );

    let character_private = patch_entity_audience(
        &d,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&d, &g.world_w1, &g.ke_w1_public).await,
        "character-private",
        Some(&g.character_a),
    )
    .await;
    assert_eq!(
        character_private["entity"]["holder_entry_id"],
        json!(character_holder_a)
    );
    assert!(view_a().await.contains(&g.ke_w1_public));
    assert!(!view_b().await.contains(&g.ke_w1_public));
    assert_eq!(
        graph_entity(&world_kb_graph(&d, &g.world_w1).await, &g.ke_w1_public)["holder_entry_id"],
        json!(character_holder_a)
    );

    let cleared = patch_entity_audience(
        &d,
        &g.world_w1,
        &g.ke_w1_public,
        entity_version(&d, &g.world_w1, &g.ke_w1_public).await,
        "shared",
        None,
    )
    .await;
    assert!(cleared["entity"]["holder_entry_id"].is_null());
    assert!(cleared["entity"]["disclosure"].is_null());
    assert!(
        view_a().await.contains(&g.ke_w1_public) && view_b().await.contains(&g.ke_w1_public),
        "an explicit shared World row returns to every bound Character"
    );
    let cleared_graph = world_kb_graph(&d, &g.world_w1).await;
    let graph_row = graph_entity(&cleared_graph, &g.ke_w1_public);
    assert!(graph_row["holder_entry_id"].is_null());
    assert!(graph_row["disclosure"].is_null());
}
