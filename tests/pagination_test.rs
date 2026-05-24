use powerupgameon_api::features::locations::domain::GeoStatus;
use powerupgameon_api::utils::firestore::{
    build_page_cursor, document_ref_path, parse_page_cursor,
};

#[test]
fn document_ref_path_includes_parent_and_subcollection() {
    let path = document_ref_path(
        "projects/p/databases/(default)/documents/campaigns/c1",
        "registrations",
        "session-1",
    );
    assert_eq!(
        path,
        "projects/p/databases/(default)/documents/campaigns/c1/registrations/session-1"
    );
}

#[test]
fn pagination_cursor_round_trips_timestamp_and_name() {
    let parent = "projects/p/databases/(default)/documents/campaigns/c1";
    let mut row = serde_json::Map::new();
    row.insert(
        "registeredAt".into(),
        serde_json::json!(1_700_000_000_000_i64),
    );
    row.insert(
        "__name__".into(),
        serde_json::json!(format!("{parent}/registrations/s1")),
    );

    let cursor = build_page_cursor(&row, "registeredAt", parent, "registrations").expect("cursor");
    let (ts, name) = parse_page_cursor(&cursor, "registeredAt").expect("parse");
    assert_eq!(ts, 1_700_000_000_000);
    assert!(name.ends_with("/registrations/s1"));
}

#[test]
fn geo_status_from_str_maps_known_values() {
    assert_eq!(GeoStatus::from_str("valid"), GeoStatus::Valid);
    assert_eq!(GeoStatus::from_str("outside"), GeoStatus::Outside);
    assert_eq!(GeoStatus::from_str("no_zones"), GeoStatus::NoZones);
    assert_eq!(GeoStatus::from_str("unknown"), GeoStatus::NoZones);
}
