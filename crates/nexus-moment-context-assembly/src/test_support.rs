//! Contract-DTO fixture builders for this crate's unit tests (NXM-7).
//!
//! `RetainedRecord` is a generated upstream type with no literal constructor,
//! so the tests build one from its wire JSON — the same shape
//! `memory.recall` serves.

use nexus_spoke_adapter::spoke_schemas::RetainedRecord;
use serde_json::json;

/// One authored long-term memory: `nexus.<kind>` carrying a legacy
/// `memory_id`, with `text` as the rendered body.
pub fn memory(memory_id: &str, kind: &str, text: &str, updated_at: &str) -> RetainedRecord {
    record(&json!({
        "record_id": format!("rec_{memory_id}"),
        "kind": format!("nexus.{kind}"),
        "text": text,
        "structured": { "nexus_legacy": { "memory_id": memory_id } },
        "scope": { "visibility": "shared" },
        "at": updated_at,
    }))
}

/// One `fact` fragment record with its legacy origin metadata.
pub fn fragment(
    fragment_id: &str,
    summary: &str,
    keywords: &[&str],
    created_at: &str,
) -> RetainedRecord {
    record(&json!({
        "record_id": format!("lgc:character_memory_fragments:{fragment_id}"),
        "kind": "fact",
        "text": summary,
        "structured": { "nexus_legacy": { "fragment_id": fragment_id, "keywords": keywords } },
        "scope": { "visibility": "shared" },
        "at": created_at,
    }))
}

/// One recall record with no legacy metadata: the provider's own shape.
pub fn native(record_id: &str, kind: &str, text: &str, at: &str) -> RetainedRecord {
    record(&json!({
        "record_id": record_id,
        "kind": kind,
        "text": text,
        "at": at,
    }))
}

fn record(fields: &serde_json::Value) -> RetainedRecord {
    let mut content = json!({ "text": fields["text"] });
    if let Some(structured) = fields.get("structured").filter(|v| !v.is_null()) {
        content["structured"] = structured.clone();
    }
    let scope = fields
        .get("scope")
        .filter(|scope| !scope.is_null())
        .cloned()
        .unwrap_or_else(|| json!({ "visibility": "shared" }));
    let at = fields["at"].as_str().unwrap_or("1970-01-01T00:00:00Z");
    serde_json::from_value(json!({
        "record_id": fields["record_id"],
        "space_id": "spc_local_default",
        "subject_id": "sbj_test",
        "kind": fields["kind"],
        "content": content,
        "extensions": {},
        "scope": scope,
        "provenance": {
            "anchors": [],
            "captured_at": at,
            "captured_by": "nexus:legacy",
            "source_context_ids": [],
        },
        "created_at": at,
        "updated_at": at,
        "revision": 1,
        "schema_version": 1,
    }))
    .expect("retained record fixture")
}
