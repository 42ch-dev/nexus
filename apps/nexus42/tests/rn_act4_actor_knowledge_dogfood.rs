//! RN-ACT-4 multi-World no-copy dogfood against live daemon HTTP + CLI.

mod common;

use common::rn_act4::{
    expected, names, page_index, seed, view_character_cli, view_creator_cli, NAME_A_SHARE,
    NAME_A_W1_LOCAL, NAME_B_SHARE, NAME_W1_PUBLIC, NAME_W1_SECRET, NAME_W2_PUBLIC,
};
use common::LiveDaemon;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rn_act4_five_views_share_row_identity_without_copies() {
    let d = LiveDaemon::start().await;
    let g = seed(&d).await;

    let a_w1 = view_character_cli(&d, &g.character_a, &g.world_w1, &g.bind_a_w1).await;
    let a_w2 = view_character_cli(&d, &g.character_a, &g.world_w2, &g.bind_a_w2).await;
    let b_w1 = view_character_cli(&d, &g.character_b, &g.world_w1, &g.bind_b_w1).await;
    let later = view_character_cli(&d, &g.character_a, &g.world_w3, &g.bind_a_w3).await;
    let creator = view_creator_cli(&d, &g.world_w1).await;

    assert_eq!(
        names(&a_w1),
        expected(&[NAME_W1_PUBLIC, NAME_A_SHARE, NAME_A_W1_LOCAL])
    );
    assert_eq!(names(&a_w2), expected(&[NAME_W2_PUBLIC, NAME_A_SHARE]));
    assert_eq!(names(&b_w1), expected(&[NAME_W1_PUBLIC, NAME_B_SHARE]));
    assert_eq!(names(&later), expected(&[NAME_A_SHARE]));
    assert_eq!(
        names(&creator),
        expected(&[
            NAME_W1_PUBLIC,
            NAME_W1_SECRET,
            NAME_A_SHARE,
            NAME_B_SHARE,
            NAME_A_W1_LOCAL
        ])
    );

    let a_w1_i = page_index(&a_w1);
    let a_w2_i = page_index(&a_w2);
    let b_w1_i = page_index(&b_w1);
    let later_i = page_index(&later);
    let creator_i = page_index(&creator);

    assert_eq!(a_w1_i[NAME_A_SHARE]["entry_id"], g.ke_a_share);
    assert_eq!(a_w2_i[NAME_A_SHARE]["entry_id"], g.ke_a_share);
    assert_eq!(later_i[NAME_A_SHARE]["entry_id"], g.ke_a_share);
    assert_eq!(creator_i[NAME_A_SHARE]["entry_id"], g.ke_a_share);

    assert_eq!(a_w1_i[NAME_W1_PUBLIC]["entry_id"], g.ke_w1_public);
    assert_eq!(b_w1_i[NAME_W1_PUBLIC]["entry_id"], g.ke_w1_public);
    assert_eq!(creator_i[NAME_W1_PUBLIC]["entry_id"], g.ke_w1_public);
    assert_eq!(a_w1_i[NAME_A_W1_LOCAL]["entry_id"], g.ke_a_w1_local);
    assert_eq!(creator_i[NAME_W1_SECRET]["entry_id"], g.ke_w1_secret);
    assert_eq!(creator_i[NAME_W1_SECRET]["creator_only"], true);
    assert_eq!(a_w2_i[NAME_W2_PUBLIC]["entry_id"], g.ke_w2_public);

    assert_eq!(a_w1_i[NAME_A_SHARE]["owner"]["kind"], "character");
    assert_eq!(a_w1_i[NAME_A_SHARE]["owner"]["id"], g.character_a);
    assert_eq!(a_w1_i[NAME_A_W1_LOCAL]["owner"]["kind"], "actor_world_binding");
    assert_eq!(a_w1_i[NAME_A_W1_LOCAL]["owner"]["id"], g.bind_a_w1);
    assert_eq!(a_w1_i[NAME_W1_PUBLIC]["owner"]["kind"], "world");
    assert_eq!(a_w1_i[NAME_W1_PUBLIC]["owner"]["id"], g.world_w1);

    let character_owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE owner_kind = 'character' AND character_id = ?",
    )
    .bind(&g.character_a)
    .fetch_one(&d.pool)
    .await
    .unwrap();
    assert_eq!(character_owned, 1, "Character A KE must not be copied");

    let named: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE canonical_name IN (?, ?, ?, ?, ?, ?)",
    )
    .bind(NAME_W1_PUBLIC)
    .bind(NAME_W1_SECRET)
    .bind(NAME_W2_PUBLIC)
    .bind(NAME_A_SHARE)
    .bind(NAME_B_SHARE)
    .bind(NAME_A_W1_LOCAL)
    .fetch_one(&d.pool)
    .await
    .unwrap();
    assert_eq!(named, 6, "fixture must write exactly one row per named KE");
}
