use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Firestore `locationId` for prize inventory not tied to a geo location.
pub const CAMPAIGN_WIDE_LOCATION_ID: &str = "__campaign__";
pub const CAMPAIGN_WIDE_LOCATION_NAME: &str = "Campaign-wide";

pub fn is_campaign_wide_location(location_id: &str) -> bool {
    location_id.trim() == CAMPAIGN_WIDE_LOCATION_ID
}

/// Normalizes admin input: missing, blank, or sentinel → campaign-wide scope.
pub fn normalize_inventory_location_id(location_id: &str) -> &str {
    let trimmed = location_id.trim();
    if trimmed.is_empty() || is_campaign_wide_location(trimmed) {
        CAMPAIGN_WIDE_LOCATION_ID
    } else {
        trimmed
    }
}

/// Location key used when reconciling inventory against submissions.
pub fn submission_inventory_location_key(row: &Map<String, Value>) -> String {
    row.get("locationId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| CAMPAIGN_WIDE_LOCATION_ID.to_string())
}

pub fn submission_matches_inventory_location(
    row: &Map<String, Value>,
    inventory_location_id: &str,
) -> bool {
    let sub_location = row
        .get("locationId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if is_campaign_wide_location(inventory_location_id) {
        sub_location.is_none() || sub_location == Some(CAMPAIGN_WIDE_LOCATION_ID)
    } else {
        sub_location == Some(inventory_location_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySlot {
    pub id: String,
    #[serde(rename = "locationId")]
    pub location_id: String,
    #[serde(rename = "prizeId")]
    pub prize_id: String,
    #[serde(rename = "totalQuantity")]
    pub total_quantity: i64,
    #[serde(rename = "awardedCount")]
    pub awarded_count: i64,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

/// Firestore `.update().object()` replaces the whole document in our client — merge fields
/// so `locationId`, `prizeId`, and `totalQuantity` survive awarded-count updates.
/// Returns `(location_id, prize_id)` when a submission consumed real-prize inventory.
pub fn resolve_inventory_decrement(
    sub: &Map<String, Value>,
    prizes: &[Map<String, Value>],
) -> Option<(String, String)> {
    let prize_name = sub.get("prize").and_then(|v| v.as_str())?;
    let location_id = submission_inventory_location_key(sub);
    if prize_name == "pending" || prize_name == "Nothing" {
        return None;
    }
    let prize = prizes.iter().find(|p| {
        p.get("name").and_then(|n| n.as_str()) == Some(prize_name)
            && p.get("isRealPrize")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
    })?;
    let prize_id = prize.get("id").and_then(|v| v.as_str())?;
    Some((location_id, prize_id.to_string()))
}

pub fn merge_inventory_slot_fields(
    existing: &Map<String, Value>,
    awarded_count: i64,
    updated_at: i64,
) -> Map<String, Value> {
    let mut merged: Map<String, Value> = existing
        .iter()
        .filter(|(k, _)| !k.starts_with("_firestore"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    merged.insert("awardedCount".into(), json!(awarded_count));
    merged.insert("updatedAt".into(), json!(updated_at));
    merged
}

impl InventorySlot {
    pub fn slot_key(location_id: &str, prize_id: &str) -> String {
        format!("{location_id}_{prize_id}")
    }

    pub fn remaining(&self, releasable_now: i64) -> i64 {
        let cap = releasable_now.min(self.total_quantity);
        (cap - self.awarded_count).max(0)
    }

    pub fn is_claimable(&self, releasable_now: i64) -> bool {
        self.remaining(releasable_now) > 0
    }
}

#[derive(Debug, Clone)]
pub struct InventoryView {
    pub location_id: String,
    pub location_name: String,
    pub prize_id: String,
    pub prize_name: String,
    pub total_quantity: i64,
    pub awarded_count: i64,
    pub releasable_now: i64,
    pub remaining: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_inventory_location_id_maps_blank_to_campaign_wide() {
        assert_eq!(normalize_inventory_location_id(""), CAMPAIGN_WIDE_LOCATION_ID);
        assert_eq!(
            normalize_inventory_location_id(CAMPAIGN_WIDE_LOCATION_ID),
            CAMPAIGN_WIDE_LOCATION_ID
        );
        assert_eq!(normalize_inventory_location_id("venue-1"), "venue-1");
    }

    #[test]
    fn submission_matches_campaign_wide_inventory() {
        let no_location = Map::from_iter([("locationId".into(), json!(null))]);
        assert!(submission_matches_inventory_location(
            &no_location,
            CAMPAIGN_WIDE_LOCATION_ID
        ));

        let venue = Map::from_iter([("locationId".into(), json!("venue-1"))]);
        assert!(!submission_matches_inventory_location(
            &venue,
            CAMPAIGN_WIDE_LOCATION_ID
        ));
    }

    #[test]
    fn merge_inventory_slot_fields_preserves_metadata() {
        let existing = Map::from_iter([
            ("locationId".into(), json!("loc1")),
            ("prizeId".into(), json!("prize1")),
            ("totalQuantity".into(), json!(5)),
            ("awardedCount".into(), json!(1)),
        ]);
        let merged = merge_inventory_slot_fields(&existing, 2, 99);
        assert_eq!(merged.get("totalQuantity").and_then(|v| v.as_i64()), Some(5));
        assert_eq!(merged.get("awardedCount").and_then(|v| v.as_i64()), Some(2));
    }
}
