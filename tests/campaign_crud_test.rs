//! Live Firestore integration test for campaign CRUD.
//! Run: cargo test --test campaign_crud_test -- --ignored --nocapture

use powerupgameon_api::app_state::AppState;
use powerupgameon_api::config::Config;
use powerupgameon_api::features::campaigns::domain::CampaignStatus;
use powerupgameon_api::features::campaigns::infrastructure::{
    campaign_to_json, map_campaign, CampaignRepository, CAMPAIGNS_COLLECTION,
};
use powerupgameon_api::init_crypto_providers;
use serde_json::{json, Map, Value};
use std::sync::Arc;

async fn test_state() -> Arc<AppState> {
    dotenvy::dotenv().ok();
    init_crypto_providers().expect("crypto providers");
    let config = Config::load().expect("config");
    AppState::new(config).await.expect("app state")
}

fn unique_slug(prefix: &str) -> String {
    format!("{}-{}", prefix, uuid::Uuid::new_v4().simple())
}

async fn read_raw_campaign(state: &AppState, id: &str) -> Option<Map<String, Value>> {
    state
        .db
        .client
        .fluent()
        .select()
        .by_id_in(CAMPAIGNS_COLLECTION)
        .obj()
        .one(id)
        .await
        .expect("firestore read")
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn campaign_create_persists_name_in_firestore() {
    let state = test_state().await;
    let slug = unique_slug("e2e-create");
    let name = "E2E Create Name Test";

    let created = CampaignRepository::create(&state, &slug, name, CampaignStatus::Draft)
        .await
        .expect("create");

    assert_eq!(created.name, name);
    assert_eq!(created.slug, slug);

    let raw = read_raw_campaign(&state, &created.id)
        .await
        .expect("raw doc missing after create");
    assert_eq!(
        raw.get("name").and_then(|v| v.as_str()),
        Some(name),
        "name field missing in Firestore after create: {raw:?}"
    );

    let listed = CampaignRepository::find_all(&state).await.expect("list");
    assert!(
        listed.iter().any(|c| c.id == created.id && c.name == name),
        "created campaign not found in list with name"
    );
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn campaign_update_persists_renamed_name() {
    let state = test_state().await;
    let slug = unique_slug("e2e-update");
    let created = CampaignRepository::create(&state, &slug, "Original Name", CampaignStatus::Draft)
        .await
        .expect("create");

    let updated_name = "Renamed Campaign";
    let mut payload = Map::new();
    payload.insert("name".into(), json!(updated_name));

    let updated = CampaignRepository::update(&state, &created.id, payload)
        .await
        .expect("update");
    assert_eq!(updated.name, updated_name);

    let raw = read_raw_campaign(&state, &created.id)
        .await
        .expect("raw doc");
    assert_eq!(
        raw.get("name").and_then(|v| v.as_str()),
        Some(updated_name),
        "name not updated in Firestore: {raw:?}"
    );
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn campaign_read_by_slug_and_archive() {
    let state = test_state().await;
    let slug = unique_slug("e2e-archive");
    let name = "Archive Me";
    let created = CampaignRepository::create(&state, &slug, name, CampaignStatus::Active)
        .await
        .expect("create");

    let by_slug = CampaignRepository::find_by_slug(&state, &slug)
        .await
        .expect("find by slug")
        .expect("campaign not found by slug");
    assert_eq!(by_slug.name, name);

    CampaignRepository::archive(&state, &created.id)
        .await
        .expect("archive");

    let archived = CampaignRepository::find_by_id(&state, &created.id)
        .await
        .expect("find by id")
        .expect("campaign missing");
    assert_eq!(archived.status, CampaignStatus::Archived);

    let json = campaign_to_json(&archived);
    assert_eq!(json.get("name").and_then(|v| v.as_str()), Some(name));
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn partial_settings_update_preserves_name_and_slug() {
    let state = test_state().await;
    let slug = unique_slug("e2e-settings");
    let name = "Settings Preserve Test";
    let created = CampaignRepository::create(&state, &slug, name, CampaignStatus::Draft)
        .await
        .expect("create");

    let mut payload = Map::new();
    payload.insert("challengeStartTime".into(), json!(1_700_000_000_000_i64));
    payload.insert("challengeEndTime".into(), json!(1_800_000_000_000_i64));

    let updated = CampaignRepository::update(&state, &created.id, payload)
        .await
        .expect("settings update");
    assert_eq!(updated.name, name);
    assert_eq!(updated.slug, slug);

    let raw = read_raw_campaign(&state, &created.id)
        .await
        .expect("raw doc");
    assert_eq!(raw.get("name").and_then(|v| v.as_str()), Some(name));
    assert_eq!(
        raw.get("slug").and_then(|v| v.as_str()),
        Some(slug.as_str())
    );
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn partial_update_on_missing_campaign_does_not_create_ghost_doc() {
    let state = test_state().await;
    let ghost_id = uuid::Uuid::new_v4().to_string();

    let mut payload = Map::new();
    payload.insert("challengeStartTime".into(), json!(1_700_000_000_000_i64));
    payload.insert("challengeEndTime".into(), json!(1_800_000_000_000_i64));

    let result = CampaignRepository::update(&state, &ghost_id, payload).await;
    assert!(
        result.is_err(),
        "update on missing campaign must fail, got: {result:?}"
    );

    let raw = read_raw_campaign(&state, &ghost_id).await;
    assert!(
        raw.is_none(),
        "ghost document must not be created by partial update, found: {raw:?}"
    );
}

#[test]
fn map_campaign_requires_name_field() {
    let mut doc = Map::new();
    doc.insert("id".into(), json!("abc"));
    doc.insert("slug".into(), json!("test"));
    doc.insert("status".into(), json!("draft"));
    assert!(
        map_campaign(doc).is_none(),
        "docs without name must be skipped"
    );
}

#[test]
fn map_campaign_reads_name_field() {
    let mut doc = Map::new();
    doc.insert("id".into(), json!("abc"));
    doc.insert("slug".into(), json!("test"));
    doc.insert("name".into(), json!("My Campaign"));
    doc.insert("status".into(), json!("draft"));
    let campaign = map_campaign(doc).expect("should map");
    assert_eq!(campaign.name, "My Campaign");
}
