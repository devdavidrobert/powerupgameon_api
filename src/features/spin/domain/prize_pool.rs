use crate::features::campaigns::domain::Campaign;
use crate::features::inventory::application::InventoryService;
use crate::features::inventory::domain::InventorySlot;
use crate::utils::firestore::document_id_from_map;
use serde_json::{Map, Value};
use std::collections::HashMap;

pub type SpinPoolEntry = (Map<String, Value>, String);

#[derive(Debug, Clone)]
pub struct ClaimableRealEntry {
    pub prize: Map<String, Value>,
    pub prize_id: String,
    pub remaining: i64,
}

pub fn prize_id_from_map(prize: &Map<String, Value>) -> Option<String> {
    prize
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| document_id_from_map(prize))
}

pub fn is_consolation_prize(prize: &Map<String, Value>) -> bool {
    prize.get("isRealPrize").and_then(|v| v.as_bool()) == Some(false)
}

pub fn has_consolation_prize(prizes: &[Map<String, Value>]) -> bool {
    prizes.iter().any(is_consolation_prize)
}

pub fn is_real_prize(prize: &Map<String, Value>) -> bool {
    !is_consolation_prize(prize)
}

pub fn partition_spin_pool(
    prizes: &[Map<String, Value>],
    slot_by_prize: &HashMap<String, InventorySlot>,
    campaign: &Campaign,
    now: i64,
    excluded_real_ids: &std::collections::HashSet<String>,
) -> (Vec<ClaimableRealEntry>, Vec<SpinPoolEntry>) {
    let mut real_claimable = Vec::new();
    let mut consolation = Vec::new();

    for prize in prizes {
        let Some(prize_id) = prize_id_from_map(prize) else {
            continue;
        };

        if is_consolation_prize(prize) {
            consolation.push((prize.clone(), prize_id));
            continue;
        }

        if excluded_real_ids.contains(&prize_id) {
            continue;
        }

        if let Some(slot) = slot_by_prize.get(&prize_id) {
            let releasable = InventoryService::releasable_now(campaign, slot, now);
            let remaining = slot.remaining(releasable);
            if remaining > 0 {
                real_claimable.push(ClaimableRealEntry {
                    prize: prize.clone(),
                    prize_id,
                    remaining,
                });
            }
        }
    }

    (real_claimable, consolation)
}

pub fn pick_wheel_fallback(prizes: &[Map<String, Value>]) -> Option<SpinPoolEntry> {
    for prize in prizes {
        if is_consolation_prize(prize) {
            if let Some(entry) = prize_entry(prize) {
                return Some(entry);
            }
        }
    }

    prizes.iter().rev().filter_map(prize_entry).next()
}

pub fn prize_entry(prize: &Map<String, Value>) -> Option<SpinPoolEntry> {
    let id = prize_id_from_map(prize)?;
    Some((prize.clone(), id))
}

pub fn spin_prize_from_entry(entry: &SpinPoolEntry, award_as_real: bool) -> (String, String, i64, bool) {
    let (won, prize_id) = entry;
    let prize_name = won.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let order = won.get("order").and_then(|v| v.as_i64()).unwrap_or(0);
    let is_real = award_as_real && is_real_prize(won);
    (prize_id.clone(), prize_name.to_string(), order, is_real)
}
