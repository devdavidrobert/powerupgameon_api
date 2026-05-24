use powerupgameon_api::features::campaigns::domain::{
    Campaign, CampaignStatus, GeoEnforcement, StaggerMode, StaggerStep,
};
use powerupgameon_api::features::inventory::application::InventoryService;
use powerupgameon_api::features::inventory::domain::InventorySlot;

fn campaign_with_window(
    start: i64,
    end: i64,
    mode: StaggerMode,
    schedule: Option<Vec<StaggerStep>>,
) -> Campaign {
    Campaign {
        id: "c1".into(),
        slug: "test".into(),
        name: "Test".into(),
        status: CampaignStatus::Active,
        challenge_start_time: Some(start),
        challenge_end_time: Some(end),
        stagger_mode: mode,
        stagger_schedule: schedule,
        geo_enforcement: GeoEnforcement::Reject,
        spin_pass_percent: 100,
        created_at: None,
        updated_at: None,
    }
}

fn slot(total: i64, awarded: i64) -> InventorySlot {
    InventorySlot {
        id: "loc_prize".into(),
        location_id: "loc".into(),
        prize_id: "prize".into(),
        total_quantity: total,
        awarded_count: awarded,
        updated_at: None,
    }
}

#[test]
fn immediate_mode_releases_full_quantity() {
    let campaign = campaign_with_window(0, 1000, StaggerMode::Immediate, None);
    let s = slot(10, 0);
    assert_eq!(InventoryService::releasable_now(&campaign, &s, 500), 10);
}

#[test]
fn linear_mode_releases_half_at_midpoint() {
    let campaign = campaign_with_window(0, 1000, StaggerMode::Linear, None);
    let s = slot(10, 0);
    assert_eq!(InventoryService::releasable_now(&campaign, &s, 500), 5);
}

#[test]
fn linear_mode_releases_none_at_start() {
    let campaign = campaign_with_window(0, 1000, StaggerMode::Linear, None);
    let s = slot(10, 0);
    assert_eq!(InventoryService::releasable_now(&campaign, &s, 0), 0);
}

#[test]
fn stepped_mode_uses_schedule_percent() {
    let campaign = campaign_with_window(
        0,
        1000,
        StaggerMode::Stepped,
        Some(vec![
            StaggerStep {
                release_at: 0,
                release_percent: 0.25,
            },
            StaggerStep {
                release_at: 500,
                release_percent: 0.75,
            },
        ]),
    );
    let s = slot(100, 0);
    assert_eq!(InventoryService::releasable_now(&campaign, &s, 100), 25);
    assert_eq!(InventoryService::releasable_now(&campaign, &s, 600), 75);
}

#[test]
fn slot_remaining_respects_releasable_cap() {
    let s = slot(10, 3);
    assert_eq!(s.remaining(8), 5);
    assert_eq!(s.remaining(2), 0);
}

#[test]
fn slot_claimable_when_remaining_positive() {
    let s = slot(10, 3);
    assert!(s.is_claimable(8));
    assert!(!s.is_claimable(3));
}
