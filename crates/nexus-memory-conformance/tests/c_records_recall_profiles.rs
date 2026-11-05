//! C-matrix families E, F and G (N3): retained records, recall/read selection
//! and trusted profile documents, on both real providers.
//!
//! Every scenario body is provider-agnostic and runs against the real
//! `nexus_core` local provider and the independent durable fixture provider
//! through the signed `spoke-connect` memory surface. The `_local` / `_remote`
//! suffix follows matrix §3. Authorization is exercised with real authenticated
//! principals and real grants: a refused read is asserted as a typed refusal,
//! never as an empty result, and every positive control observes the authorized
//! content itself.
//!
//! C-042 is N4-R (reflection basis) and deliberately has no placeholder here;
//! C-073's refusal belongs to Task 10. No ignored test stands in for either.

#![allow(clippy::too_many_lines)] // one end-to-end scenario per row

mod support;

use serde_json::{json, Value};

use support::{
    bootstrap, candidate_ids, capture_body, context_scope, error_code, error_details,
    grant_body, is_committed, receipt_id, result, retry_class, shared_scope, Actor, Matrix,
    CONTEXT, OTHER_CONTEXT, OTHER_SUBJECT, SUBJECT,
};

// ---------------------------------------------------------------------------
// Protocol request builders and observations
// ---------------------------------------------------------------------------

/// A scoped grant: `contexts` present means a context-restricted grant, with
/// `shared_access` meaningful only in that shape (§6.5).
fn scoped_grant(
    capabilities: &[&str],
    grantee: &str,
    subject: &str,
    contexts: &[&str],
    shared_access: bool,
    key: &str,
) -> Value {
    let mut request = json!({
        "subject_id": subject,
        "principal_id": grantee,
        "capabilities": capabilities,
        "idempotency_key": key,
    });
    if !contexts.is_empty() {
        request["context_ids"] = json!(contexts);
        request["shared_access"] = json!(shared_access);
    }
    request
}

fn record_create_body(
    subject: &str,
    key: &str,
    kind: &str,
    text: &str,
    scope: &Value,
    provenance: &[&str],
) -> Value {
    json!({
        "subject_id": subject,
        "kind": kind,
        "content": { "text": text },
        "scope": scope,
        "source_context_ids": provenance,
        "idempotency_key": key,
    })
}

fn record_list_request(
    selection: Value,
    filter: Option<&[&str]>,
    limit: Option<u64>,
    cursor: Option<&str>,
) -> Value {
    let mut request = json!({ "subject_id": SUBJECT, "selection": selection });
    if let Some(filter) = filter {
        request["filter"] = json!({ "source_context_ids": filter });
    }
    if let Some(limit) = limit {
        request["limit"] = json!(limit);
    }
    if let Some(cursor) = cursor {
        request["cursor"] = json!(cursor);
    }
    request
}

fn recall_request(
    subject: &str,
    selection: Value,
    max_items: u64,
    max_bytes: Option<u64>,
    query: Option<&str>,
    filter: Option<&[&str]>,
    cursor: Option<&str>,
) -> Value {
    let mut request = json!({
        "subject_id": subject,
        "selection": selection,
        "budget": { "max_items": max_items },
    });
    if let Some(max_bytes) = max_bytes {
        request["budget"]["max_bytes"] = json!(max_bytes);
    }
    if let Some(query) = query {
        request["query"] = json!(query);
    }
    if let Some(filter) = filter {
        request["filter"] = json!({ "source_context_ids": filter });
    }
    if let Some(cursor) = cursor {
        request["cursor"] = json!(cursor);
    }
    request
}

fn profile_document_request(
    subject: &str,
    kind: &str,
    expected_revision: Value,
    format: &str,
    body: &str,
    key: &str,
) -> Value {
    json!({
        "subject_id": subject,
        "profile_kind": kind,
        "expected_revision": expected_revision,
        "document": { "format": format, "body": body },
        "idempotency_key": key,
    })
}

fn subject_all() -> Value {
    json!({ "mode": "subject_all" })
}

fn shared_only() -> Value {
    json!({ "mode": "shared_only" })
}

fn selected_contexts(contexts: &[&str], include_shared: bool) -> Value {
    json!({
        "mode": "selected_contexts",
        "context_ids": contexts,
        "include_shared": include_shared,
    })
}

/// The record page's ids, in served order.
fn page_ids(body: &Value) -> Vec<String> {
    assert!(
        body.get("error").is_none(),
        "expected a record page, got {body}"
    );
    body["records"]
        .as_array()
        .unwrap_or_else(|| panic!("a record page names its rows: {body}"))
        .iter()
        .map(|record| {
            record["record_id"]
                .as_str()
                .expect("record id")
                .to_owned()
        })
        .collect()
}

/// The served records of a recall page, in rank order.
fn recalled_records(body: &Value) -> &Vec<Value> {
    assert!(
        body.get("error").is_none(),
        "expected a recall page, got {body}"
    );
    body["records"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall page names its rows: {body}"))
}

fn recalled_texts(body: &Value) -> Vec<String> {
    recalled_records(body)
        .iter()
        .map(|record| {
            record["content"]["text"]
                .as_str()
                .expect("recalled text")
                .to_owned()
        })
        .collect()
}

/// §9: the budget is the UTF-8 byte length of the serialized content fields of
/// the returned record.
fn content_bytes(record: &Value) -> usize {
    serde_json::to_vec(&record["content"])
        .expect("record content serializes")
        .len()
}

fn record_of(body: &Value) -> &Value {
    assert!(
        body.get("error").is_none(),
        "expected a record read, got {body}"
    );
    &body["record"]
}

fn profiles_of(body: &Value) -> &Vec<Value> {
    assert!(
        body.get("error").is_none(),
        "expected a profile page, got {body}"
    );
    body["profiles"]
        .as_array()
        .unwrap_or_else(|| panic!("a profile page names its documents: {body}"))
}

fn profile_kinds(body: &Value) -> Vec<String> {
    profiles_of(body)
        .iter()
        .map(|profile| {
            profile["profile_kind"]
                .as_str()
                .expect("profile kind")
                .to_owned()
        })
        .collect()
}

/// A refusal the protocol documents for this operation; any other code is a
/// failure, and the retry class is always terminal.
fn assert_refused_with(body: &Value, allowed: &[&str]) {
    let code = error_code(body);
    assert!(
        allowed.contains(&code),
        "the refusal is one of the protocol's {allowed:?}, observed {code}: {body}"
    );
    assert_eq!(retry_class(body), "terminal", "{body}");
}

async fn create_record(actor: &Actor<'_>, body: Value) -> String {
    let committed = actor.create_record(body).await;
    assert!(is_committed(&committed), "the record write commits: {committed}");
    result(&committed)["record_id"]
        .as_str()
        .expect("a committed create names its record")
        .to_owned()
}

async fn list_page(
    actor: &Actor<'_>,
    selection: Value,
    limit: Option<u64>,
    cursor: Option<&str>,
) -> Value {
    actor
        .list_records(record_list_request(selection, None, limit, cursor))
        .await
}

/// The current served body of one record, for a no-effect observation.
async fn current_record(matrix: &Matrix, record_id: &str) -> Value {
    record_of(&matrix.admin().get_record(record_id).await).clone()
}

/// The §7.0.2 read surface of one scenario, in a comparable tuple.
type ReadState = (
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
    Value,
);

/// Every base read of the C-046 surface, in one snapshot observation.
async fn read_state(
    admin: &Actor<'_>,
    shared: &str,
    contextual: &str,
    receipt_id: &str,
) -> ReadState {
    (
        admin.describe().await,
        admin.list_spaces().await,
        admin.get_subject(SUBJECT).await,
        admin.list_grants(json!({})).await,
        admin
            .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
            .await,
        admin.get_record(shared).await,
        admin.get_record(contextual).await,
        admin
            .list_records(record_list_request(subject_all(), None, None, None))
            .await,
        admin
            .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
            .await,
        admin.get_profile(json!({ "subject_id": SUBJECT })).await,
        admin.get_receipt(receipt_id).await,
    )
}

// ---------------------------------------------------------------------------
// E. Retained records
// ---------------------------------------------------------------------------

/// C-040 / MEM-R007: create/get/update/delete round trip, and a stale
/// revision changes nothing.
async fn record_crud_and_stale_cas(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();

    let created = admin
        .create_record(record_create_body(
            SUBJECT,
            "k-c040-create",
            "note",
            "first draft of the harbor promise",
            &shared_scope(),
            &["src-draft"],
        ))
        .await;
    assert!(is_committed(&created), "{created}");
    assert_eq!(result(&created)["revision"], json!(1));
    let record_id = result(&created)["record_id"]
        .as_str()
        .expect("record id")
        .to_owned();

    let read = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(read["record_id"], json!(record_id));
    assert_eq!(read["content"]["text"], json!("first draft of the harbor promise"));
    assert_eq!(read["scope"], shared_scope());
    assert_eq!(read["revision"], json!(1));
    assert_eq!(read["provenance"]["captured_by"], json!(admin.principal_id()));
    assert_eq!(
        read["provenance"]["source_context_ids"],
        json!(["src-draft"]),
        "the recorded provenance claims are served verbatim: {read}"
    );
    assert!(
        read["provenance"]["captured_at"].is_string(),
        "the provider stamps captured_at: {read}"
    );
    assert!(read["created_at"].is_string(), "{read}");
    assert!(read["updated_at"].is_string(), "{read}");

    let updated = admin
        .update_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": "second draft of the harbor promise" },
            "kind": "fact",
            "idempotency_key": "k-c040-update",
        }))
        .await;
    assert!(is_committed(&updated), "{updated}");
    assert_eq!(result(&updated)["record_id"], json!(record_id));
    assert_eq!(result(&updated)["revision"], json!(2));

    // The stale precondition is the same request shape against revision 1.
    let stale = admin
        .update_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": "stale overwrite" },
            "idempotency_key": "k-c040-update-stale",
        }))
        .await;
    assert_eq!(error_code(&stale), "REVISION_CONFLICT", "{stale}");
    assert_eq!(retry_class(&stale), "terminal", "{stale}");

    let after_stale = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(after_stale["revision"], json!(2), "no write on failed CAS");
    assert_eq!(
        after_stale["content"]["text"],
        json!("second draft of the harbor promise")
    );
    assert_eq!(after_stale["kind"], json!("fact"));

    let deleted = admin
        .delete_record(json!({
            "record_id": record_id,
            "expected_revision": 2,
            "reason": "superseded by the final route",
            "idempotency_key": "k-c040-delete",
        }))
        .await;
    assert!(is_committed(&deleted), "{deleted}");
    assert_eq!(result(&deleted)["tombstoned"], json!(true));

    // An admin keeps audit visibility of the tombstone; the content is gone.
    let tombstoned = record_of(&admin.get_record(&record_id).await).clone();
    assert!(tombstoned["tombstone"].is_object(), "{tombstoned}");
    assert_eq!(tombstoned["content"]["text"], json!(""));
    assert_eq!(tombstoned["revision"], json!(3));

    // Deleted records leave the default listing and recall.
    let listing = admin
        .list_records(record_list_request(subject_all(), None, None, None))
        .await;
    assert_eq!(listing["total_authorized"], json!(0), "{listing}");
    let recalled = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    assert_eq!(recalled["total_authorized"], json!(0), "{recalled}");
    assert!(recalled_records(&recalled).is_empty());
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_040_record_crud_and_stale_cas_local() {
    record_crud_and_stale_cas(Matrix::local().await).await;
}

