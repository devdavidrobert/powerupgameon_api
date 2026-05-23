use powerupgameon_api::features::campaigns::presentation::extract_slug_from_path;

#[test]
fn slug_extraction_from_nested_paths() {
    assert_eq!(
        extract_slug_from_path("/api/campaigns/summer-2026/questions").unwrap(),
        "summer-2026"
    );
    assert_eq!(
        extract_slug_from_path("/api/campaigns/test2/settings").unwrap(),
        "test2"
    );
}

use powerupgameon_api::features::campaigns::domain::{Campaign, CampaignStatus, GeoEnforcement, StaggerMode};

#[test]
fn draft_campaign_is_not_publicly_accessible() {
    let campaign = Campaign {
        id: "1".into(),
        slug: "draft".into(),
        name: "Draft".into(),
        status: CampaignStatus::Draft,
        challenge_start_time: None,
        challenge_end_time: None,
        stagger_mode: StaggerMode::Immediate,
        stagger_schedule: None,
        geo_enforcement: GeoEnforcement::Reject,
        created_at: None,
        updated_at: None,
    };
    assert!(!campaign.is_publicly_accessible());
}

#[test]
fn active_campaign_is_publicly_accessible() {
    let campaign = Campaign {
        id: "1".into(),
        slug: "live".into(),
        name: "Live".into(),
        status: CampaignStatus::Active,
        challenge_start_time: None,
        challenge_end_time: None,
        stagger_mode: StaggerMode::Immediate,
        stagger_schedule: None,
        geo_enforcement: GeoEnforcement::Reject,
        created_at: None,
        updated_at: None,
    };
    assert!(campaign.is_publicly_accessible());
}
