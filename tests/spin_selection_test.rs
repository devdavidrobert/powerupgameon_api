use powerupgameon_api::features::campaigns::domain::{
    Campaign, CampaignStatus, GeoEnforcement, StaggerMode,
};
use powerupgameon_api::features::inventory::domain::InventorySlot;
use powerupgameon_api::features::spin::application::spin_service::{
    LocationPoolSnapshot, SchedulePressureMetrics,
};
use powerupgameon_api::features::spin::application::{
    SpinService, CONSOLATION_BIAS_FACTOR, MAX_CLAIM_RETRIES, MIN_REAL_WEIGHT, REAL_BIAS_FACTOR,
};
use powerupgameon_api::features::spin::domain::{
    has_consolation_prize, is_consolation_prize, partition_spin_pool, pick_wheel_fallback,
    prize_id_from_map, ClaimableRealEntry,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::{json, Map};
use std::collections::{HashMap, HashSet};

fn sample_campaign(start: i64, end: i64) -> Campaign {
    Campaign {
        id: "camp-1".into(),
        slug: "test".into(),
        name: "Test".into(),
        status: CampaignStatus::Active,
        challenge_start_time: Some(start),
        challenge_end_time: Some(end),
        stagger_mode: StaggerMode::Immediate,
        stagger_schedule: None,
        geo_enforcement: GeoEnforcement::Reject,
        spin_pass_percent: 100,
        created_at: None,
        updated_at: None,
    }
}

fn prize(name: &str, id: &str, order: i64, is_real: bool) -> Map<String, serde_json::Value> {
    Map::from_iter([
        ("name".into(), json!(name)),
        ("id".into(), json!(id)),
        ("order".into(), json!(order)),
        ("isRealPrize".into(), json!(is_real)),
    ])
}

fn slot(location: &str, prize_id: &str, total: i64, awarded: i64) -> InventorySlot {
    InventorySlot {
        id: format!("{location}_{prize_id}"),
        location_id: location.into(),
        prize_id: prize_id.into(),
        total_quantity: total,
        awarded_count: awarded,
        updated_at: None,
    }
}

#[test]
fn partition_includes_consolation_when_real_inventory_is_exhausted() {
    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("So Close", "p2", 2, false),
    ];
    let slots = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 1, 1)),
        ("p2".into(), slot("loc-a", "p2", 1, 0)),
    ]);
    let excluded = HashSet::new();
    let campaign = sample_campaign(0, 1000);

    let (real, consolation) = partition_spin_pool(&prizes, &slots, &campaign, 500, &excluded);

    assert!(real.is_empty());
    assert_eq!(consolation.len(), 1);
    assert_eq!(
        consolation[0].0.get("name").and_then(|v| v.as_str()),
        Some("So Close")
    );
}

#[test]
fn partition_separates_claimable_real_and_consolation() {
    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("So Close", "p2", 2, false),
    ];
    let slots = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 5, 0)),
        ("p2".into(), slot("loc-a", "p2", 1, 0)),
    ]);
    let excluded = HashSet::new();
    let campaign = sample_campaign(0, 1000);

    let (real, consolation) = partition_spin_pool(&prizes, &slots, &campaign, 500, &excluded);

    assert_eq!(real.len(), 1);
    assert_eq!(real[0].remaining, 5);
    assert_eq!(consolation.len(), 1);
}

#[test]
fn partition_is_location_scoped_via_slot_map() {
    let prizes = vec![prize("Steam Can", "p1", 1, true)];
    let slots_loc_a = HashMap::from([("p1".into(), slot("loc-a", "p1", 5, 0))]);
    let slots_loc_b = HashMap::from([("p1".into(), slot("loc-b", "p1", 0, 5))]);
    let excluded = HashSet::new();
    let campaign = sample_campaign(0, 1000);

    let (real_a, _) = partition_spin_pool(&prizes, &slots_loc_a, &campaign, 500, &excluded);
    let (real_b, _) = partition_spin_pool(&prizes, &slots_loc_b, &campaign, 500, &excluded);

    assert_eq!(real_a.len(), 1);
    assert!(real_b.is_empty());
}

#[test]
fn has_consolation_prize_detects_flagged_entries() {
    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("Try Again", "p2", 2, false),
    ];
    assert!(has_consolation_prize(&prizes));
}