#[tokio::test]
async fn c_040_record_crud_and_stale_cas_remote() {
    record_crud_and_stale_cas(Matrix::remote().await).await;
}

/// C-041 / MEM-R007: each committed content update advances the revision by
/// exactly one; revisions are opaque across providers.
async fn revision_changes_once_per_real_update(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let record_id = create_record(
        &admin,
        record_create_body(SUBJECT, "k-c041-create", "note", "revision one", &shared_scope(), &[]),
    )
    .await;
    let mut previous = record_of(&admin.get_record(&record_id).await).clone()["revision"]
        .as_u64()
        .expect("revision");

    for (index, text) in ["revision two", "revision three", "revision four"]
        .iter()
        .enumerate()
    {
        let updated = admin
            .update_record(json!({
                "record_id": record_id,
                "expected_revision": previous,
                "content": { "text": text },
                "idempotency_key": format!("k-c041-update-{index}"),
            }))
            .await;
        assert!(is_committed(&updated), "update {index} commits: {updated}");
        let revision = result(&updated)["revision"].as_u64().expect("revision");
        assert_eq!(
            revision,
            previous + 1,
            "one real update advances the revision by exactly one"
        );
        let read = record_of(&admin.get_record(&record_id).await).clone();
        assert_eq!(read["revision"], json!(revision));
        assert_eq!(read["content"]["text"], json!(text));
        previous = revision;
    }

    // A stale attempt after three real updates is refused and changes nothing.
    let stale = admin
        .update_record(json!({
            "record_id": record_id,
            "expected_revision": 2,
            "content": { "text": "lost update" },
            "idempotency_key": "k-c041-stale",
        }))
        .await;
    assert_eq!(error_code(&stale), "REVISION_CONFLICT", "{stale}");
    let read = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(read["revision"], json!(previous));
    assert_eq!(read["content"]["text"], json!("revision four"));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_041_revision_changes_once_per_real_update_local() {
    revision_changes_once_per_real_update(Matrix::local().await).await;
}

#[tokio::test]
async fn c_041_revision_changes_once_per_real_update_remote() {
    revision_changes_once_per_real_update(Matrix::remote().await).await;
}

/// C-043 / MEM-R015: pages are bounded by the caller's limit, ordered
/// consistently across a fresh re-read, and end without a cursor.
async fn record_pages_are_bounded_and_stable(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let mut expected = Vec::new();
    for index in 0..5 {
        expected.push(
            create_record(
                &admin,
                record_create_body(
                    SUBJECT,
                    &format!("k-c043-{index}"),
                    "note",
                    &format!("page row {index}"),
                    &shared_scope(),
                    &[],
                ),
            )
            .await,
        );
    }

    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = list_page(&admin, subject_all(), Some(2), cursor.as_deref()).await;
        let ids = page_ids(&page);
        assert!(
            ids.len() <= 2,
            "a page never exceeds the requested bound: {page}"
        );
        assert_eq!(page["total_authorized"], json!(5), "{page}");
        seen.extend(ids);
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        pages += 1;
        assert!(pages <= 4, "the paging terminates: {seen:?}");
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(pages, 3, "five rows at two per page need three pages");
    seen.sort();
    let mut wanted = expected.clone();
    wanted.sort();
    assert_eq!(seen, wanted, "pages cover every row exactly once");

    // No state change: the first page is byte-identical on a fresh read.
    let first = list_page(&admin, subject_all(), Some(2), None).await;
    let again = list_page(&admin, subject_all(), Some(2), None).await;
    assert_eq!(first, again, "the page order is stable for unchanged state");
    assert_eq!(page_ids(&first).len(), 2);
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_043_record_pages_are_bounded_and_stable_local() {
    record_pages_are_bounded_and_stable(Matrix::local().await).await;
}

#[tokio::test]
async fn c_043_record_pages_are_bounded_and_stable_remote() {
    record_pages_are_bounded_and_stable(Matrix::remote().await).await;
}

/// C-044 / MEM-R003 + MEM-R017: promotion needs one grant carrying `promote`
/// and `records_write` over both placements; a frozen space refuses it with no
/// effect; provenance survives the accepted promotion.
async fn record_promote_checks_grant_and_freeze(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let record_id = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c044-create",
            "note",
            "promotion subject",
            &context_scope(CONTEXT),
            &["src-promotion"],
        ),
    )
    .await;


    // (a) No promotion authority: refused, no placement or revision change.
    let denied = matrix
        .peer()
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": context_scope(OTHER_CONTEXT),
            "idempotency_key": "k-c044-denied",
        }))
        .await;
    assert_refused_with(&denied, &["NOT_AUTHORIZED", "NOT_FOUND"]);
    let after_denied = current_record(&matrix, &record_id).await;
    assert_eq!(after_denied["scope"], context_scope(CONTEXT), "{after_denied}");
    assert_eq!(after_denied["revision"], json!(1), "no promotion side effect");

    // (b) Frozen space: the maintenance allowlist refuses the promotion.
    let revision = admin.list_spaces().await["spaces"][0]["revision"]
        .as_u64()
        .expect("space revision");
    let frozen = admin.freeze_space("k-c044-freeze", revision).await;
    assert!(is_committed(&frozen), "{frozen}");
    let frozen_revision = result(&frozen)["revision"].as_u64().expect("revision");
    let frozen_promote = admin
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": context_scope(OTHER_CONTEXT),
            "idempotency_key": "k-c044-frozen",
        }))
        .await;
    assert_eq!(error_code(&frozen_promote), "SPACE_FROZEN", "{frozen_promote}");
    assert_eq!(retry_class(&frozen_promote), "terminal");
    let after_frozen = current_record(&matrix, &record_id).await;
    assert_eq!(after_frozen["scope"], context_scope(CONTEXT), "{after_frozen}");
    assert_eq!(after_frozen["revision"], json!(1));
    let unfrozen = admin.unfreeze_space("k-c044-unfreeze", frozen_revision).await;
    assert!(is_committed(&unfrozen), "{unfrozen}");

    // (c) One grant carrying both capabilities over source and target.
    let granted = admin
        .grant(scoped_grant(
            &["promote", "records_write"],
            &peer_id,
            SUBJECT,
            &[CONTEXT, OTHER_CONTEXT],
            false,
            "k-c044-grant",
        ))
        .await;
    assert!(is_committed(&granted), "{granted}");
    let promoted = matrix
        .peer()
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "target_scope": context_scope(OTHER_CONTEXT),
            "idempotency_key": "k-c044-promote",
        }))
        .await;
    assert!(is_committed(&promoted), "{promoted}");
    assert_eq!(result(&promoted)["revision"], json!(2));
    assert_eq!(result(&promoted)["scope"], context_scope(OTHER_CONTEXT));

    let moved = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(moved["scope"], context_scope(OTHER_CONTEXT));
    assert_eq!(moved["revision"], json!(2));
    assert_eq!(
        moved["provenance"]["captured_by"],
        json!(admin.principal_id()),
        "promotion leaves provenance untouched: {moved}"
    );
    assert_eq!(moved["provenance"]["source_context_ids"], json!(["src-promotion"]));
    assert_eq!(moved["content"]["text"], json!("promotion subject"));

    // The same single grant does not reach shared placement without the
    // shared facet: the promotion is refused and changes nothing.
    let to_shared = matrix
        .peer()
        .promote_record(json!({
            "record_id": record_id,
            "expected_revision": 2,
            "target_scope": shared_scope(),
            "idempotency_key": "k-c044-shared",
        }))
        .await;
    assert_refused_with(&to_shared, &["NOT_AUTHORIZED", "NOT_FOUND"]);
    let after_shared = current_record(&matrix, &record_id).await;
    assert_eq!(after_shared["scope"], context_scope(OTHER_CONTEXT));
    assert_eq!(after_shared["revision"], json!(2), "{after_shared}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_044_record_promote_checks_grant_and_freeze_local() {
    record_promote_checks_grant_and_freeze(Matrix::local().await).await;
}

