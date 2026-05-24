use serde::{Deserialize, Serialize};

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
