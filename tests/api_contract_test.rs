use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use powerupgameon_api::controllers::prizes::PrizeBody;
use powerupgameon_api::error::SuccessResponse;
use powerupgameon_api::features::campaigns::domain::{
    Campaign, CampaignStatus, GeoEnforcement, StaggerMode, StaggerStep,
};
use powerupgameon_api::features::campaigns::infrastructure::{
    build_update_payload, CampaignPaths, CampaignUpdateInput,
};
use powerupgameon_api::features::campaigns::presentation::{
    get_campaign_settings_admin, CampaignContext,
};
use powerupgameon_api::middleware::challenge_window::require_challenge_open_middleware;
use serde_json::{json, Value};
use tower::ServiceExt;

fn campaign_context(status: CampaignStatus) -> CampaignContext {
    CampaignContext {
        paths: CampaignPaths::new("camp-1"),
        campaign: Campaign {
            id: "camp-1".into(),
            slug: "test".into(),
            name: "Test".into(),
            status,
            challenge_start_time: None,
            challenge_end_time: Some(chrono::Utc::now().timestamp_millis() - 60_000),
            stagger_mode: StaggerMode::Stepped,
            stagger_schedule: Some(vec![StaggerStep {
                release_at: 1_700_000_000_000_i64,
                release_percent: 0.5,
            }]),
            geo_enforcement: GeoEnforcement::Flag,
            created_at: None,
            updated_at: Some(1_700_000_000_001_i64),
        },
    }
}

#[test]
fn success_response_serializes_pagination_as_camel_case() {
    let json = serde_json::to_value(SuccessResponse::<Vec<Value>> {
        success: true,
        data: Some(vec![]),
        message: None,
        code: None,
        next_cursor: Some("next-page".into()),
        has_more: Some(true),
    })
    .unwrap();

    assert_eq!(json["nextCursor"], "next-page");
    assert_eq!(json["hasMore"], true);
    assert!(json.get("next_cursor").is_none());
    assert!(json.get("has_more").is_none());
}

#[test]
fn prize_body_deserializes_is_real_prize_from_camel_case() {
    let body: PrizeBody =
        serde_json::from_value(json!({ "name": "So Close", "isRealPrize": false })).unwrap();

    assert_eq!(body.name.as_deref(), Some("So Close"));
    assert_eq!(body.is_real_prize, Some(false));
}

#[tokio::test]
async fn admin_settings_full_returns_private_fields_for_draft_campaign() {
    let Json(res) = get_campaign_settings_admin(campaign_context(CampaignStatus::Draft))
        .await
        .unwrap();
    let data = serde_json::to_value(res.data.unwrap()).unwrap();

    assert_eq!(data["staggerMode"], "stepped");
    assert_eq!(data["geoEnforcement"], "flag");
    assert!(data["staggerSchedule"].is_array());
}

async fn ok_handler() -> &'static str {
    "ok"
}

async fn inject_closed_active_campaign(mut req: Request<Body>, next: Next) -> Response {
    req.extensions_mut()
        .insert(campaign_context(CampaignStatus::Active));
    next.run(req).await
}

#[tokio::test]
async fn admin_full_route_pattern_bypasses_challenge_open_middleware() {
    let public_prizes = Router::new()
        .route("/", get(ok_handler))
        .layer(middleware::from_fn(require_challenge_open_middleware))
        .layer(middleware::from_fn(inject_closed_active_campaign));

    let admin_prizes = Router::new().route("/admin/full", get(ok_handler));
    let app = Router::new().nest("/prizes", public_prizes.merge(admin_prizes));

    let public_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/prizes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_response.status(), StatusCode::FORBIDDEN);

    let admin_response = app
        .oneshot(
            Request::builder()
                .uri("/prizes/admin/full")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(admin_response.status(), StatusCode::OK);
}

#[test]
fn build_update_payload_clears_stagger_schedule_for_non_stepped_modes() {
    use powerupgameon_api::features::campaigns::domain::StaggerMode;

    let payload = build_update_payload(&CampaignUpdateInput {
        stagger_mode: Some(StaggerMode::Immediate),
        clear_stagger_schedule: true,
        ..Default::default()
    })
    .unwrap();

    assert_eq!(payload.get("staggerSchedule"), Some(&Value::Null));
}

#[test]
fn config_allows_explicit_and_vercel_project_origins() {
    use powerupgameon_api::config::Config;

    let config = Config {
        port: 4000,
        node_env: "production".into(),
        is_production: true,
        firebase_project_id: None,
        firebase_service_account_json: None,
        allowed_origins: vec!["https://custom.example.com".into()],
        cors_vercel_project: Some("powerupgameon".into()),
        trust_proxy: true,
        rate_limit_window_ms: 900_000,
        rate_limit_max: 200,
        api_csrf_secret: "secret".into(),
        spin_token_secret: "secret".into(),
        spin_token_ttl_minutes: 60,
        redis_url: None,
        allowed_admin_emails: vec![],
        ip_geo_enabled: false,
        ip_geo_max_distance_km: 150.0,
        ip_geo_api_url: None,
    };

    assert!(config.is_origin_allowed("https://custom.example.com"));
    assert!(config.is_origin_allowed("https://powerupgameon.vercel.app"));
    assert!(config.is_origin_allowed("https://powerupgameon-git-main-user.vercel.app"));
    assert!(!config.is_origin_allowed("https://other.vercel.app"));
}
