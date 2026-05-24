use serde::{Deserialize, Serialize};

/// Subcollection name for uniqueness locks under each campaign.
pub const UNIQUENESS_SUBCOL: &str = "uniqueness";

/// Lightweight device fingerprint payload sent from the client.
/// The stable `deviceId` (UUID stored in localStorage) is the primary identifier.
/// Additional signals are collected for correlation and admin forensics but are
/// intentionally coarse (no canvas, audio, WebGL, or fonts) to respect privacy
/// and performance lessons from the prior deployment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceFingerprint {
    /// Stable device identifier (UUIDv4 persisted in localStorage on the client).
    #[serde(rename = "deviceId")]
    pub device_id: String,

    /// Optional coarse signals for correlation (e.g. when deviceId is cleared).
    #[serde(rename = "platform", skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(rename = "hardwareConcurrency", skip_serializing_if = "Option::is_none")]
    pub hardware_concurrency: Option<u32>,

    #[serde(rename = "deviceMemory", skip_serializing_if = "Option::is_none")]
    pub device_memory: Option<u32>,

    #[serde(rename = "screenWidth", skip_serializing_if = "Option::is_none")]
    pub screen_width: Option<u32>,

    #[serde(rename = "screenHeight", skip_serializing_if = "Option::is_none")]
    pub screen_height: Option<u32>,

    #[serde(rename = "colorDepth", skip_serializing_if = "Option::is_none")]
    pub color_depth: Option<u32>,

    #[serde(rename = "timezone", skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    /// Short hash of the variable signals (for IP+signal correlation rules).
    #[serde(rename = "fingerprintHash", skip_serializing_if = "Option::is_none")]
    pub fingerprint_hash: Option<String>,
}

/// Result of a uniqueness check. When `allowed` is false, `reason` contains the
/// error code for the client (e.g. "DEVICE_ALREADY_USED").
#[derive(Debug, Clone)]
pub struct UniquenessCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub existing_session_id: Option<String>,
}

/// Kinds of locks we create in the uniqueness subcollection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniquenessLockKind {
    /// Primary device lock (keyed by deviceId).
    Device,
    /// Optional secondary correlation record (IP + fingerprint hash).
    IpCorrelation,
}

impl UniquenessLockKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Device => "device_lock",
            Self::IpCorrelation => "ip_correlation",
        }
    }
}

/// Document stored for a device lock under the campaign's uniqueness subcollection.
/// Document ID is typically "device_{deviceId}".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLockDoc {
    pub kind: String,
    #[serde(rename = "deviceId")]
    pub device_id: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "hasPlayed")]
    pub has_played: bool,
    #[serde(rename = "playedAt", skip_serializing_if = "Option::is_none")]
    pub played_at: Option<i64>,
    #[serde(rename = "registeredAt")]
    pub registered_at: i64,
    pub ip: String,
    #[serde(rename = "userAgent")]
    pub user_agent: String,
    /// The full fingerprint payload at time of lock creation (for audit).
    #[serde(rename = "deviceFingerprint", skip_serializing_if = "Option::is_none")]
    pub device_fingerprint: Option<DeviceFingerprint>,
}
