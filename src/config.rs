use anyhow::{bail, Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub node_env: String,
    pub is_production: bool,
    pub firebase_project_id: Option<String>,
    pub firebase_service_account_json: Option<String>,
    pub allowed_origins: Vec<String>,
    pub trust_proxy: bool,
    pub rate_limit_window_ms: u64,
    pub rate_limit_max: u32,
    pub api_csrf_secret: String,
    pub spin_token_secret: String,
    pub spin_token_ttl_minutes: u32,
    pub redis_url: Option<String>,
    pub allowed_admin_emails: Vec<String>,
    pub ip_geo_enabled: bool,
    pub ip_geo_max_distance_km: f64,
    pub ip_geo_api_url: Option<String>,
    /// When set (e.g. `powerupgameon`), also allow `https://{project}.vercel.app` and preview URLs.
    pub cors_vercel_project: Option<String>,
}

impl Config {
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        if self.allowed_origins.iter().any(|allowed| allowed == origin) {
            return true;
        }
        let Some(project) = self.cors_vercel_project.as_deref() else {
            return false;
        };
        if origin == format!("https://{project}.vercel.app") {
            return true;
        }
        origin.starts_with(&format!("https://{project}-")) && origin.ends_with(".vercel.app")
    }

    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let node_env = env::var("NODE_ENV").unwrap_or_else(|_| "development".into());
        let is_production = node_env == "production";

        let raw_origins = env::var("ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect::<Vec<_>>();

        let mut allowed_origins = if raw_origins.is_empty() {
            vec![
                "http://localhost:3000".into(),
                "http://127.0.0.1:3000".into(),
            ]
        } else {
            raw_origins
        };

        for key in ["FRONTEND_URL", "NEXT_PUBLIC_APP_URL"] {
            if let Ok(url) = env::var(key) {
                let trimmed = url.trim().trim_end_matches('/');
                if !trimmed.is_empty() && !allowed_origins.iter().any(|o| o == trimmed) {
                    allowed_origins.push(trimmed.to_string());
                }
            }
        }

        let cors_vercel_project = env::var("CORS_VERCEL_PROJECT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let allowed_admin_emails = env::var("ALLOWED_ADMIN_EMAILS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();

        let trust_proxy = matches!(env::var("TRUST_PROXY").as_deref(), Ok("1") | Ok("true"));

        let api_csrf_secret = env::var("API_CSRF_SECRET").unwrap_or_else(|_| {
            if is_production {
                String::new()
            } else {
                "dev-csrf-secret-change-me".into()
            }
        });

        let spin_token_secret = env::var("SPIN_TOKEN_SECRET").unwrap_or_else(|_| {
            if is_production {
                String::new()
            } else {
                "dev-spin-token-secret-change-me".into()
            }
        });

        if is_production && api_csrf_secret.is_empty() {
            bail!("API_CSRF_SECRET must be set in production.");
        }
        if is_production && spin_token_secret.is_empty() {
            bail!("SPIN_TOKEN_SECRET must be set in production.");
        }

        let spin_token_ttl_minutes = env::var("SPIN_TOKEN_TTL_MINUTES")
            .unwrap_or_else(|_| "60".into())
            .parse()
            .unwrap_or(60);

        let redis_url = env::var("REDIS_URL").ok().filter(|s| !s.trim().is_empty());

        if is_production && redis_url.is_none() {
            tracing::warn!(
                "[steam-api] REDIS_URL is unset: rate limit counters are per-process only. Set REDIS_URL for multi-instance deployments."
            );
        }

        if is_production && trust_proxy {
            tracing::info!(
                "[steam-api] TRUST_PROXY=1: client IP is taken from X-Forwarded-For. Deploy behind a proxy that strips client-supplied forwarding headers."
            );
        }

        let port = env::var("PORT")
            .unwrap_or_else(|_| "4000".into())
            .parse()
            .context("PORT must be a valid number")?;

        let ip_geo_enabled = matches!(
            env::var("IP_GEO_ENABLED").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE")
        );

        let ip_geo_max_distance_km = env::var("IP_GEO_MAX_DISTANCE_KM")
            .unwrap_or_else(|_| "150".into())
            .parse()
            .unwrap_or(150.0);

        let ip_geo_api_url = env::var("IP_GEO_API_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());

        Ok(Self {
            port,
            node_env,
            is_production,
            firebase_project_id: env::var("FIREBASE_PROJECT_ID").ok(),
            firebase_service_account_json: env::var("FIREBASE_SERVICE_ACCOUNT_JSON").ok(),
            allowed_origins,
            trust_proxy,
            rate_limit_window_ms: 15 * 60 * 1000,
            rate_limit_max: 200,
            api_csrf_secret,
            spin_token_secret,
            spin_token_ttl_minutes,
            redis_url,
            allowed_admin_emails,
            ip_geo_enabled,
            ip_geo_max_distance_km,
            ip_geo_api_url,
            cors_vercel_project,
        })
    }

    pub fn project_id(&self, service_account_project: Option<&str>) -> Result<String> {
        if let Some(id) = &self.firebase_project_id {
            return Ok(id.clone());
        }
        if let Some(id) = service_account_project {
            return Ok(id.to_string());
        }
        bail!("FIREBASE_PROJECT_ID is required when not present in service account JSON.");
    }
}
