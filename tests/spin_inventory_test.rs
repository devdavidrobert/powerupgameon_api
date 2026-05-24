use powerupgameon_api::features::inventory::domain::InventorySlot;

#[test]
fn slot_key_format() {
    assert_eq!(InventorySlot::slot_key("loc-1", "prize-2"), "loc-1_prize-2");
}

#[test]
fn concurrent_claim_simulation_respects_cap() {
    let total = 5_i64;
    let mut awarded = 0_i64;
    let releasable = total;

    for _ in 0..20 {
        let slot = InventorySlot {
            id: "x".into(),
            location_id: "loc".into(),
            prize_id: "prize".into(),
            total_quantity: total,
            awarded_count: awarded,
            updated_at: None,
        };
        if slot.is_claimable(releasable) {
            awarded += 1;
        }
    }

    assert_eq!(awarded, total);
}

#[test]
fn staggered_cap_limits_early_claims() {
    let total = 10_i64;
    let releasable_now = 3_i64;
    let mut awarded = 0_i64;

    for _ in 0..10 {
        let slot = InventorySlot {
            id: "x".into(),
            location_id: "loc".into(),
            prize_id: "prize".into(),
            total_quantity: total,
            awarded_count: awarded,
            updated_at: None,
        };
        if slot.is_claimable(releasable_now) {
            awarded += 1;
        }
    }

    assert_eq!(awarded, 3);
}
