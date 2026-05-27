use powerupgameon_api::features::inventory::domain::{
    normalize_inventory_location_id, CAMPAIGN_WIDE_LOCATION_ID,
};
use powerupgameon_api::features::inventory::presentation::{
    DeleteInventoryBody, UpsertInventoryBody,
};

#[test]
fn upsert_inventory_body_location_id_is_optional() {
    let body: UpsertInventoryBody =
        serde_json::from_value(serde_json::json!({ "prizeId": "p1", "totalQuantity": 5 }))
            .unwrap();
    assert!(body.location_id.is_none());
}

#[test]
fn delete_inventory_body_location_id_is_optional() {
    let body: DeleteInventoryBody =
        serde_json::from_value(serde_json::json!({ "prizeId": "p1" })).unwrap();
    assert!(body.location_id.is_none());
}

#[test]
fn missing_location_id_normalizes_to_campaign_wide() {
    assert_eq!(
        normalize_inventory_location_id(""),
        CAMPAIGN_WIDE_LOCATION_ID
    );
}