#[tokio::test]
async fn c_044_record_promote_checks_grant_and_freeze_remote() {
    record_promote_checks_grant_and_freeze(Matrix::remote().await).await;
}

/// C-045 / MEM-R015: the advertised `max_content_bytes` is the enforced bound,
/// at-limit content is stored whole, and an over-limit create/update refuses
/// with `INVALID_INPUT` and no silent partial write.
async fn advertised_content_bounds_are_enforced(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let described = admin.describe().await;
    let max_content_bytes = described["limits"]["max_content_bytes"]
        .as_u64()
        .expect("describe advertises max_content_bytes");

    // The published measure is the canonical serialization of the complete
    // `content` object: `{"text":"..."}` costs 11 bytes of framing.
    let framing = 11_u64;
    let at_bound = "a".repeat(usize::try_from(max_content_bytes - framing).expect("bound fits usize"));
    let created = admin
        .create_record(record_create_body(
            SUBJECT,
            "k-c045-at-bound",
            "note",
            &at_bound,
            &shared_scope(),
            &[],
        ))
        .await;
    assert!(
        is_committed(&created),
        "content exactly at the advertised bound is admitted: {created}"
    );
    let record_id = result(&created)["record_id"].as_str().expect("record id").to_owned();
    let stored = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(
        stored["content"]["text"].as_str().expect("stored text").len(),
        at_bound.len(),
        "the at-bound write is stored whole, never truncated"
    );

    // One byte over the advertised bound refuses before any effect.
    let over_bound = "b".repeat(usize::try_from(max_content_bytes - framing).expect("bound fits usize") + 1);
    let refused = admin
        .create_record(record_create_body(
            SUBJECT,
            "k-c045-over-bound",
            "note",
            &over_bound,
            &shared_scope(),
            &[],
        ))
        .await;
    assert_eq!(error_code(&refused), "INVALID_INPUT", "{refused}");
    assert_eq!(retry_class(&refused), "terminal", "{refused}");
    assert_eq!(error_details(&refused)["field"], json!("content"), "{refused}");
    assert_bound_reported(&refused, max_content_bytes);
    let listed = admin
        .list_records(record_list_request(subject_all(), None, None, None))
        .await;
    assert_eq!(
        listed["total_authorized"],
        json!(1),
        "the refused oversize create wrote nothing: {listed}"
    );

    // The same bound governs update.
    let update_refused = admin
        .update_record(json!({
            "record_id": record_id,
            "expected_revision": 1,
            "content": { "text": over_bound },
            "idempotency_key": "k-c045-update-over",
        }))
        .await;
    assert_eq!(error_code(&update_refused), "INVALID_INPUT", "{update_refused}");
    assert_eq!(retry_class(&update_refused), "terminal");
    assert_bound_reported(&update_refused, max_content_bytes);
    let unchanged = record_of(&admin.get_record(&record_id).await).clone();
    assert_eq!(unchanged["revision"], json!(1), "no partial update: {unchanged}");
    assert_eq!(
        unchanged["content"]["text"].as_str().expect("text").len(),
        at_bound.len()
    );
    matrix.shutdown().await;
}

/// The exceeded provider bound is reported on the refusal: `details.limit`
/// where the boundary carries it, otherwise the refusal message names the
/// advertised value.
fn assert_bound_reported(refusal: &Value, max_content_bytes: u64) {
    let details = error_details(refusal);
    let reported = details["limit"].as_str() == Some("max_content_bytes")
        || refusal["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(&max_content_bytes.to_string()));
    assert!(
        reported,
        "the refusal reports the exceeded max_content_bytes ({max_content_bytes}): {refusal}"
    );
}

#[tokio::test]
async fn c_045_advertised_content_bounds_are_enforced_local() {
    advertised_content_bounds_are_enforced(Matrix::local().await).await;
}

#[tokio::test]
async fn c_045_advertised_content_bounds_are_enforced_remote() {
    advertised_content_bounds_are_enforced(Matrix::remote().await).await;
}

/// C-046 / MEM-R011: every base read leaves the observable state — records,
/// profiles, subject, space, grants, candidates, receipts and the pagination
/// snapshot — byte-identical.
async fn base_read_operations_do_not_mutate(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let shared = create_record(
        &admin,
        record_create_body(SUBJECT, "k-c046-shared", "note", "shared read row", &shared_scope(), &[]),
    )
    .await;
    let contextual = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c046-context",
            "fact",
            "context read row",
            &context_scope(CONTEXT),
            &["src-c046"],
        ),
    )
    .await;
    let profile = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            Value::Null,
            "markdown",
            "# Mira\nread-only subject profile",
            "k-c046-profile",
        ))
        .await;
    assert!(is_committed(&profile), "{profile}");
    let captured = admin
        .capture(capture_body("k-c046-capture", "pending read row", &shared_scope()))
        .await;
    assert!(is_committed(&captured), "{captured}");
    let receipt_id = receipt_id(&captured).to_owned();

    // A page cursor minted before the reads is the state fingerprint: a
    // mutation moves the provider snapshot and invalidates it.
    let page = list_page(&admin, subject_all(), Some(1), None).await;
    let cursor = page["next_cursor"]
        .as_str()
        .expect("two rows leave a continuation")
        .to_owned();

    let before = read_state(&admin, &shared, &contextual, &receipt_id).await;

    // The same reads again: nothing may move.
    let after = read_state(&admin, &shared, &contextual, &receipt_id).await;
    assert_eq!(before.0, after.0, "describe is a pure read");
    assert_eq!(before.1, after.1, "spaces.list is a pure read");
    assert_eq!(before.2, after.2, "subjects.get is a pure read");
    assert_eq!(before.3, after.3, "grants.list is a pure read");
    assert_eq!(before.4, after.4, "candidates.list is a pure read");
    assert_eq!(before.5, after.5, "records.get is a pure read");
    assert_eq!(before.6, after.6);
    assert_eq!(before.7, after.7, "records.list is a pure read: {:?}", after.7);
    assert_eq!(before.8, after.8, "recall is a pure read: {:?}", after.8);
    assert_eq!(before.9, after.9, "profile.get is a pure read");
    assert_eq!(before.10, after.10, "receipts.get is a pure read");

    // The pre-read snapshot still continues: no read advanced it.
    let continued = list_page(&admin, subject_all(), Some(1), Some(&cursor)).await;
    assert_eq!(
        page_ids(&continued).len(),
        1,
        "the cursor minted before the reads still pages: {continued}"
    );
    assert_ne!(page_ids(&continued)[0], page_ids(&page)[0]);
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_046_base_read_operations_do_not_mutate_local() {
    base_read_operations_do_not_mutate(Matrix::local().await).await;
}

