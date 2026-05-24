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

/// Inventory row exists for this location and prize (including `totalQuantity == 0`).
pub fn is_prize_allocated_at_location(
    prize_id: &str,
    slot_by_prize: &HashMap<String, InventorySlot>,
) -> bool {
    slot_by_prize.contains_key(prize_id)
}

/// Inventory slot has stock configured (`totalQuantity > 0`) — used for award eligibility.
pub fn is_prize_configured_at_location(
    prize_id: &str,
    slot_by_prize: &HashMap<String, InventorySlot>,
) -> bool {
    slot_by_prize
        .get(prize_id)
        .is_some_and(|slot| slot.total_quantity > 0)
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
            if is_prize_configured_at_location(&prize_id, slot_by_prize) {
                consolation.push((prize.clone(), prize_id));
            }
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

/// Last-resort spin outcome when the weighted pool is empty. Only consolation prizes
/// may be awarded here — never a real prize without inventory (see `claim_tx`).
pub fn pick_wheel_fallback(prizes: &[Map<String, Value>]) -> Option<SpinPoolEntry> {
    for prize in prizes {
        if is_consolation_prize(prize) {
            if let Some(entry) = prize_entry(prize) {
                return Some(entry);
            }
        }
    }
    None
}

/// Prizes shown on the wheel — every campaign prize with an inventory row at this location
/// (including exhausted or zero-quantity slots). Award eligibility is handled separately
/// by `partition_spin_pool` and `claim_tx`.
pub fn wheel_display_prizes(
    prizes: &[Map<String, Value>],
    slot_by_prize: &HashMap<String, InventorySlot>,
    _campaign: &Campaign,
    _now: i64,
) -> Vec<Map<String, Value>> {
    let mut out: Vec<Map<String, Value>> = prizes
        .iter()
        .filter(|prize| {
            prize_id_from_map(prize)
                .is_some_and(|id| is_prize_allocated_at_location(&id, slot_by_prize))
        })
        .cloned()
        .collect();

    out.sort_by_key(|p| p.get("order").and_then(|v| v.as_i64()).unwrap_or(0));
    out
}

/// Campaigns without geofence locations show every configured prize on the wheel.
pub fn wheel_display_prizes_no_geofence(prizes: &[Map<String, Value>]) -> Vec<Map<String, Value>> {
    let mut out = prizes.to_vec();
    out.sort_by_key(|p| p.get("order").and_then(|v| v.as_i64()).unwrap_or(0));
    out
}

/// Spin pool for campaigns without geofence locations — consolation prizes only.
pub fn partition_spin_pool_no_geofence(
    prizes: &[Map<String, Value>],
) -> (Vec<ClaimableRealEntry>, Vec<SpinPoolEntry>) {
    let consolation = prizes
        .iter()
        .filter(|prize| is_consolation_prize(prize))
        .filter_map(|prize| prize_entry(prize))
        .collect();
    (Vec::new(), consolation)
}

/// Public wheel payload — strips Firestore metadata.
pub fn prize_to_wheel_json(prize: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(id) = prize_id_from_map(prize) {
        out.insert("id".into(), Value::String(id));
    }
    if let Some(name) = prize.get("name") {
        out.insert("name".into(), name.clone());
    }
    if let Some(order) = prize.get("order") {
        out.insert("order".into(), order.clone());
    }
    if let Some(is_real) = prize.get("isRealPrize") {
        out.insert("isRealPrize".into(), is_real.clone());
    } else {
        out.insert("isRealPrize".into(), Value::Bool(true));
    }
    if let Some(image_url) = prize
        .get("imageUrl")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.insert("imageUrl".into(), Value::String(image_url.to_string()));
    }
    out
}

pub fn has_consolation_at_location(
    prizes: &[Map<String, Value>],
    slot_by_prize: &HashMap<String, InventorySlot>,
) -> bool {
    prizes.iter().any(|p| {
        is_consolation_prize(p)
            && prize_id_from_map(p)
                .is_some_and(|id| is_prize_configured_at_location(&id, slot_by_prize))
    })
}

pub fn prize_entry(prize: &Map<String, Value>) -> Option<SpinPoolEntry> {
    let id = prize_id_from_map(prize)?;
    Some((prize.clone(), id))
}

pub fn spin_prize_from_entry(
    entry: &SpinPoolEntry,
    award_as_real: bool,
) -> (String, String, i64, bool) {
    let (won, prize_id) = entry;
    let prize_name = won.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let order = won.get("order").and_then(|v| v.as_i64()).unwrap_or(0);
    let is_real = award_as_real && is_real_prize(won);
    (prize_id.clone(), prize_name.to_string(), order, is_real)
}
