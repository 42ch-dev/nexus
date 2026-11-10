//! Round-trip proofs for the in-memory `KnowledgeEntryPort` stub and the
//! `KnowledgeEntryRecord ↔ spoke KnowledgeEntry` conversion seam it is driven
//! through (K2 §5.1 P0).
//!
//! A dry-run caller binds the stub and reports what a run *would* have
//! written, so two things must hold from the public surface:
//! - the seam it converts through preserves the executable fields a caller
//!   reads back (identity, owner, type, name, status, revision, body summary /
//!   tags / state), and
//! - the stub answers exactly what was put, fails closed on an unknown id, and
//!   records every intent in call order for that report.

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_spoke_adapter::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};
use nexus_spoke_adapter::{
    KnowledgeEntryPort, SpokeRejectCode, SpokeResult, StubKnowledgeEntryMutation,
    StubKnowledgeEntryPort,
};
use serde_json::json;

/// A record carrying every field the executable path reads back, so a field
/// the seam drops fails an assertion instead of hiding behind a default.
fn executable_record() -> KnowledgeEntryRecord {
    let mut record =
        KnowledgeEntryRecord::new("wld_v232", BlockType::Character, "Reluctant Cartographer");
    record.entry_id = "kb_v232_rt".to_string();
    record.status = "confirmed".to_string();
    record.revision = Some(7);
    record.body = Some(KnowledgeEntryBody {
        summary: Some("Maps coasts that move.".to_string()),
        tags: Some(vec!["pov".to_string(), "travel".to_string()]),
        state: Some(json!({"character": {"current_hp": 80}})),
        ..Default::default()
    });
    record
}

#[test]
fn conversion_round_trip_preserves_executable_fields() {
    let record = executable_record();

    let roundtripped = spoke_to_knowledge_record(knowledge_record_to_spoke(&record))
        .expect("reverse conversion of a forward-converted record");

    assert_eq!(roundtripped.entry_id, "kb_v232_rt");
    assert_eq!(roundtripped.world_id(), Some("wld_v232"));
    assert_eq!(roundtripped.block_type, BlockType::Character);
    assert_eq!(roundtripped.canonical_name, "Reluctant Cartographer");
    assert_eq!(roundtripped.status, "confirmed");
    assert_eq!(roundtripped.revision, Some(7));

    let body = roundtripped.body.as_ref().expect("body survives the seam");
    let source = record.body.as_ref().expect("fixture carries a body");
    assert_eq!(body.summary, source.summary);
    assert_eq!(body.tags, source.tags);
    assert_eq!(body.state, source.state);
}

#[tokio::test]
async fn stub_put_get_round_trip_and_missing_id_rejects() {
    let port = StubKnowledgeEntryPort::new();
    let wire = knowledge_record_to_spoke(&executable_record());

    match port.put_knowledge_entry(wire.clone(), None).await {
        SpokeResult::Ok(stored) => assert_eq!(json!(stored), json!(wire)),
        SpokeResult::Reject(reject) => panic!("put was rejected: {reject:?}"),
    }
    assert_eq!(port.entry_count(), 1);

    match port.get_knowledge_entry("kb_v232_rt").await {
        SpokeResult::Ok(found) => assert_eq!(json!(found), json!(wire)),
        SpokeResult::Reject(reject) => panic!("stored id was not found: {reject:?}"),
    }

    match port.get_knowledge_entry("kb_absent").await {
        SpokeResult::Reject(reject) => {
            assert_eq!(reject.code, SpokeRejectCode::KnowledgeEntryNotFound);
        }
        SpokeResult::Ok(entry) => panic!("a missing id answered with {entry:?}"),
    }
    assert_eq!(port.entry_count(), 1, "a miss must not insert an entry");
}

#[tokio::test]
async fn mutation_log_records_intents_in_order_and_drains() {
    let port = StubKnowledgeEntryPort::new();

    port.put_knowledge_entry(knowledge_record_to_spoke(&executable_record()), Some(6))
        .await;
    port.get_knowledge_entry("kb_v232_rt").await;
    port.get_knowledge_entry("kb_absent").await;

    let expected = vec![
        StubKnowledgeEntryMutation::Put {
            entry_id: "kb_v232_rt".to_string(),
            expected_base_revision: Some(6),
        },
        StubKnowledgeEntryMutation::Get {
            entry_id: "kb_v232_rt".to_string(),
        },
        StubKnowledgeEntryMutation::Get {
            entry_id: "kb_absent".to_string(),
        },
    ];
    assert_eq!(
        port.mutations(),
        expected,
        "every call is logged in call order, refused ones included"
    );

    assert_eq!(port.drain_mutations(), expected);
    assert!(
        port.mutations().is_empty(),
        "drain_mutations leaves the log empty"
    );
}