#[tokio::test]
async fn c_046_base_read_operations_do_not_mutate_remote() {
    base_read_operations_do_not_mutate(Matrix::remote().await).await;
}

/// C-047 / MEM-R026: a by-id read rechecks current authorization — a revoked
/// grant and a placement change both turn the read into a refusal, never an
/// empty result.
async fn by_id_reads_recheck_current_authorization(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let shared = create_record(
        &admin,
        record_create_body(SUBJECT, "k-c047-shared", "note", "shared by-id row", &shared_scope(), &[]),
    )
    .await;

    // Positive control: the peer reads the shared record under a grant that
    // covers shared placement.
    let grant = admin
        .grant(scoped_grant(&["records_read"], &peer_id, SUBJECT, &[], false, "k-c047-grant"))
        .await;
    assert!(is_committed(&grant), "{grant}");
    let grant_id = result(&grant)["grant_id"].as_str().expect("grant id").to_owned();
    let visible = record_of(&matrix.peer().get_record(&shared).await).clone();
    assert_eq!(visible["record_id"], json!(shared));
    assert_eq!(visible["content"]["text"], json!("shared by-id row"));

    // Revoke that grant at its own revision: the next by-id read rechecks.
    let revoked = admin.revoke_grant(&grant_id, 1, "k-c047-revoke").await;
    assert!(is_committed(&revoked), "{revoked}");
    assert_eq!(result(&revoked)["revoked"], json!(true));
    let denied = matrix.peer().get_record(&shared).await;
    assert_eq!(error_code(&denied), "NOT_FOUND", "{denied}");
    assert_eq!(retry_class(&denied), "terminal");
    assert!(
        denied.get("record").is_none(),
        "a refused read is never served as an empty record: {denied}"
    );

    // Placement change: a context-scoped grant saw the record, and the same
    // grant no longer covers it after the record moves to another context.
    let placed = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c047-placed",
            "note",
            "moved by-id row",
            &context_scope(CONTEXT),
            &[],
        ),
    )
    .await;
    let scoped = admin
        .grant(scoped_grant(&["records_read"], &peer_id, SUBJECT, &[CONTEXT], false, "k-c047-scoped"))
        .await;
    assert!(is_committed(&scoped), "{scoped}");
    let before_move = record_of(&matrix.peer().get_record(&placed).await).clone();
    assert_eq!(before_move["scope"], context_scope(CONTEXT));

    let moved = admin
        .promote_record(json!({
            "record_id": placed,
            "expected_revision": 1,
            "target_scope": context_scope(OTHER_CONTEXT),
            "idempotency_key": "k-c047-move",
        }))
        .await;
    assert!(is_committed(&moved), "{moved}");
    let after_move = matrix.peer().get_record(&placed).await;
    assert_refused_with(&after_move, &["NOT_FOUND", "NOT_AUTHORIZED"]);
    assert!(after_move.get("record").is_none(), "{after_move}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_047_by_id_reads_recheck_current_authorization_local() {
    by_id_reads_recheck_current_authorization(Matrix::local().await).await;
}

#[tokio::test]
async fn c_047_by_id_reads_recheck_current_authorization_remote() {
    by_id_reads_recheck_current_authorization(Matrix::remote().await).await;
}

// ---------------------------------------------------------------------------
// F. Recall and read selection
// ---------------------------------------------------------------------------

/// Seed one record per placement for the recall selection scenarios.
async fn seed_placements(admin: &Actor<'_>, prefix: &str) -> (String, String, String) {
    let shared = create_record(
        admin,
        record_create_body(
            SUBJECT,
            &format!("k-{prefix}-shared"),
            "note",
            &format!("{prefix} shared placement"),
            &shared_scope(),
            &[],
        ),
    )
    .await;
    let context = create_record(
        admin,
        record_create_body(
            SUBJECT,
            &format!("k-{prefix}-context"),
            "note",
            &format!("{prefix} first context placement"),
            &context_scope(CONTEXT),
            &[],
        ),
    )
    .await;
    let other = create_record(
        admin,
        record_create_body(
            SUBJECT,
            &format!("k-{prefix}-other"),
            "note",
            &format!("{prefix} second context placement"),
            &context_scope(OTHER_CONTEXT),
            &[],
        ),
    )
    .await;
    (shared, context, other)
}

