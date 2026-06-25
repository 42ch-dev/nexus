//! Pagination-info serialization parity (V1.66 hygiene T3).
//!
//! Ensures that all daemon list handlers converge on the canonical
//! `nexus_contracts::PaginationInfo` envelope and that its JSON shape matches
//! the wire contract (required `limit`/`has_more`, optional `next_cursor`).

use nexus_contracts::PaginationInfo;

#[test]
fn pagination_info_serializes_to_expected_shape() {
    let info = PaginationInfo {
        limit: 50,
        next_cursor: Some("v2:1:2".to_string()),
        has_more: true,
    };
    let value = serde_json::to_value(&info).expect("serialize PaginationInfo");
    assert_eq!(value["limit"], 50);
    assert_eq!(value["next_cursor"], "v2:1:2");
    assert_eq!(value["has_more"], true);
}

#[test]
fn pagination_info_omits_null_next_cursor() {
    let info = PaginationInfo {
        limit: 50,
        next_cursor: None,
        has_more: false,
    };
    let json = serde_json::to_string(&info).expect("serialize PaginationInfo");
    assert!(!json.contains("next_cursor"));
}

#[test]
fn pagination_info_round_trips_through_json() {
    let info = PaginationInfo {
        limit: 250,
        next_cursor: Some("cursor_abc".to_string()),
        has_more: true,
    };
    let json = serde_json::to_string(&info).expect("serialize");
    let decoded: PaginationInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded, info);
}
