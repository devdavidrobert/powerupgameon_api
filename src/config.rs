use anyhow::{bail, Context, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub node_env: String,
    pub is_production: bool,
    pub firebase_project_id: Option<String>,
    pub firebase_storage_bucket: Option<String>,
    pub firebase_service_account_json: Option<String>,
    pub allowed_origins: Vec<String>,
    pub trust_proxy: bool,
    pub rate_limit_enabled: bool,
    pub global_rate_limit_max: u32,
    pub global_rate_limit_window_secs: u64,
    pub registration_rate_limit_max: u32,
    pub submission_rate_limit_max: u32,
    pub spin_rate_limit_max: u32,
    pub rate_limit_key_prefix: Option<String>,
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
    /// Public web app origin used to serve uploaded assets through Vercel CDN (e.g. `https://powerupgameon.vercel.app`).
    pub public_web_origin: Option<String>,
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

        let rate_limit_enabled = !matches!(
            env::var("RATE_LIMIT_ENABLED").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        );

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

        let global_rate_limit_max = env::var("GLOBAL_RATE_LIMIT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(200);

        let global_rate_limit_window_secs = env::var("GLOBAL_RATE_LIMIT_WINDOW_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(15 * 60);

        let registration_rate_limit_max = env::var("REGISTRATION_RATE_LIMIT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let submission_rate_limit_max = env::var("SUBMISSION_RATE_LIMIT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let spin_rate_limit_max = env::var("SPIN_RATE_LIMIT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let rate_limit_key_prefix = env::var("RATE_LIMIT_KEY_PREFIX")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let public_web_origin = env::var("PUBLIC_WEB_ORIGIN")
            .ok()
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .filter(|s| !s.is_empty());

        Ok(Self {
            port,
            node_env,
            is_production,
            firebase_project_id: env::var("FIREBASE_PROJECT_ID").ok(),
            firebase_storage_bucket: env::var("FIREBASE_STORAGE_BUCKET")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            firebase_service_account_json: env::var("FIREBASE_SERVICE_ACCOUNT_JSON").ok(),
            allowed_origins,
            trust_proxy,
            rate_limit_enabled,
            global_rate_limit_max,
            global_rate_limit_window_secs,
            registration_rate_limit_max,
            submission_rate_limit_max,
            spin_rate_limit_max,
            rate_limit_key_prefix,
            rate_limit_window_ms: global_rate_limit_window_secs * 1000,
            rate_limit_max: global_rate_limit_max,
            api_csrf_secret,
            spin_token_secret,
            spin_token_ttl_minutes,
            redis_url,
            allowed_admin_emails,
            ip_geo_enabled,
            ip_geo_max_distance_km,
            ip_geo_api_url,
            cors_vercel_project,
            public_web_origin,
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

    pub fn storage_bucket(&self, project_id: &str) -> String {
        self.firebase_storage_bucket
            .clone()
            .unwrap_or_else(|| format!("{project_id}.firebasestorage.app"))
    }
}