/// C-050 / MEM-R006: `subject_all` with an unrestricted grant returns every
/// retained record of that subject and nothing of another subject.
async fn subject_all_requires_unrestricted_grant(matrix: Matrix) {
    bootstrap(&matrix, &["recall"]).await;
    let admin = matrix.admin();
    let (shared, context, other) = seed_placements(&admin, "c050").await;
    assert_eq!(
        result(&admin.ensure_subject(OTHER_SUBJECT).await)["created"],
        json!(true)
    );
    let foreign = create_record(
        &admin,
        record_create_body(
            OTHER_SUBJECT,
            "k-c050-foreign",
            "note",
            "another subject's row",
            &shared_scope(),
            &[],
        ),
    )
    .await;

    let recalled = matrix
        .peer()
        .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    let ids = page_ids(&recalled);
    assert_eq!(recalled["total_authorized"], json!(3), "{recalled}");
    assert_eq!(ids.len(), 3);
    for wanted in [&shared, &context, &other] {
        assert!(ids.contains(wanted), "the authorized row {wanted} is recalled: {ids:?}");
    }
    assert!(
        !ids.contains(&foreign),
        "another subject's record is never recalled: {ids:?}"
    );
    assert_eq!(recalled["truncated"], json!(false));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_050_subject_all_requires_unrestricted_grant_local() {
    subject_all_requires_unrestricted_grant(Matrix::local().await).await;
}

#[tokio::test]
async fn c_050_subject_all_requires_unrestricted_grant_remote() {
    subject_all_requires_unrestricted_grant(Matrix::remote().await).await;
}

/// C-051 / MEM-R006: `shared_only` under a shared-access grant returns only
/// shared-placement records.
async fn shared_only_excludes_context_records(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let (shared, context, other) = seed_placements(&admin, "c051").await;
    let grant = admin
        .grant(scoped_grant(
            &["recall"],
            &peer_id,
            SUBJECT,
            &[CONTEXT],
            true,
            "k-c051-grant",
        ))
        .await;
    assert!(is_committed(&grant), "{grant}");

    let recalled = matrix
        .peer()
        .recall(recall_request(SUBJECT, shared_only(), 10, None, None, None, None))
        .await;
    assert_eq!(recalled["total_authorized"], json!(1), "{recalled}");
    assert_eq!(page_ids(&recalled), vec![shared.clone()]);
    let texts = recalled_texts(&recalled);
    assert!(texts.iter().any(|text| text.contains("c051 shared placement")));
    for excluded in [&context, &other] {
        assert!(
            !page_ids(&recalled).contains(excluded),
            "context-placed records are outside shared_only: {recalled}"
        );
    }
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_051_shared_only_excludes_context_records_local() {
    shared_only_excludes_context_records(Matrix::local().await).await;
}

#[tokio::test]
async fn c_051_shared_only_excludes_context_records_remote() {
    shared_only_excludes_context_records(Matrix::remote().await).await;
}

/// C-052 / MEM-R006: `selected_contexts` with `include_shared: true` returns the
/// union of the named context and shared placement, and nothing else.
async fn selected_contexts_can_include_shared(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let (shared, context, other) = seed_placements(&admin, "c052").await;
    let grant = admin
        .grant(scoped_grant(
            &["recall"],
            &peer_id,
            SUBJECT,
            &[CONTEXT],
            true,
            "k-c052-grant",
        ))
        .await;
    assert!(is_committed(&grant), "{grant}");

    let recalled = matrix
        .peer()
        .recall(recall_request(
            SUBJECT,
            selected_contexts(&[CONTEXT], true),
            10,
            None,
            None,
            None,
            None,
        ))
        .await;
    assert_eq!(recalled["total_authorized"], json!(2), "{recalled}");
    let mut ids = page_ids(&recalled);
    ids.sort();
    let mut wanted = vec![shared.clone(), context.clone()];
    wanted.sort();
    assert_eq!(ids, wanted, "the union is exactly the named context plus shared");
    assert!(
        !page_ids(&recalled).contains(&other),
        "the unselected context stays excluded: {recalled}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_052_selected_contexts_can_include_shared_local() {
    selected_contexts_can_include_shared(Matrix::local().await).await;
}

#[tokio::test]
async fn c_052_selected_contexts_can_include_shared_remote() {
    selected_contexts_can_include_shared(Matrix::remote().await).await;
}

/// C-053 / MEM-R006: `selected_contexts` with `include_shared: false` returns
/// exactly the named context; a mode is never expressed by empty overloading.
async fn selected_contexts_can_exclude_shared(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let (shared, context, other) = seed_placements(&admin, "c053").await;
    let grant = admin
        .grant(scoped_grant(
            &["recall"],
            &peer_id,
            SUBJECT,
            &[CONTEXT],
            true,
            "k-c053-grant",
        ))
        .await;
    assert!(is_committed(&grant), "{grant}");

    let recalled = matrix
        .peer()
        .recall(recall_request(
            SUBJECT,
            selected_contexts(&[CONTEXT], false),
            10,
            None,
            None,
            None,
            None,
        ))
        .await;
    assert_eq!(page_ids(&recalled), vec![context.clone()], "{recalled}");
    assert_eq!(recalled["total_authorized"], json!(1));
    assert!(!page_ids(&recalled).contains(&shared), "shared is excluded");
    assert!(!page_ids(&recalled).contains(&other));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_053_selected_contexts_can_exclude_shared_local() {
    selected_contexts_can_exclude_shared(Matrix::local().await).await;
}

#[tokio::test]
async fn c_053_selected_contexts_can_exclude_shared_remote() {
    selected_contexts_can_exclude_shared(Matrix::remote().await).await;
}

/// C-054 / MEM-R004: recall in all three modes never returns a candidate and
/// never counts one in `total_authorized`.
async fn recall_never_returns_candidates(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let (shared, context, other) = seed_placements(&admin, "c054").await;
    let marker = "candidate-marker-never-recalled";
    let captured = admin
        .capture(capture_body("k-c054-capture", marker, &shared_scope()))
        .await;
    let candidates = candidate_ids(&captured);
    assert_eq!(candidates.len(), 1);
    let pending = admin
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await;
    assert_eq!(
        pending["candidates"][0]["candidate_id"],
        json!(candidates[0]),
        "the capture really left a pending candidate: {pending}"
    );

    for (selection, expected_total) in [
        (subject_all(), 3_i64),
        (shared_only(), 1),
        (selected_contexts(&[CONTEXT], false), 1),
    ] {
        let recalled = admin
            .recall(recall_request(SUBJECT, selection, 10, None, None, None, None))
            .await;
        assert_eq!(recalled["total_authorized"], json!(expected_total), "{recalled}");
        let serialized = recalled.to_string();
        assert!(
            !serialized.contains(&candidates[0]),
            "a candidate id never appears in recall: {recalled}"
        );
        assert!(
            !serialized.contains(marker),
            "captured text never appears in recall: {recalled}"
        );
    }
    let listed = admin
        .list_records(record_list_request(subject_all(), None, None, None))
        .await;
    let mut listed_ids = page_ids(&listed);
    listed_ids.sort();
    let mut placements = vec![shared.clone(), context.clone(), other.clone()];
    placements.sort();
    assert_eq!(
        listed_ids, placements,
        "the capture left no retained record behind: {listed}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_054_recall_never_returns_candidates_local() {
    recall_never_returns_candidates(Matrix::local().await).await;
}

#[tokio::test]
async fn c_054_recall_never_returns_candidates_remote() {
    recall_never_returns_candidates(Matrix::remote().await).await;
}

/// C-055 / MEM-R015 + §9: the UTF-8 byte budget is hard, an oversized first item
/// is surfaced whole and flagged, truncation is reported, and the cursor
/// continues the same ranking without losing or duplicating a row.
async fn recall_enforces_utf8_budget_and_overflow(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    for (index, text) in [
        "harbor gate log: the dawn watch changed",
        "market ledger entry: three barrels of salt",
        "northern road survey: the bridge holds",
    ]
    .iter()
    .enumerate()
    {
        create_record(
            &admin,
            record_create_body(
                SUBJECT,
                &format!("k-c055-{index}"),
                "note",
                text,
                &shared_scope(),
                &[],
            ),
        )
        .await;
    }

    // The unbudgeted page establishes the provider's own rank order.
    let full = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    assert_eq!(recalled_records(&full).len(), 3);
    assert_eq!(full["truncated"], json!(false));
    assert!(full["next_cursor"].as_str().is_none());
    assert!(full["overflow_record_id"].as_str().is_none());
    let order = page_ids(&full);
    let sizes: Vec<usize> = recalled_records(&full).iter().map(content_bytes).collect();

    // A budget that fits the first row and cannot fit a second one.
    let budget = u64::try_from(sizes[0]).expect("size fits u64") + 5;
    let mut collected: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = admin
            .recall(recall_request(
                SUBJECT,
                subject_all(),
                10,
                Some(budget),
                None,
                None,
                cursor.as_deref(),
            ))
            .await;
        assert_eq!(
            page["truncated"],
            json!(page["next_cursor"].as_str().is_some()),
            "truncation is reported exactly when more rows remain: {page}"
        );
        assert_eq!(page["total_authorized"], json!(3));
        for record in recalled_records(&page) {
            let overflow = page["overflow_record_id"].as_str()
                == record["record_id"].as_str();
            assert!(
                content_bytes(record) <= usize::try_from(budget).expect("budget fits usize")
                    || overflow,
                "no served row silently exceeds the byte budget: {record}"
            );
        }
        collected.extend(page_ids(&page));
        cursor = page["next_cursor"].as_str().map(str::to_owned);
        pages += 1;
        assert!(pages <= 5, "paging terminates: {collected:?}");
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(collected, order, "pages follow the same rank order, once each");

    // Bytes > budget while characters fit: a single multibyte body larger than
    // the budget is returned WHOLE and flagged, not dropped or truncated.
    let multibyte = "港の灯台が霧の中に浮かぶ".repeat(8);
    assert_eq!(
        result(&admin.ensure_subject(OTHER_SUBJECT).await)["created"],
        json!(true)
    );
    let record_id = create_record(
        &admin,
        record_create_body(
            OTHER_SUBJECT,
            "k-c055-multibyte",
            "note",
            &multibyte,
            &shared_scope(),
            &[],
        ),
    )
    .await;
    let single = admin
        .recall(recall_request(OTHER_SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    assert_eq!(page_ids(&single), vec![record_id.clone()]);
    let bytes = content_bytes(&recalled_records(&single)[0]);
    let characters = multibyte.chars().count();
    assert!(bytes > characters, "the corpus row is genuinely multibyte");
    let tiny_budget = u64::try_from(characters).expect("chars fit u64") + 20;
    assert!(
        u64::try_from(bytes).expect("bytes fit u64") > tiny_budget,
        "the budget is below the byte length while above the character count"
    );
    let overflowed = admin
        .recall(recall_request(
            OTHER_SUBJECT,
            subject_all(),
            10,
            Some(tiny_budget),
            None,
            None,
            None,
        ))
        .await;
    let records = recalled_records(&overflowed);
    assert_eq!(records.len(), 1, "the oversized row is surfaced whole: {overflowed}");
    assert_eq!(records[0]["record_id"], json!(record_id));
    assert_eq!(
        records[0]["content"]["text"], json!(multibyte),
        "the whole body is served, never truncated: {overflowed}"
    );
    assert_eq!(overflowed["overflow_record_id"], json!(record_id), "{overflowed}");
    assert_eq!(overflowed["total_authorized"], json!(1));
    assert_eq!(overflowed["truncated"], json!(false));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_055_recall_enforces_utf8_budget_and_overflow_local() {
    recall_enforces_utf8_budget_and_overflow(Matrix::local().await).await;
}

#[tokio::test]
async fn c_055_recall_enforces_utf8_budget_and_overflow_remote() {
    recall_enforces_utf8_budget_and_overflow(Matrix::remote().await).await;
}

/// C-056 / MEM-R026: a continuation works unchanged, and a real scope, grant or
/// state change invalidates the cursor with `CURSOR_INVALID` — never a silently
/// re-filtered page.
async fn cursor_invalid_after_grant_or_state_change(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    for index in 0..3 {
        create_record(
            &admin,
            record_create_body(
                SUBJECT,
                &format!("k-c056-{index}"),
                "note",
                &format!("cursor row {index}"),
                &context_scope(CONTEXT),
                &[if index == 0 { "src-first" } else { "src-later" }],
            ),
        )
        .await;
    }
    let grant = admin
        .grant(scoped_grant(&["records_read"], &peer_id, SUBJECT, &[CONTEXT], false, "k-c056-grant"))
        .await;
    assert!(is_committed(&grant), "{grant}");
    let selection = || selected_contexts(&[CONTEXT], false);

    // Continuation under identical state succeeds.
    let page = list_page(&matrix.peer(), selection(), Some(1), None).await;
    assert_eq!(page["total_authorized"], json!(3), "{page}");
    let cursor = page["next_cursor"].as_str().expect("more rows remain").to_owned();
    let continued = matrix
        .peer()
        .list_records(record_list_request(selection(), None, Some(1), Some(&cursor)))
        .await;
    assert_eq!(page_ids(&continued).len(), 1, "{continued}");
    assert_ne!(page_ids(&continued), page_ids(&page));

    // A different bound query (an added provenance filter) is a different
    // cursor scope: the same cursor is refused.
    let scope_changed = matrix
        .peer()
        .list_records(record_list_request(
            selection(),
            Some(&["src-first"][..]),
            Some(1),
            Some(&cursor),
        ))
        .await;
    assert_eq!(error_code(&scope_changed), "CURSOR_INVALID", "{scope_changed}");
    assert_eq!(retry_class(&scope_changed), "terminal");

    // A grant change moves the provider snapshot: the cursor no longer applies.
    let before_grant = list_page(&matrix.peer(), selection(), Some(1), None).await;
    let cursor = before_grant["next_cursor"]
        .as_str()
        .expect("more rows remain")
        .to_owned();
    let extra = admin
        .grant(grant_body("capture", &peer_id, "k-c056-extra-grant"))
        .await;
    assert!(is_committed(&extra), "{extra}");
    let after_grant = matrix
        .peer()
        .list_records(record_list_request(selection(), None, Some(1), Some(&cursor)))
        .await;
    assert_eq!(error_code(&after_grant), "CURSOR_INVALID", "{after_grant}");
    // Re-issued from a fresh snapshot the page is served.
    let reissued = list_page(&matrix.peer(), selection(), Some(1), None).await;
    assert_eq!(reissued["total_authorized"], json!(3), "{reissued}");

    // A committed state change invalidates the cursor the same way.
    let cursor = reissued["next_cursor"].as_str().expect("rows remain").to_owned();
    create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c056-later",
            "note",
            "cursor row appended after the page",
            &context_scope(CONTEXT),
            &[],
        ),
    )
    .await;
    let after_state = matrix
        .peer()
        .list_records(record_list_request(selection(), None, Some(1), Some(&cursor)))
        .await;
    assert_eq!(error_code(&after_state), "CURSOR_INVALID", "{after_state}");
    let fresh = list_page(&matrix.peer(), selection(), Some(1), None).await;
    assert_eq!(
        fresh["total_authorized"],
        json!(4),
        "the fresh snapshot sees the new row: {fresh}"
    );
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_056_cursor_invalid_after_grant_or_state_change_local() {
    cursor_invalid_after_grant_or_state_change(Matrix::local().await).await;
}

#[tokio::test]
async fn c_056_cursor_invalid_after_grant_or_state_change_remote() {
    cursor_invalid_after_grant_or_state_change(Matrix::remote().await).await;
}

/// C-057 / §7.22: `selection` is a required `ReadSelection`; its absence or an
/// explicit null is a format error, never an authorization outcome.
async fn missing_or_null_selection_is_invalid_input(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    create_record(
        &admin,
        record_create_body(SUBJECT, "k-c057-row", "note", "selection shape row", &shared_scope(), &[]),
    )
    .await;

    // Positive control through the typed surface: the same call with a
    // well-formed selection is served, so the refusals below are about shape.
    let served = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    assert_eq!(served["total_authorized"], json!(1), "{served}");

    let mut raw = matrix.raw_admin().await;
    for payload in [
        json!({
            "space_id": matrix.space_id(),
            "subject_id": SUBJECT,
            "budget": { "max_items": 10 },
        }),
        json!({
            "space_id": matrix.space_id(),
            "subject_id": SUBJECT,
            "selection": Value::Null,
            "budget": { "max_items": 10 },
        }),
    ] {
        let response = raw.invoke("memory.recall", &payload).await;
        let error = response
            .get("error")
            .unwrap_or_else(|| panic!("a malformed recall is refused: {response}"));
        assert_eq!(error["code"], json!("INVALID_INPUT"), "{response}");
        assert_eq!(error["details"]["retry_class"], json!("terminal"), "{response}");
        assert_ne!(
            error["code"],
            json!("NOT_AUTHORIZED"),
            "format validation is not an authorization outcome: {response}"
        );
    }
    raw.close().await;
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_057_missing_or_null_selection_is_invalid_input_local() {
    missing_or_null_selection_is_invalid_input(Matrix::local().await).await;
}

#[tokio::test]
async fn c_057_missing_or_null_selection_is_invalid_input_remote() {
    missing_or_null_selection_is_invalid_input(Matrix::remote().await).await;
}

/// C-057A / MEM-R006: a context-limited grant never widens through a well-formed
/// selection — `subject_all` and `shared_only` both refuse, while the covered
/// selection is served.
async fn scoped_grant_cannot_widen_recall(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let (shared, context, other) = seed_placements(&admin, "c057a").await;
    let grant = admin
        .grant(scoped_grant(&["recall"], &peer_id, SUBJECT, &[CONTEXT], false, "k-c057a-grant"))
        .await;
    assert!(is_committed(&grant), "{grant}");

    for (selection, name) in [
        (subject_all(), "subject_all"),
        (shared_only(), "shared_only"),
        (selected_contexts(&[OTHER_CONTEXT], false), "an uncovered context"),
    ] {
        let refused = matrix
            .peer()
            .recall(recall_request(SUBJECT, selection, 10, None, None, None, None))
            .await;
        assert_eq!(
            error_code(&refused),
            "NOT_AUTHORIZED",
            "{name} is outside the scoped grant: {refused}"
        );
        assert_eq!(retry_class(&refused), "terminal");
        assert!(
            refused.get("records").is_none(),
            "a refused recall never returns a page: {refused}"
        );
    }

    // The covered selection is served, so the refusals were scope, not absence.
    let served = matrix
        .peer()
        .recall(recall_request(
            SUBJECT,
            selected_contexts(&[CONTEXT], false),
            10,
            None,
            None,
            None,
            None,
        ))
        .await;
    assert_eq!(page_ids(&served), vec![context.clone()], "{served}");
    assert_eq!(served["total_authorized"], json!(1));
    assert!(!page_ids(&served).contains(&shared));
    assert!(!page_ids(&served).contains(&other));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_057a_scoped_grant_cannot_widen_recall_local() {
    scoped_grant_cannot_widen_recall(Matrix::local().await).await;
}

#[tokio::test]
async fn c_057a_scoped_grant_cannot_widen_recall_remote() {
    scoped_grant_cannot_widen_recall(Matrix::remote().await).await;
}

/// C-058 / §9 item 2: ranking is deterministic for identical provider state,
/// request and policy. Cross-provider order equality is NOT required, so only
/// per-provider stability and invariants are asserted.
async fn ranking_is_deterministic_per_provider(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    for (index, text) in [
        "the gate opened once before dawn",
        "a gate warden counted the barrels",
        "market noise without the hint",
    ]
    .iter()
    .enumerate()
    {
        create_record(
            &admin,
            record_create_body(
                SUBJECT,
                &format!("k-c058-{index}"),
                "note",
                text,
                &shared_scope(),
                &[],
            ),
        )
        .await;
    }

    let first = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, Some("gate"), None, None))
        .await;
    let second = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, Some("gate"), None, None))
        .await;
    assert_eq!(page_ids(&first), page_ids(&second), "identical inputs rank identically");
    assert_eq!(first, second);
    let policy = first["engine_policy_version"]
        .as_str()
        .expect("recall stamps its engine policy version");
    assert!(!policy.is_empty());
    assert_eq!(page_ids(&first).len(), 3);
    let mut sorted = page_ids(&first);
    sorted.sort();
    let mut unique = sorted.clone();
    unique.dedup();
    assert_eq!(
        unique, sorted,
        "every authorized row is ranked exactly once: {sorted:?}"
    );

    // The hint orders, it never filters: the row without it is still present.
    let texts = recalled_texts(&first);
    assert!(
        texts.iter().any(|text| text.contains("market noise")),
        "a non-matching hint never removes an authorized row: {texts:?}"
    );

    // A budget never changes the ranking of the rows it does serve.
    let budgeted = admin
        .recall(recall_request(SUBJECT, subject_all(), 1, None, Some("gate"), None, None))
        .await;
    assert_eq!(page_ids(&budgeted), vec![page_ids(&first)[0].clone()]);
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_058_ranking_is_deterministic_per_provider_local() {
    ranking_is_deterministic_per_provider(Matrix::local().await).await;
}

#[tokio::test]
async fn c_058_ranking_is_deterministic_per_provider_remote() {
    ranking_is_deterministic_per_provider(Matrix::remote().await).await;
}

/// C-059 / §6.13: the provenance filter is ANDed with the placement selection —
/// a provenance match never widens visibility, and placement membership alone
/// never survives a non-matching filter.
async fn provenance_filter_is_anded_with_placement(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let peer_id = matrix.peer_principal().principal_id.as_str().to_owned();
    let shared_w1 = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c059-shared-w1",
            "note",
            "shared row with world one provenance",
            &shared_scope(),
            &["w1"],
        ),
    )
    .await;
    let context_w1 = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c059-context-w1",
            "note",
            "context row with world one provenance",
            &context_scope(CONTEXT),
            &["w1", "w2"],
        ),
    )
    .await;
    let context_w2 = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c059-context-w2",
            "note",
            "context row with world two provenance",
            &context_scope(CONTEXT),
            &["w2"],
        ),
    )
    .await;
    let other_w1 = create_record(
        &admin,
        record_create_body(
            SUBJECT,
            "k-c059-other-w1",
            "note",
            "unselected context with world one provenance",
            &context_scope(OTHER_CONTEXT),
            &["w1"],
        ),
    )
    .await;
    let grant = admin
        .grant(scoped_grant(&["recall"], &peer_id, SUBJECT, &[CONTEXT], true, "k-c059-grant"))
        .await;
    assert!(is_committed(&grant), "{grant}");

    // include_shared: placement admits CONTEXT + shared; the filter keeps w1.
    let both = matrix
        .peer()
        .recall(recall_request(
            SUBJECT,
            selected_contexts(&[CONTEXT], true),
            10,
            None,
            None,
            Some(&["w1"][..]),
            None,
        ))
        .await;
    let mut ids = page_ids(&both);
    ids.sort();
    let mut wanted = vec![shared_w1.clone(), context_w1.clone()];
    wanted.sort();
    assert_eq!(ids, wanted, "placement and provenance are both required: {both}");
    assert!(
        !page_ids(&both).contains(&other_w1),
        "a provenance match never widens placement: {both}"
    );
    assert!(
        !page_ids(&both).contains(&context_w2),
        "placement membership alone never survives the filter: {both}"
    );
    assert_eq!(both["total_authorized"], json!(2));

    // Without shared access the shared record is outside the placement, so the
    // matching provenance cannot surface it.
    let contexts_only = matrix
        .peer()
        .recall(recall_request(
            SUBJECT,
            selected_contexts(&[CONTEXT], false),
            10,
            None,
            None,
            Some(&["w1"][..]),
            None,
        ))
        .await;
    assert_eq!(
        page_ids(&contexts_only),
        vec![context_w1.clone()],
        "the filter never acts as placement: {contexts_only}"
    );
    assert!(!page_ids(&contexts_only).contains(&shared_w1));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_059_provenance_filter_is_anded_with_placement_local() {
    provenance_filter_is_anded_with_placement(Matrix::local().await).await;
}

#[tokio::test]
async fn c_059_provenance_filter_is_anded_with_placement_remote() {
    provenance_filter_is_anded_with_placement(Matrix::remote().await).await;
}

// ---------------------------------------------------------------------------
// G. Profile documents
// ---------------------------------------------------------------------------

/// C-060 / §6.8 + §10: an authored profile document is stored and served byte
/// for byte, and no captured/untrusted text is merged into it.
async fn profile_content_is_byte_preserved(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let markdown = "## Mira\n\n- CJK: 港の灯台\n- trailing spaces   \n\ttabbed line\n\"quoted\" and \\ backslash\n";
    let written = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            Value::Null,
            "markdown",
            markdown,
            "k-c060-personality",
        ))
        .await;
    assert!(is_committed(&written), "{written}");
    assert_eq!(result(&written)["profile_kind"], json!("personality"));
    assert_eq!(result(&written)["revision"], json!(1));

    let page = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": ["personality"] }))
        .await;
    let profiles = profiles_of(&page);
    assert_eq!(profiles.len(), 1, "{page}");
    assert_eq!(profiles[0]["profile_kind"], json!("personality"));
    assert_eq!(profiles[0]["document"]["format"], json!("markdown"));
    assert_eq!(
        profiles[0]["document"]["body"].as_str().expect("profile body"),
        markdown,
        "the authored body is served byte for byte"
    );
    assert_eq!(profiles[0]["revision"], json!(1));
    assert_eq!(profiles[0]["updated_by"], json!(admin.principal_id()));

    // A JSON-format body is preserved as authored, not reserialized.
    let json_body = "{\"n\": 1.0, \"text\": \"héllo\", \"nested\": null}";
    let written = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "preferences",
            Value::Null,
            "json",
            json_body,
            "k-c060-preferences",
        ))
        .await;
    assert!(is_committed(&written), "{written}");
    let page = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": ["preferences"] }))
        .await;
    assert_eq!(
        profiles_of(&page)[0]["document"]["body"],
        json!(json_body),
        "the trusted JSON body is byte-preserved"
    );
    assert_eq!(profile_kinds(&page), vec!["preferences".to_owned()]);
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_060_profile_content_is_byte_preserved_local() {
    profile_content_is_byte_preserved(Matrix::local().await).await;
}

