//! Live Firestore integration test for question POST → PUT → DELETE lifecycle.
//! Run: cargo test --test question_lifecycle_test -- --ignored --nocapture

use powerupgameon_api::app_state::AppState;
use powerupgameon_api::config::Config;
use powerupgameon_api::features::campaigns::domain::CampaignStatus;
use powerupgameon_api::features::campaigns::infrastructure::{
    CampaignPaths, CampaignRepository,
};
use powerupgameon_api::init_crypto_providers;
use powerupgameon_api::models::question::QuestionModel;
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

fn question_payload(text: &str) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert("text".into(), json!(text));
    data.insert(
        "options".into(),
        json!(["A", "B", "C", "D"]),
    );
    data.insert("correctIndex".into(), json!(0));
    data.insert("order".into(), json!(1));
    data
}

#[tokio::test]
#[ignore = "requires FIREBASE_SERVICE_ACCOUNT_JSON"]
async fn question_create_update_delete_lifecycle() {
    let state = test_state().await;
    let slug = unique_slug("e2e-question");
    let campaign = CampaignRepository::create(
        &state,
        &slug,
        "Question Lifecycle Test",
        CampaignStatus::Active,
    )
    .await
    .expect("create campaign");
    let paths = CampaignPaths::new(campaign.id.clone());

    let created = QuestionModel::create(&state, &paths, question_payload("Original?"))
        .await
        .expect("create question");
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("create returned id")
        .to_string();

    let found = QuestionModel::find_by_id(&state, &paths, &id)
        .await
        .expect("find after create")
        .expect("question missing after create");
    assert_eq!(
        found.get("text").and_then(|v| v.as_str()),
        Some("Original?")
    );

    let mut updates = Map::new();
    updates.insert("text".into(), json!("Updated?"));
    let updated = QuestionModel::update(&state, &paths, &id, updates)
        .await
        .expect("update question");
    assert_eq!(
        updated.get("text").and_then(|v| v.as_str()),
        Some("Updated?")
    );

    QuestionModel::delete(&state, &paths, &id)
        .await
        .expect("delete question");
    assert!(
        QuestionModel::find_by_id(&state, &paths, &id)
            .await
            .expect("find after delete")
            .is_none()
    );
}