#[test]
fn consolation_requires_explicit_flag() {
    let mut p = prize("So Close", "p2", 2, true);
    p.remove("isRealPrize");
    assert!(!is_consolation_prize(&p));

    let consolation = prize("So Close", "p2", 2, false);
    assert!(is_consolation_prize(&consolation));
}

#[test]
fn pick_wheel_fallback_never_awards_real_prize_without_consolation() {
    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("Merch", "p2", 2, true),
    ];
    assert!(pick_wheel_fallback(&prizes).is_none());
}

#[test]
fn wheel_display_prizes_excludes_real_without_inventory_slot() {
    use powerupgameon_api::features::spin::domain::wheel_display_prizes;

    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("test_prize", "p3", 2, true),
        prize("So Close", "p2", 3, false),
    ];
    let slots = HashMap::from([("p1".into(), slot("loc-a", "p1", 5, 0))]);
    let campaign = sample_campaign(0, 1000);

    let wheel = wheel_display_prizes(&prizes, &slots, &campaign, 500);
    let names: Vec<&str> = wheel
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(names, vec!["Steam Can"]);
}

#[test]
fn wheel_display_includes_configured_consolation_at_location() {
    use powerupgameon_api::features::spin::domain::wheel_display_prizes;

    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("So Close", "p2", 3, false),
    ];
    let slots = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 5, 0)),
        ("p2".into(), slot("loc-a", "p2", 1, 0)),
    ]);
    let campaign = sample_campaign(0, 1000);

    let wheel = wheel_display_prizes(&prizes, &slots, &campaign, 500);
    let names: Vec<&str> = wheel
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(names, vec!["Steam Can", "So Close"]);
}

#[test]
fn wheel_display_includes_zero_quantity_allocated_slots() {
    use powerupgameon_api::features::spin::domain::wheel_display_prizes;

    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("Sold Out", "p2", 2, true),
        prize("So Close", "p3", 3, false),
    ];
    let slots = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 5, 0)),
        ("p2".into(), slot("loc-a", "p2", 0, 0)),
        ("p3".into(), slot("loc-a", "p3", 1, 0)),
    ]);
    let campaign = sample_campaign(0, 1000);

    let wheel = wheel_display_prizes(&prizes, &slots, &campaign, 500);
    let names: Vec<&str> = wheel
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(names, vec!["Steam Can", "Sold Out", "So Close"]);
}

#[test]
fn wheel_display_includes_exhausted_real_prize() {
    use powerupgameon_api::features::spin::domain::wheel_display_prizes;

    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("So Close", "p2", 2, false),
    ];
    let slots = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 5, 5)),
        ("p2".into(), slot("loc-a", "p2", 1, 0)),
    ]);
    let campaign = sample_campaign(0, 1000);

    let wheel = wheel_display_prizes(&prizes, &slots, &campaign, 500);
    let names: Vec<&str> = wheel
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(names, vec!["Steam Can", "So Close"]);
}

#[test]
fn schedule_pressure_favors_real_when_behind_schedule() {
    let campaign = sample_campaign(0, 1000);
    let now = 500;
    let real_slots = vec![slot("loc-a", "p1", 10, 0)];
    let real_claimable = vec![ClaimableRealEntry {
        prize: prize("Steam Can", "p1", 1, true),
        prize_id: "p1".into(),
        remaining: 10,
    }];

    let metrics = SpinService::compute_schedule_pressure_metrics(
        &campaign,
        &real_slots.iter().collect::<Vec<_>>(),
        &real_claimable,
        now,
    );

    assert!(
        metrics.deficit > 0.0,
        "mid-campaign with zero awards should be behind"
    );
    assert!(
        metrics.real_weight > metrics.consolation_weight,
        "behind schedule should favor real wins"
    );
}

#[test]
fn schedule_pressure_favors_consolation_when_ahead_of_schedule() {
    let campaign = sample_campaign(0, 1000);
    let now = 100;
    let real_slots = vec![slot("loc-a", "p1", 10, 8)];
    let real_claimable = vec![ClaimableRealEntry {
        prize: prize("Steam Can", "p1", 1, true),
        prize_id: "p1".into(),
        remaining: 2,
    }];

    let metrics = SpinService::compute_schedule_pressure_metrics(
        &campaign,
        &real_slots.iter().collect::<Vec<_>>(),
        &real_claimable,
        now,
    );

    assert!(
        metrics.deficit < 0.0,
        "early campaign with most awards should be ahead"
    );
    assert!(
        metrics.consolation_weight > 1.0,
        "ahead of schedule should increase consolation weight"
    );
}

