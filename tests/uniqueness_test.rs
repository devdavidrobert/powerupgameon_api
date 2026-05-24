//! Feature-level tests for the Uniqueness / "one entry per person" hardening.
//!
//! RUNNING TESTS (MANDATORY RULE):
//!   Always run at the *feature level*:
//!     cargo test --test uniqueness_test
//!   Never use the full-suite `cargo test` for changes that only touch uniqueness logic.
//!   After any edit to src/features/uniqueness/** or the registration tx check,
//!   re-execute the above command before considering the change complete.
//!
//! This file follows the style of challenge_window_middleware_test.rs (axum/tower
//! where useful) but focuses on the uniqueness domain + repository helpers + the
//! hard-blocking logic that lives inside RegistrationModel::register_tx.
//!
//! Full end-to-end duplicate registration tests require a live (or emulated)
//! Firestore instance with a campaign parent. Those tests are marked #[ignore]
//! so they don't break CI when no DB is present. When a test Firestore is
//! available they can be run explicitly with --ignored.

use powerupgameon_api::features::uniqueness::application::UniquenessService;
use powerupgameon_api::features::uniqueness::domain::{UniquenessCheckResult, UniquenessLockKind};

#[test]
fn device_lock_id_format_is_stable_and_namespaced() {
    assert_eq!(
        UniquenessService::device_lock_id("device-uuid-123"),
        "device_device-uuid-123"
    );
}

#[test]
fn ip_correlation_key_handles_missing_hash() {
    let k = UniquenessService::ip_correlation_id("10.0.0.1", None);
    assert!(k.starts_with("ip_10_0_0_1"));
}

#[test]
fn error_code_prefers_explicit_reason() {
    let res = UniquenessCheckResult {
        allowed: false,
        reason: Some("IP_DEVICE_CONFLICT".into()),
        existing_session_id: None,
    };
    assert_eq!(
        UniquenessService::error_code_for(&res),
        Some("IP_DEVICE_CONFLICT")
    );
}

#[test]
fn error_code_defaults_to_device_already_used() {
    let res = UniquenessCheckResult {
        allowed: false,
        reason: None,
        existing_session_id: None,
    };
    assert_eq!(
        UniquenessService::error_code_for(&res),
        Some("DEVICE_ALREADY_USED")
    );
}

#[test]
fn lock_kind_strings_are_stable() {
    assert_eq!(UniquenessLockKind::Device.as_str(), "device_lock");
    assert_eq!(UniquenessLockKind::IpCorrelation.as_str(), "ip_correlation");
}

#[test]
fn minimal_fingerprint_roundtrips_device_id() {
    let fp = UniquenessService::minimal_fingerprint("dev-xyz".into());
    assert_eq!(fp.device_id, "dev-xyz");
    assert!(fp.fingerprint_hash.is_none());
}

// ---------------------------------------------------------------------------
// Integration-style scenarios (require Firestore for full tx coverage)
// ---------------------------------------------------------------------------

/// This test documents the expected hard-block behaviour for the registration
/// transaction when the same stable deviceId is reused.
///
/// In a real environment with a configured test Firestore:
/// 1. Create a campaign + registration via the public POST /registrations
///    supplying a deviceId.
/// 2. Attempt a second registration with the *same* deviceId (different name
///    or new sessionId) — it must be rejected with 409 + code "DEVICE_ALREADY_USED".
///
/// Because we have no DB in this CI context the test is ignored.
#[tokio::test]
#[ignore]
async fn device_registration_is_hard_blocked_on_duplicate_device_id() {
    // Placeholder: wire a real AppState + router and exercise the flow twice.
    // See how other integration tests (if any) bootstrap Firestore via env.
    panic!("Implement with live Firestore when available");
}

/// Documents that admin delete_registration also releases the device lock
/// (so the original person can re-register after a correction).
#[tokio::test]
#[ignore]
async fn admin_delete_releases_device_lock_allowing_replay() {
    panic!("Implement with live Firestore when available");
}

/// Documents a concurrent registration race: two requests with the same
/// deviceId arriving at nearly the same time must result in exactly one
/// success thanks to the atomic transaction + uniqueness check inside it.
#[tokio::test]
#[ignore]
async fn concurrent_registrations_same_device_are_serialized_by_tx() {
    panic!("Implement with live Firestore + concurrent requests when available");
}

/// Optional IP + fingerprint correlation hard block (policy driven).
/// The current implementation primarily blocks on deviceId; this test
/// records the desired future behaviour for the secondary signal path.
#[test]
fn ip_device_correlation_concept_is_represented_in_error_codes() {
    // The code path for "IP_DEVICE_CONFLICT" is already wired in the
    // registration error mapping and client handling; actual policy
    // decision lives in the tx check (future enhancement).
    let conflict = UniquenessCheckResult {
        allowed: false,
        reason: Some("IP_DEVICE_CONFLICT".into()),
        existing_session_id: None,
    };
    assert_eq!(
        UniquenessService::error_code_for(&conflict),
        Some("IP_DEVICE_CONFLICT")
    );
}
