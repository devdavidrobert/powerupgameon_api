use crate::app_state::AppState;
use crate::controllers::{auth, prizes, questions, raffles, registrations, submissions};
use crate::error::{json_error, SuccessResponse};
use crate::features::branding::presentation::upload_brand_logo;
use crate::features::campaigns::presentation::{
    archive_campaign, clear_campaign_timers, create_campaign, get_campaign, get_campaign_settings,
    get_campaign_settings_admin, list_campaigns, update_campaign, update_campaign_settings,
};
use crate::features::inventory::presentation::{delete_inventory, list_inventory, upsert_inventory};
use crate::features::locations::presentation::{
    create_location, delete_location, list_locations, update_location,
};
use crate::features::spin::presentation::{get_wheel_prizes, spin_wheel};
use crate::middleware::auth::{authenticate_middleware, require_admin_middleware};
use crate::middleware::campaign_context::inject_campaign_context;
use crate::middleware::challenge_window::require_challenge_open_middleware;
use crate::middleware::csrf::{mint_csrf_token, require_csrf_middleware};
use crate::middleware::rate_limit::{
    global_rate_limit_middleware, registration_rate_limit_middleware, spin_rate_limit_middleware,
    submission_rate_limit_middleware,
};
use crate::middleware::request_context::request_context_middleware;
use axum::{
    extract::State,
    http::{HeaderValue, Method, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

fn with_admin(state: Arc<AppState>, router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    router
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_admin_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state,
            authenticate_middleware,
        ))
}

fn with_challenge_open(router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    router.layer(middleware::from_fn(require_challenge_open_middleware))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors_state = state.clone();
    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderName::from_static("x-csrf-token"),
            axum::http::HeaderName::from_static("x-request-id"),
        ])
        .allow_credentials(true)
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin: &HeaderValue, _| {
                let Ok(origin_str) = origin.to_str() else {
                    return false;
                };
                cors_state.config.is_origin_allowed(origin_str)
            },
        ));

    let admin = state.clone();

    let api_questions = with_challenge_open(
        Router::new()
            .route("/", get(questions::get_all_questions))
            .route("/{id}", get(questions::get_question)),
    )
    .merge(with_admin(
        admin.clone(),
        Router::new()
            .route("/admin/full", get(questions::get_all_questions_admin))
            .route("/", post(questions::create_question))
            .route(
                "/{id}",
                put(questions::update_question).delete(questions::delete_question),
            )
            .route(
                "/{id}/options/{option_index}/image/upload",
                post(questions::upload_question_option_image)
                    .layer(RequestBodyLimitLayer::new(3 * 1024 * 1024)),
            ),
    ));

    let api_prizes = with_challenge_open(
        Router::new()
            .route("/", get(prizes::get_all_prizes))
            .route("/{id}", get(prizes::get_prize)),
    )
    .merge(with_admin(
        admin.clone(),
        Router::new()
            .route("/admin/full", get(prizes::get_all_prizes_admin))
            .route("/", post(prizes::create_prize))
            .route(
                "/{id}",
                put(prizes::update_prize).delete(prizes::delete_prize),
            )
            .route(
                "/{id}/image/upload",
                post(prizes::upload_prize_image).layer(RequestBodyLimitLayer::new(3 * 1024 * 1024)),
            ),
    ));

    let api_registrations = with_challenge_open(Router::new().route(
        "/",
        post(registrations::register).layer(middleware::from_fn_with_state(
            state.clone(),
            registration_rate_limit_middleware,
        )),
    ))
    .merge(with_admin(
        admin.clone(),
        Router::new()
            .route("/", get(registrations::get_all_registrations))
            .route("/{id}", delete(registrations::delete_registration)),
    ));

    let api_submissions = with_challenge_open(Router::new().route(
        "/",
        post(submissions::create_submission).layer(middleware::from_fn_with_state(
            state.clone(),
            submission_rate_limit_middleware,
        )),
    ))
    .merge(with_admin(
        admin.clone(),
        Router::new()
            .route("/", get(submissions::get_all_submissions))
            .route(
                "/{id}",
                get(submissions::get_submission).delete(submissions::delete_submission),
            ),
    ));

    let api_spin = with_challenge_open(
        Router::new()
            .route("/wheel-prizes", get(get_wheel_prizes))
            .route(
                "/",
                post(spin_wheel).layer(middleware::from_fn_with_state(
                    state.clone(),
                    spin_rate_limit_middleware,
                )),
            ),
    );

    let api_settings = Router::new()
        .route("/", get(get_campaign_settings))
        .merge(with_admin(
            admin.clone(),
            Router::new()
                .route("/admin/full", get(get_campaign_settings_admin))
                .route("/", put(update_campaign_settings))
                .route("/timers", delete(clear_campaign_timers))
                .route(
                    "/brand-logos/upload",
                    post(upload_brand_logo).layer(RequestBodyLimitLayer::new(3 * 1024 * 1024)),
                ),
        ));

    let api_locations = with_admin(
        admin.clone(),
        Router::new()
            .route("/", get(list_locations).post(create_location))
            .route("/{id}", put(update_location).delete(delete_location)),
    );

    let api_inventory = with_admin(
        admin.clone(),
        Router::new().route("/", get(list_inventory).put(upsert_inventory).delete(delete_inventory)),
    );

    let api_raffles = with_admin(
        admin,
        Router::new()
            .route(
                "/",
                get(raffles::get_all_raffles).post(raffles::create_raffle),
            )
            .route("/{raffle_id}/winners", get(raffles::get_raffle_winners))
            .route(
                "/winners/{winner_id}",
                patch(raffles::update_winner_gift_status),
            ),
    );

    let campaign_slug_routes = Router::new()
        .nest("/questions", api_questions)
        .nest("/prizes", api_prizes)
        .nest("/registrations", api_registrations)
        .nest("/submissions", api_submissions)
        .nest("/spin", api_spin)
        .nest("/settings", api_settings)
        .nest("/locations", api_locations)
        .nest("/inventory", api_inventory)
        .nest("/raffles", api_raffles);

    let api_campaign_admin = with_admin(
        state.clone(),
        Router::new()
            .route("/api/campaigns", get(list_campaigns).post(create_campaign))
            .route(
                "/api/campaigns/{slug}",
                get(get_campaign)
                    .put(update_campaign)
                    .delete(archive_campaign),
            ),
    );

    let api_auth = Router::new()
        .route("/verify", post(auth::verify_token))
        .route("/session", post(auth::create_session));

    let csrf = |router: Router<Arc<AppState>>| {
        router.layer(middleware::from_fn_with_state(
            state.clone(),
            require_csrf_middleware,
        ))
    };

    Router::new()
        .route("/health", get(health))
        .route("/api/csrf-token", get(csrf_token))
        .merge(csrf(api_campaign_admin))
        .nest(
            "/api/campaigns/{slug}",
            csrf(campaign_slug_routes).layer(middleware::from_fn_with_state(
                state.clone(),
                inject_campaign_context,
            )),
        )
        .nest("/api/auth", csrf(api_auth))
        .fallback(|| async { json_error(StatusCode::NOT_FOUND, "Route not found.") })
        .layer(middleware::from_fn_with_state(
            state.clone(),
            global_rate_limit_middleware,
        ))
        .layer(middleware::from_fn(request_context_middleware))
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

async fn csrf_token(State(state): State<Arc<AppState>>) -> axum::response::Response {
    match mint_csrf_token(&state.config) {
        Ok(token) => {
            SuccessResponse::data(serde_json::json!({ "csrfToken": token })).into_response()
        }
        Err(err) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
    }
}
