use crate::config::Config;
use anyhow::{bail, Context, Result};
use firestore::{FirestoreDb, FirestoreSimpleBatchWriteOptions, FirestoreSimpleBatchWriter};
use std::sync::Arc;

#[derive(Clone)]
pub struct FirestoreService {
    pub client: FirestoreDb,
    pub project_id: String,
}

impl FirestoreService {
    pub async fn new(config: &Config) -> Result<Arc<Self>> {
        let json = config
            .firebase_service_account_json
            .as_ref()
            .context("FIREBASE_SERVICE_ACCOUNT_JSON is required")?;

        if json.trim().is_empty() {
            if config.is_production {
                bail!("FIREBASE_SERVICE_ACCOUNT_JSON is required in production (JSON string).");
            }
            bail!("Set FIREBASE_SERVICE_ACCOUNT_JSON to the full service account JSON as a single-line string for local development.");
        }

        let service_account: serde_json::Value =
            serde_json::from_str(json).context("FIREBASE_SERVICE_ACCOUNT_JSON must be valid JSON.")?;

        let project_id = config.project_id(service_account["project_id"].as_str())?;

        let temp_path = std::env::temp_dir().join(format!(
            "powerupgameon-firebase-{}.json",
            std::process::id()
        ));
        std::fs::write(&temp_path, json)?;
        std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", &temp_path);

        let client = FirestoreDb::new(&project_id)
            .await
            .context("Failed to initialize Firestore client")?;

        crate::logger::log(
            config,
            "info",
            "firebase_admin_initialized",
            serde_json::json!({}),
        );

        Ok(Arc::new(Self { client, project_id }))
    }

    pub async fn batch_writer(&self) -> Result<FirestoreSimpleBatchWriter> {
        FirestoreSimpleBatchWriter::new(
            self.client.clone(),
            FirestoreSimpleBatchWriteOptions::new(),
        )
        .await
        .context("Failed to create Firestore batch writer")
    }
}
