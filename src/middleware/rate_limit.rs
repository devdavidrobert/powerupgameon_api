use crate::app_state::AppState;
use crate::error::json_error;
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

pub async fn check_rate_limit(
    state: &AppState,
    key: &str,
    rule: &RateLimitRule,
) -> Result<(), Response> {
    let full_key = format!("{}:{}", rule.prefix, key);

    if let Some(redis) = &state.redis {
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

fn rate_limit_message(prefix: &str) -> &'static str {
    match prefix {
        "rl_reg" => "Too many registration attempts from this IP. Please try again in an hour.",
        "rl_sub" => "Too many submissions. Please try again later.",
        "rl_spin" => "Too many spin attempts. Please try again later.",
        _ => "Too many requests. Please try again later.",
    }
}

pub const GLOBAL_RULE: RateLimitRule = RateLimitRule {
    prefix: "rl_global",
    window: Duration::from_secs(15 * 60),
    max: 200,
};

pub const REGISTRATION_RULE: RateLimitRule = RateLimitRule {
    prefix: "rl_reg",
    window: Duration::from_secs(60 * 60),
    max: 3,
};

pub const SUBMISSION_RULE: RateLimitRule = RateLimitRule {
    prefix: "rl_sub",
    window: Duration::from_secs(15 * 60),
    max: 30,
};

pub const SPIN_RULE: RateLimitRule = RateLimitRule {
    prefix: "rl_spin",
    window: Duration::from_secs(60 * 60),
    max: 8,
};

pub async fn global_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip(
        req.headers(),
        state.config.trust_proxy,
        "unknown",
    );
    if let Err(resp) = check_rate_limit(&state, &ip, &GLOBAL_RULE).await {
        return resp;
    }
    next.run(req).await
}

pub async fn registration_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip(
        req.headers(),
        state.config.trust_proxy,
        "unknown",
    );
    if let Err(resp) = check_rate_limit(&state, &ip, &REGISTRATION_RULE).await {
        return resp;
    }
    next.run(req).await
}

pub async fn submission_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip(
        req.headers(),
        state.config.trust_proxy,
        "unknown",
    );
    if let Err(resp) = check_rate_limit(&state, &ip, &SUBMISSION_RULE).await {
        return resp;
    }
    next.run(req).await
}

pub async fn spin_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let ip = crate::utils::client_ip::get_client_ip(
        req.headers(),
        state.config.trust_proxy,
        "unknown",
    );
    let sid = if let Some(body) = req.extensions().get::<String>() {
        body.clone()
    } else {
        "na".into()
    };
    let key = format!("{ip}:{sid}");
    if let Err(resp) = check_rate_limit(&state, &key, &SPIN_RULE).await {
        return resp;
    }
    next.run(req).await
}
