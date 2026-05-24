use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CampaignStatus {
    Draft,
    Active,
    Archived,
}

impl CampaignStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "archived" => Self::Archived,
            _ => Self::Draft,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StaggerMode {
    Linear,
    Stepped,
    Immediate,
}

impl StaggerMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "stepped" => Self::Stepped,
            "immediate" => Self::Immediate,
            _ => Self::Linear,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Stepped => "stepped",
            Self::Immediate => "immediate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GeoEnforcement {
    Reject,
    Flag,
}

impl GeoEnforcement {
    pub fn from_str(s: &str) -> Self {
        match s {
            "flag" => Self::Flag,
            _ => Self::Reject,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Flag => "flag",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaggerStep {
    #[serde(rename = "releaseAt")]
    pub release_at: i64,
    #[serde(rename = "releasePercent")]
    pub release_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub status: CampaignStatus,
    #[serde(rename = "challengeStartTime", skip_serializing_if = "Option::is_none")]
    pub challenge_start_time: Option<i64>,
    #[serde(rename = "challengeEndTime", skip_serializing_if = "Option::is_none")]
    pub challenge_end_time: Option<i64>,
    #[serde(rename = "staggerMode")]
    pub stagger_mode: StaggerMode,
    #[serde(rename = "staggerSchedule", skip_serializing_if = "Option::is_none")]
    pub stagger_schedule: Option<Vec<StaggerStep>>,
    #[serde(rename = "geoEnforcement")]
    pub geo_enforcement: GeoEnforcement,
    #[serde(rename = "createdAt", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl Campaign {
    pub fn is_publicly_accessible(&self) -> bool {
        self.status == CampaignStatus::Active
    }
}
