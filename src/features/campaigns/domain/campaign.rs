use serde::{Deserialize, Serialize};

use super::player_outcome_copy::PlayerOutcomeCopy;
use super::ip_rate_limit::resolve_ip_rate_limit_window_secs;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrandLogo {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(rename = "sortOrder")]
    pub sort_order: i64,
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
    /// Minimum % of gradable questions answered correctly to earn a wheel spin (0–100).
    #[serde(rename = "spinPassPercent")]
    pub spin_pass_percent: i64,
    #[serde(rename = "brandLogos", skip_serializing_if = "Option::is_none")]
    pub brand_logos: Option<Vec<BrandLogo>>,
    #[serde(rename = "playerOutcomeCopy", skip_serializing_if = "Option::is_none")]
    pub player_outcome_copy: Option<PlayerOutcomeCopy>,
    #[serde(rename = "registrationFormHeader", skip_serializing_if = "Option::is_none")]
    pub registration_form_header: Option<String>,
    /// Cooldown window (seconds) before the same IP can register, submit, or spin again.
    #[serde(rename = "ipRateLimitWindowSecs", skip_serializing_if = "Option::is_none")]
    pub ip_rate_limit_window_secs: Option<i64>,
    #[serde(rename = "createdAt", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl Campaign {
    pub fn is_publicly_accessible(&self) -> bool {
        self.status == CampaignStatus::Active
    }

    pub fn spin_pass_percent(&self) -> i64 {
        self.spin_pass_percent.clamp(0, 100)
    }

    pub fn sorted_brand_logos(&self) -> Vec<BrandLogo> {
        let mut logos = self.brand_logos.clone().unwrap_or_default();
        logos.sort_by_key(|logo| logo.sort_order);
        logos
    }

    pub fn player_outcome_copy_or_default(&self) -> PlayerOutcomeCopy {
        self.player_outcome_copy.clone().unwrap_or_default()
    }

    pub fn registration_form_header_or_default(&self) -> String {
        self.registration_form_header
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_REGISTRATION_FORM_HEADER.to_string())
    }

    pub fn ip_rate_limit_window_secs(&self) -> u64 {
        resolve_ip_rate_limit_window_secs(self.ip_rate_limit_window_secs)
    }
}

pub const MAX_BRAND_LOGOS: usize = 2;
pub const DEFAULT_REGISTRATION_FORM_HEADER: &str = "Rider Details";
pub const MAX_REGISTRATION_FORM_HEADER_LEN: usize = 120;
