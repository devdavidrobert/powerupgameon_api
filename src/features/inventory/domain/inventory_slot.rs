use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