#[tokio::test]
async fn c_060_profile_content_is_byte_preserved_remote() {
    profile_content_is_byte_preserved(Matrix::remote().await).await;
}

/// C-061 / MEM-R007: `profile.update` is CAS-guarded; a stale revision and a
/// second create-only write both refuse with no effect.
async fn profile_update_enforces_cas(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let created = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            Value::Null,
            "markdown",
            "# Mira\nfirst authored body",
            "k-c061-create",
        ))
        .await;
    assert!(is_committed(&created), "{created}");
    assert_eq!(result(&created)["revision"], json!(1));

    let stale = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            json!(9),
            "markdown",
            "# Mira\nstale overwrite",
            "k-c061-stale",
        ))
        .await;
    assert_eq!(error_code(&stale), "REVISION_CONFLICT", "{stale}");
    assert_eq!(retry_class(&stale), "terminal");
    let unchanged = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": ["personality"] }))
        .await;
    assert_eq!(profiles_of(&unchanged)[0]["revision"], json!(1));
    assert_eq!(
        profiles_of(&unchanged)[0]["document"]["body"],
        json!("# Mira\nfirst authored body"),
        "a refused stale write changes nothing: {unchanged}"
    );

    let fresh = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            json!(1),
            "markdown",
            "# Mira\nsecond authored body",
            "k-c061-fresh",
        ))
        .await;
    assert!(is_committed(&fresh), "{fresh}");
    assert_eq!(result(&fresh)["revision"], json!(2));

    // Creating an existing kind with `expected_revision: null` is a conflict.
    let recreate = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            Value::Null,
            "markdown",
            "# Mira\ncreate-only retry",
            "k-c061-recreate",
        ))
        .await;
    assert_eq!(error_code(&recreate), "REVISION_CONFLICT", "{recreate}");
    let after = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": ["personality"] }))
        .await;
    assert_eq!(profiles_of(&after)[0]["revision"], json!(2));
    assert_eq!(
        profiles_of(&after)[0]["document"]["body"],
        json!("# Mira\nsecond authored body")
    );

    // The earlier revision token stays stale after the accepted write.
    let lost = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            json!(1),
            "markdown",
            "# Mira\nlost update",
            "k-c061-lost",
        ))
        .await;
    assert_eq!(error_code(&lost), "REVISION_CONFLICT", "{lost}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_061_profile_update_enforces_cas_local() {
    profile_update_enforces_cas(Matrix::local().await).await;
}

