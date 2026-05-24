use crate::error::{ApiError, ApiResult};
use firestore::{FirestoreDb, FirestoreResult, ParentPathBuilder};

pub const CAMPAIGNS_COLLECTION: &str = "campaigns";

#[derive(Clone, Debug)]
pub struct CampaignPaths {
    pub campaign_id: String,
}

impl CampaignPaths {
    pub fn new(campaign_id: impl Into<String>) -> Self {
        Self {
            campaign_id: campaign_id.into(),
        }
    }

    pub fn parent(&self, db: &FirestoreDb) -> FirestoreResult<ParentPathBuilder> {
        db.parent_path(CAMPAIGNS_COLLECTION, &self.campaign_id)
    }

    pub fn parent_str(&self, db: &FirestoreDb) -> ApiResult<String> {
        Ok(self
            .parent(db)
            .map_err(|e| ApiError::Internal(e.into()))?
            .to_string())
    }
}
