use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::get,
    Json, Router,
};
use powerupgameon_api::features::campaigns::domain::{
    Campaign, CampaignStatus, GeoEnforcement, StaggerMode,
};
use powerupgameon_api::features::campaigns::infrastructure::CampaignPaths;
use powerupgameon_api::features::campaigns::presentation::CampaignContext;
use powerupgameon_api::middleware::challenge_window::require_challenge_open_middleware;
use tower::ServiceExt;

fn closed_campaign_context() -> CampaignContext {
    CampaignContext {
        paths: CampaignPaths::new("camp-1"),
        campaign: Campaign {
            id: "camp-1".into(),
            slug: "test".into(),
            name: "Test".into(),
            status: CampaignStatus::Active,
            challenge_start_time: None,
            challenge_end_time: Some(chrono::Utc::now().timestamp_millis() - 60_000),
            stagger_mode: StaggerMode::Linear,
            stagger_schedule: None,
            geo_enforcement: GeoEnforcement::Reject,
            spin_pass_percent: 100,
            brand_logos: None,
            player_outcome_copy: None,
            registration_form_header: None,
            ip_rate_limit_window_secs: None,
            created_at: None,
            updated_at: None,
        },
    }
}

fn not_started_campaign_context() -> CampaignContext {
    CampaignContext {
        paths: CampaignPaths::new("camp-1"),
        campaign: Campaign {
            id: "camp-1".into(),
            slug: "test".into(),
            name: "Test".into(),
            status: CampaignStatus::Active,
            challenge_start_time: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
            challenge_end_time: None,
            stagger_mode: StaggerMode::Linear,
            stagger_schedule: None,
            geo_enforcement: GeoEnforcement::Reject,
            spin_pass_percent: 100,
            brand_logos: None,
            player_outcome_copy: None,
            registration_form_header: None,
            ip_rate_limit_window_secs: None,
            created_at: None,
            updated_at: None,
        },
    }
}

async fn inject_not_started_campaign(mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(not_started_campaign_context());
    next.run(req).await
}

async fn inject_closed_campaign(mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(closed_campaign_context());
    next.run(req).await
}

async fn ok_handler() -> Json<&'static str> {
    Json("ok")
}

#[tokio::test]
async fn require_challenge_open_blocks_closed_campaign_requests() {
    let app = Router::new()
        .route("/play", get(ok_handler))
        .layer(middleware::from_fn(require_challenge_open_middleware))
        .layer(middleware::from_fn(inject_closed_campaign));

    let response = app
        .oneshot(Request::builder().uri("/play").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "CHALLENGE_ENDED");
}

#[tokio::test]
async fn require_challenge_open_returns_start_time_when_not_started() {
    let app = Router::new()
        .route("/play", get(ok_handler))
        .layer(middleware::from_fn(require_challenge_open_middleware))
        .layer(middleware::from_fn(inject_not_started_campaign));

    let response = app
        .oneshot(Request::builder().uri("/play").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "CHALLENGE_NOT_STARTED");
    assert!(json["data"]["challengeStartTime"].is_number());
}

#[tokio::test]
async fn settings_route_pattern_stays_outside_challenge_open_middleware() {
    let settings = Router::new().route("/", get(ok_handler));
    let questions = Router::new()
        .route("/", get(ok_handler))
        .layer(middleware::from_fn(require_challenge_open_middleware))
        .layer(middleware::from_fn(inject_closed_campaign));

    let app = Router::new()
        .nest("/settings", settings)
        .nest("/questions", questions);

    let settings_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings_response.status(), StatusCode::OK);

    let questions_response = app
        .oneshot(
            Request::builder()
                .uri("/questions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(questions_response.status(), StatusCode::FORBIDDEN);
}