#[test]
fn select_entry_picks_real_when_rng_favors_real_weight() {
    let snapshot = LocationPoolSnapshot {
        real_claimable: vec![ClaimableRealEntry {
            prize: prize("Steam Can", "p1", 1, true),
            prize_id: "p1".into(),
            remaining: 10,
        }],
        consolation: vec![(prize("So Close", "p2", 2, false), "p2".into())],
        metrics: SchedulePressureMetrics {
            total_releasable_budget: 10,
            total_awarded: 0,
            total_remaining: 10,
            campaign_elapsed: 0.5,
            schedule_pressure: 0.0,
            deficit: 0.5,
            real_weight: 100.0,
            consolation_weight: 1.0,
        },
    };
    let mut rng = StdRng::seed_from_u64(1);
    let entry = SpinService::select_entry(&snapshot, &mut rng);
    assert_eq!(entry.1, "p1");
}

#[test]
fn pick_real_weighted_respects_remaining_counts() {
    let entries = vec![
        ClaimableRealEntry {
            prize: prize("Small", "p1", 1, true),
            prize_id: "p1".into(),
            remaining: 1,
        },
        ClaimableRealEntry {
            prize: prize("Large", "p2", 2, true),
            prize_id: "p2".into(),
            remaining: 9,
        },
    ];
    let mut counts = HashMap::new();
    let mut rng = StdRng::seed_from_u64(42);
    for _ in 0..1000 {
        let entry = SpinService::pick_real_weighted(&entries, &mut rng);
        *counts.entry(entry.1).or_insert(0) += 1;
    }
    assert!(counts.get("p2").copied().unwrap_or(0) > counts.get("p1").copied().unwrap_or(0));
}

#[test]
fn max_claim_retries_constant_is_eight() {
    assert_eq!(MAX_CLAIM_RETRIES, 8);
}

#[test]
fn schedule_pressure_constants_are_sane() {
    assert!(REAL_BIAS_FACTOR > 0.0);
    assert!(MIN_REAL_WEIGHT > 0.0);
    assert!(CONSOLATION_BIAS_FACTOR > 0.0);
}

#[test]
fn build_location_pool_snapshot_uses_only_passed_slots() {
    let campaign = sample_campaign(0, 1000);
    let prizes = vec![
        prize("Steam Can", "p1", 1, true),
        prize("So Close", "p2", 2, false),
    ];
    let slot_map = HashMap::from([
        ("p1".into(), slot("loc-a", "p1", 3, 0)),
        ("p2".into(), slot("loc-a", "p2", 1, 0)),
    ]);
    let excluded = HashSet::new();

    let snapshot =
        SpinService::build_location_pool_snapshot(&campaign, &slot_map, &prizes, 500, &excluded);

    assert_eq!(snapshot.real_claimable.len(), 1);
    assert_eq!(snapshot.consolation.len(), 1);
    assert_eq!(snapshot.metrics.total_remaining, 3);
}

#[test]
fn prize_id_from_map_reads_id_field() {
    let p = prize("Test", "abc-123", 1, true);
    assert_eq!(prize_id_from_map(&p).as_deref(), Some("abc-123"));
}

#[test]
fn consolation_never_included_in_releasable_budget() {
    let campaign = sample_campaign(0, 1000);
    let real_slots = vec![slot("loc-a", "p1", 5, 1)];
    let real_claimable = vec![ClaimableRealEntry {
        prize: prize("Steam Can", "p1", 1, true),
        prize_id: "p1".into(),
        remaining: 4,
    }];

    let metrics = SpinService::compute_schedule_pressure_metrics(
        &campaign,
        &real_slots.iter().collect::<Vec<_>>(),
        &real_claimable,
        500,
    );

    assert_eq!(metrics.total_releasable_budget, 5);
    assert_eq!(metrics.total_awarded, 1);
    assert_eq!(metrics.total_remaining, 4);
}
