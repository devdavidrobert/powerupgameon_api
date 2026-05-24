use crate::features::uniqueness::domain::{DeviceFingerprint, UniquenessCheckResult};

pub struct UniquenessService;

impl UniquenessService {
    /// Generates the document ID for a primary device lock.
    /// Uses a simple prefix so it is clearly namespaced and does not collide
    /// with other lock kinds or player docs in the subcollection.
    pub fn device_lock_id(device_id: &str) -> String {
        format!("device_{}", device_id)
    }

    /// Generates a correlation key for IP + fingerprint hash secondary checks.
    /// We keep this simple (not a full hash of everything) to allow the
    /// repository to decide whether a correlation rule should trigger a hard
    /// block or just a record.
    pub fn ip_correlation_id(ip: &str, fp_hash: Option<&str>) -> String {
        match fp_hash {
            Some(h) if !h.is_empty() => format!("ip_{}_fp_{}", ip.replace('.', "_"), h),
            _ => format!("ip_{}", ip.replace('.', "_")),
        }
    }

    /// Pure decision helper: given a check result, return the error code if blocked.
    pub fn error_code_for(result: &UniquenessCheckResult) -> Option<&'static str> {
        if result.allowed {
            return None;
        }
        match result.reason.as_deref() {
            Some("IP_DEVICE_CONFLICT") => Some("IP_DEVICE_CONFLICT"),
            _ => Some("DEVICE_ALREADY_USED"),
        }
    }

    /// Optional helper to build a minimal fingerprint for storage when client
    /// only sent the deviceId (no extra signals).
    pub fn minimal_fingerprint(device_id: String) -> DeviceFingerprint {
        DeviceFingerprint {
            device_id,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_lock_id_is_prefixed() {
        assert_eq!(
            UniquenessService::device_lock_id("abc-123"),
            "device_abc-123"
        );
    }

    #[test]
    fn ip_correlation_id_includes_hash_when_present() {
        let id = UniquenessService::ip_correlation_id("1.2.3.4", Some("deadbeef"));
        assert!(id.contains("ip_1_2_3_4"));
        assert!(id.contains("fp_deadbeef"));
    }

    #[test]
    fn error_code_returns_reason_or_default() {
        let blocked = UniquenessCheckResult {
            allowed: false,
            reason: Some("IP_DEVICE_CONFLICT".into()),
            existing_session_id: None,
        };
        assert_eq!(
            UniquenessService::error_code_for(&blocked),
            Some("IP_DEVICE_CONFLICT")
        );

        let default_block = UniquenessCheckResult {
            allowed: false,
            reason: None,
            existing_session_id: None,
        };
        assert_eq!(
            UniquenessService::error_code_for(&default_block),
            Some("DEVICE_ALREADY_USED")
        );
    }
}
