use crate::config::Config;
use anyhow::{bail, Context, Result};
use firestore::{
    FirestoreDb, FirestoreDbOptions, FirestoreSimpleBatchWriteOptions, FirestoreSimpleBatchWriter,
};
use gcloud_sdk::{TokenSourceType, GCP_DEFAULT_SCOPES};
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

        let service_account: serde_json::Value = serde_json::from_str(json)
            .context("FIREBASE_SERVICE_ACCOUNT_JSON must be valid JSON.")?;

        let project_id = config.project_id(service_account["project_id"].as_str())?;

        let client = FirestoreDb::with_options_token_source(
            FirestoreDbOptions::new(project_id.clone()),
            GCP_DEFAULT_SCOPES.clone(),
            TokenSourceType::Json(json.clone()),
        )
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
