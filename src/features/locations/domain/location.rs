use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: String,
    pub name: String,
    #[serde(rename = "centerLat")]
    pub center_lat: f64,
    #[serde(rename = "centerLng")]
    pub center_lng: f64,
    #[serde(rename = "radiusMeters")]
    pub radius_meters: f64,
    pub enabled: bool,
    #[serde(rename = "createdAt", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoStatus {
    Valid,
    Outside,
    NoZones,
}

impl GeoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Outside => "outside",
            Self::NoZones => "no_zones",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GeoPoint {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone)]
pub enum GeoValidationResult {
    Matched { location_id: String },
    OutsideZones,
    NoZonesConfigured,
}
