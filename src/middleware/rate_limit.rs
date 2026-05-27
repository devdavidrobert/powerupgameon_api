use crate::app_state::AppState;
use crate::config::Config;
use crate::error::json_error;
use crate::features::campaigns::domain::DEFAULT_IP_RATE_LIMIT_WINDOW_SECS;
use crate::features::campaigns::presentation::CampaignContext;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use std::sync::Arc;
use std::time::{Duration, Instant};

static MEMORY_STORE: Lazy<DashMap<String, (u32, Instant)>> = Lazy::new(DashMap::new);

#[derive(Clone)]
pub struct RateLimitRule {
    pub prefix: &'static str,
    pub window: Duration,
    pub max: u32,
}

fn full_rate_limit_key(config: &Config, rule: &RateLimitRule, key: &str) -> String {
    match &config.rate_limit_key_prefix {
        Some(prefix) => format!("{prefix}:{}:{key}", rule.prefix),
        None => format!("{}:{key}", rule.prefix),
    }
}

pub fn is_global_rate_limit_exempt(path: &str) -> bool {
    path == "/health" || path == "/api/csrf-token"
}

pub fn global_rule(state: &AppState) -> RateLimitRule {
    RateLimitRule {
        prefix: "rl_global",
        window: Duration::from_secs(state.config.global_rate_limit_window_secs),
        max: state.config.global_rate_limit_max,
    }
}

fn campaign_ip_window_secs(req: &Request<Body>) -> u64 {
    req.extensions()
        .get::<CampaignContext>()
        .map(|ctx| ctx.campaign.ip_rate_limit_window_secs())
        .unwrap_or(DEFAULT_IP_RATE_LIMIT_WINDOW_SECS)
}

fn campaign_ip_max(config: &Config, prefix: &str) -> u32 {
    match prefix {
        "rl_reg" => config.registration_rate_limit_max,
        "rl_sub" => config.submission_rate_limit_max,
        "rl_spin" => config.spin_rate_limit_max,
        _ => 1,
    }
}

fn campaign_ip_rule(config: &Config, prefix: &'static str, window_secs: u64) -> RateLimitRule {
    RateLimitRule {
        prefix,
        window: Duration::from_secs(window_secs),
        max: campaign_ip_max(config, prefix),
    }
}

pub fn campaign_ip_rate_limit_key(campaign_id: &str, ip: &str) -> String {
    format!("{campaign_id}:{ip}")
}

pub async fn check_rate_limit_config(
    config: &Config,
    redis: &Option<redis::aio::ConnectionManager>,
    key: &str,
    rule: &RateLimitRule,
) -> Result<(), Response> {
    if !config.rate_limit_enabled {
        return Ok(());
    }

    let full_key = full_rate_limit_key(config, rule, key);

    if let Some(redis) = redis {
        let mut conn = redis.clone();
        let count: u32 = conn.incr(&full_key, 1).await.map_err(|_| {
            json_error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many requests. Please try again later.",
            )
        })?;
        if count == 1 {
            let _: () = conn
                .expire(&full_key, rule.window.as_secs() as i64)
                .await
                .map_err(|_| {
                    json_error(
                        StatusCode::TOO_MANY_REQUESTS,
                        "Too many requests. Please try again later.",
                    )
                })?;
        }
        if count > rule.max {
            return Err(json_error(
                StatusCode::TOO_MANY_REQUESTS,
                rate_limit_message(rule.prefix),
            ));
        }
        return Ok(());
    }

    let now = Instant::now();
    let mut entry = MEMORY_STORE.entry(full_key).or_insert((0, now));
    if now.duration_since(entry.1) > rule.window {
        *entry = (0, now);
    }
    entry.0 += 1;
    if entry.0 > rule.max {
        return Err(json_error(
            StatusCode::TOO_MANY_REQUESTS,
            rate_limit_message(rule.prefix),
        ));
    }
    Ok(())
}

pub async fn check_rate_limit(
    state: &AppState,
    key: &str,
    rule: &RateLimitRule,
) -> Result<(), Response> {
    check_rate_limit_config(&state.config, &state.redis, key, rule).await
}

fn rate_limit_message(prefix: &str) -> &'static str {
    match prefix {
        "rl_reg" => "This IP has already registered for this campaign. Please try again later.",
        "rl_sub" => "This IP has already submitted for this campaign. Please try again later.",
        "rl_spin" => "This IP has already spun for this campaign. Please try again later.",
        _ => "Too many requests. Please try again later.",
    }
}

pub async fn global_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path();
    if is_global_rate_limit_exempt(path) {
        return next.run(req).await;
    }

    let ip = crate::utils::client_ip::get_client_ip_from_request(
        req.headers(),
        req.extensions(),
        state.config.trust_proxy,
    );
    if let Err(resp) = check_rate_limit(&state, &ip, &global_rule(&state)).await {
        return resp;
    }
    next.run(req).await
}

pub async fn registration_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip_from_request(
        req.headers(),
        req.extensions(),
        state.config.trust_proxy,
    );
    let window_secs = campaign_ip_window_secs(&req);
    let campaign_id = req
        .extensions()
        .get::<CampaignContext>()
        .map(|ctx| ctx.campaign_id().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let key = campaign_ip_rate_limit_key(&campaign_id, &ip);
    let rule = campaign_ip_rule(&state.config, "rl_reg", window_secs);
    if let Err(resp) = check_rate_limit(&state, &key, &rule).await {
        return resp;
    }
    next.run(req).await
}

pub async fn submission_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip_from_request(
        req.headers(),
        req.extensions(),
        state.config.trust_proxy,
    );
    let window_secs = campaign_ip_window_secs(&req);
    let campaign_id = req
        .extensions()
        .get::<CampaignContext>()
        .map(|ctx| ctx.campaign_id().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let key = campaign_ip_rate_limit_key(&campaign_id, &ip);
    let rule = campaign_ip_rule(&state.config, "rl_sub", window_secs);
    if let Err(resp) = check_rate_limit(&state, &key, &rule).await {
        return resp;
    }
    next.run(req).await
}

pub async fn spin_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip_from_request(
        req.headers(),
        req.extensions(),
        state.config.trust_proxy,
    );
    let window_secs = campaign_ip_window_secs(&req);
    let campaign_id = req
        .extensions()
        .get::<CampaignContext>()
        .map(|ctx| ctx.campaign_id().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let key = campaign_ip_rate_limit_key(&campaign_id, &ip);
    let rule = campaign_ip_rule(&state.config, "rl_spin", window_secs);
    if let Err(resp) = check_rate_limit(&state, &key, &rule).await {
        return resp;
    }
    next.run(req).await
}