#[tokio::test]
async fn c_061_profile_update_enforces_cas_remote() {
    profile_update_enforces_cas(Matrix::remote().await).await;
}

/// C-062 / MEM-R005: profile admission and capability admission are distinct
/// outcomes — an absent optional profile is honest-empty success, missing
/// coverage is `NOT_AUTHORIZED`, and a missing capability is
/// `capability_missing`. No empty success stands in for a capability.
///
/// The `PROFILE_INCOMPLETE` branch is the admission-relevant *required* kind,
/// which only `memory.reflections.generate` consumes (§7.27); that operation is
/// N4-R, so this N3 row proves the three outcomes the base surface can express
/// and leaves the required-kind gate to the reflection slice rather than
/// inventing it here.
async fn profile_and_capability_admission_are_distinct(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();

    // (1) The provider declares the required authored kinds it would gate on.
    let described = admin.describe().await;
    let required = described["required_profile_kinds"]
        .as_array()
        .unwrap_or_else(|| panic!("describe advertises required_profile_kinds: {described}"))
        .iter()
        .map(|kind| kind.as_str().expect("kind").to_owned())
        .collect::<Vec<_>>();
    assert!(
        required.iter().all(|kind| !kind.is_empty()),
        "every declared required kind is a name: {described}"
    );

    // (2) A missing optional profile is an honest-empty success — for every
    // declared kind too: a plain read is not the admission gate.
    for kind in ["preferences", "personality"] {
        let page = admin
            .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": [kind] }))
            .await;
        assert!(
            page.get("error").is_none(),
            "an absent kind is honest-empty, never an error: {page}"
        );
        assert!(profiles_of(&page).is_empty());
    }

    // (3) Missing coverage is a typed authorization refusal, not empty data.
    let denied = matrix
        .peer()
        .get_profile(json!({ "subject_id": SUBJECT }))
        .await;
    assert_eq!(error_code(&denied), "NOT_AUTHORIZED", "{denied}");
    assert!(denied.get("profiles").is_none(), "{denied}");

    // (4) A missing capability is decided at dispatch, before profile policy.
    let mut raw = matrix.raw_admin().await;
    let unavailable = raw
        .invoke(
            "memory.reflections.state",
            &json!({
                "space_id": matrix.space_id(),
                "subject_id": SUBJECT,
                "selection": { "mode": "subject_all" },
            }),
        )
        .await;
    raw.close().await;
    let error = unavailable
        .get("error")
        .unwrap_or_else(|| panic!("the unavailable capability is refused: {unavailable}"));
    assert_eq!(error["code"], json!("capability_missing"), "{unavailable}");
    assert_eq!(error["details"]["retry_class"], json!("terminal"));

    // The three outcomes are three distinct shapes, never collapsed.
    assert_ne!(error["code"], json!("NOT_AUTHORIZED"));
    assert_ne!(error["code"], json!("PROFILE_INCOMPLETE"));
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_062_profile_and_capability_admission_are_distinct_local() {
    profile_and_capability_admission_are_distinct(Matrix::local().await).await;
}

#[tokio::test]
async fn c_062_profile_and_capability_admission_are_distinct_remote() {
    profile_and_capability_admission_are_distinct(Matrix::remote().await).await;
}

/// C-063 / §10: capture never mutates a profile document and no captured text
/// is merged into trusted content.
async fn capture_never_mutates_profile(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let body = "# Mira\ntrusted authored personality";
    let written = admin
        .update_profile(profile_document_request(
            SUBJECT,
            "personality",
            Value::Null,
            "markdown",
            body,
            "k-c063-profile",
        ))
        .await;
    assert!(is_committed(&written), "{written}");
    let before = admin.get_profile(json!({ "subject_id": SUBJECT })).await;

    let marker = "untrusted-capture-marker-42";
    let captured = admin
        .capture(capture_body("k-c063-capture", marker, &shared_scope()))
        .await;
    let candidates = candidate_ids(&captured);
    assert_eq!(candidates.len(), 1);
    let pending = admin
        .list_candidates(json!({ "subject_id": SUBJECT, "status": "pending" }))
        .await;
    assert!(
        pending.to_string().contains(marker),
        "the capture really recorded the untrusted text as a candidate: {pending}"
    );

    let after = admin.get_profile(json!({ "subject_id": SUBJECT })).await;
    assert_eq!(before, after, "capture leaves the profile untouched");
    assert_eq!(profiles_of(&after)[0]["document"]["body"], json!(body));
    assert!(
        !after.to_string().contains(marker),
        "captured text never enters a trusted profile: {after}"
    );

    // Recall is equally uninvolved: capture alone retains no record.
    let recalled = admin
        .recall(recall_request(SUBJECT, subject_all(), 10, None, None, None, None))
        .await;
    assert_eq!(recalled["total_authorized"], json!(0), "{recalled}");
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_063_capture_never_mutates_profile_local() {
    capture_never_mutates_profile(Matrix::local().await).await;
}

#[tokio::test]
async fn c_063_capture_never_mutates_profile_remote() {
    capture_never_mutates_profile(Matrix::remote().await).await;
}

/// C-064 / §6.8: capability concepts are never profile document kinds — a
/// provider with zero `transfer`/`governance`/`reflection` documents reads
/// honest-empty, and selection fails or succeeds on capability grounds only.
async fn selection_requires_no_fake_profile_kinds(matrix: Matrix) {
    bootstrap(&matrix, &[]).await;
    let admin = matrix.admin();
    let capability_kinds = ["transfer", "governance", "reflection"];

    // Fresh provider: all-kinds and capability-kind reads are honest-empty.
    let all = admin.get_profile(json!({ "subject_id": SUBJECT })).await;
    assert!(profiles_of(&all).is_empty(), "no profile is invented: {all}");
    let fake = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": capability_kinds }))
        .await;
    assert!(
        profiles_of(&fake).is_empty(),
        "capability concepts are not profile content kinds: {fake}"
    );

    // Real authored kinds exist; the capability kinds still read empty.
    for (kind, body) in [
        ("personality", "# Mira\nauthored personality"),
        ("preferences", "{\"tone\": \"terse\"}"),
    ] {
        let format = if kind == "preferences" { "json" } else { "markdown" };
        let written = admin
            .update_profile(profile_document_request(
                SUBJECT,
                kind,
                Value::Null,
                format,
                body,
                &format!("k-c064-{kind}"),
            ))
            .await;
        assert!(is_committed(&written), "{written}");
    }
    let mut kinds = profile_kinds(&admin.get_profile(json!({ "subject_id": SUBJECT })).await);
    kinds.sort();
    assert_eq!(kinds, vec!["personality".to_owned(), "preferences".to_owned()]);
    let after = admin
        .get_profile(json!({ "subject_id": SUBJECT, "profile_kinds": capability_kinds }))
        .await;
    assert!(
        profiles_of(&after).is_empty(),
        "no fake profile document is fabricated for a capability: {after}"
    );

    // Selection by capability fails on CAPABILITY grounds, independent of any
    // profile content.
    let mut raw = matrix.raw_admin().await;
    let unavailable = raw
        .invoke(
            "memory.reflections.list",
            &json!({
                "space_id": matrix.space_id(),
                "subject_id": SUBJECT,
            }),
        )
        .await;
    raw.close().await;
    let error = unavailable
        .get("error")
        .unwrap_or_else(|| panic!("the unavailable capability is refused: {unavailable}"));
    assert_eq!(error["code"], json!("capability_missing"), "{unavailable}");

    for kind in profile_kinds(&admin.get_profile(json!({ "subject_id": SUBJECT })).await) {
        assert!(
            !capability_kinds.contains(&kind.as_str()),
            "a capability name is never served as a profile kind: {kind}"
        );
    }
    matrix.shutdown().await;
}

#[tokio::test]
async fn c_064_selection_requires_no_fake_profile_kinds_local() {
    selection_requires_no_fake_profile_kinds(Matrix::local().await).await;
}

#[tokio::test]
async fn c_064_selection_requires_no_fake_profile_kinds_remote() {
    selection_requires_no_fake_profile_kinds(Matrix::remote().await).await;
}
