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
            let client = redis::Client::open(url.as_str())
                .map_err(|err| anyhow::anyhow!("Failed to open Redis client: {err}"))?;
            Some(ConnectionManager::new(client).await.map_err(|err| {
                anyhow::anyhow!("REDIS_URL is set but Redis is unreachable: {err}")
            })?)
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
