//! Consumes the packed `nexus-workflow-ir` archive: parses a real IR document,
//! validates it, hashes it (including the hash's own identity rules), then
//! parses a document with a dangling edge and asserts the exact structural
//! violation it must be refused with.

use std::path::PathBuf;

use nexus_workflow_ir::{content_hash, validate, Violation, WorkflowDocument};

/// Read and parse one committed IR document next to this program.
fn parse(name: &str) -> WorkflowDocument {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not a Workflow IR document: {error}", path.display()))
}

fn main() {
    let document = parse("document.json");
    assert_eq!(document.id.as_str(), "prompt-step");
    assert_eq!(document.entry, "prompt");
    assert_eq!(document.terminal, ["prompt"]);
    assert_eq!(document.nodes.len(), 1);
    assert_eq!(document.nodes[0].id, "prompt");
    assert_eq!(document.assets.len(), 1);
    assert_eq!(document.assets[0].id, "templates/prompt.md");
    assert!(document.edges.is_empty());

    // Q2 structural validation of the resolved set: this document is alone and
    // resolves no pattern body, so an empty violation list is the real answer.
    validate(&[&document], &[]).expect("the committed document is structurally valid");

    // Q3 canonical content hash: 64 lowercase hex, stable across calls, and
    // independent of its own `content_hash` field (the hash never covers itself).
    let hash = content_hash(&document).expect("the document canonicalizes");
    assert_eq!(hash.len(), 64, "{hash}");
    assert!(
        hash.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{hash} is not lowercase hex"
    );
    assert_eq!(content_hash(&document).expect("stable"), hash);

    let mut rehashed = document.clone();
    rehashed.content_hash = Some("0".repeat(64));
    assert_eq!(
        content_hash(&rehashed).expect("stable"),
        hash,
        "the hash must not cover its own field"
    );

    // A real content edit must move the identity, or approval would bind nothing.
    let mut edited = document.clone();
    edited.meta.description.push_str(" (edited)");
    assert_ne!(
        content_hash(&edited).expect("stable"),
        hash,
        "a content edit must change the canonical hash"
    );

    // A dangling edge: `ghost` is no node in this document, so the validator
    // must refuse it with the real structural violation, not a generic error.
    let dangling = parse("dangling.json");
    let violations = validate(&[&dangling], &[]).expect_err("a dangling edge is not valid");
    assert_eq!(
        violations,
        vec![Violation::DanglingEdge {
            document: "prompt-step".to_string(),
            edge: "e_dangling".to_string(),
            node: "ghost".to_string(),
        }]
    );
    assert_eq!(
        violations[0].to_string(),
        "`prompt-step`: edge `e_dangling` references unknown node `ghost`"
    );
    assert_ne!(
        content_hash(&dangling).expect("stable"),
        hash,
        "the dangling document is a different identity"
    );

    println!("nexus-workflow-ir validated {} ({hash})", document.id.as_str());
}
