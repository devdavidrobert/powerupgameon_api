use crate::config::Config;
use crate::services::firebase_auth::FirebaseAuth;
use crate::services::firestore::FirestoreService;
use redis::aio::ConnectionManager;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub db: Arc<FirestoreService>,
    pub firebase_auth: FirebaseAuth,
    pub redis: Option<ConnectionManager>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Arc<Self>> {
        let db = FirestoreService::new(&config).await?;
        let firebase_auth = FirebaseAuth::new(&config, &db).await?;

        let redis = if let Some(url) = &config.redis_url {
            match redis::Client::open(url.as_str()) {
                Ok(client) => match ConnectionManager::new(client).await {
                    Ok(conn) => Some(conn),
                    Err(err) => {
                        crate::logger::log(
                            &config,
                            "error",
                            "redis_connect_failed",
                            serde_json::json!({ "err": err.to_string() }),
                        );
                        None
                    }
                },
                Err(err) => {
                    crate::logger::log(
                        &config,
                        "warn",
                        "rate_limit_redis_unavailable",
                        serde_json::json!({ "err": err.to_string() }),
                    );
                    None
                }
            }
        } else {
            None
        };

        Ok(Arc::new(Self {
            config,
            db,
            firebase_auth,
            redis,
        }))
    }
}
