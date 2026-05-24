use powerupgameon_api::features::campaigns::domain::GeoEnforcement;
use powerupgameon_api::features::locations::application::{GeoService, IpGeoService};
use powerupgameon_api::features::locations::domain::{
    GeoPoint, GeoValidationResult, IpGeoCrossCheck, IpGeoLookup, Location,
};

fn nairobi_cbd() -> Location {
    Location {
        id: "nairobi".into(),
        name: "Nairobi CBD".into(),
        center_lat: -1.286389,
        center_lng: 36.817223,
        radius_meters: 500.0,
        enabled: true,
        created_at: None,
        updated_at: None,
    }
}

#[test]
fn haversine_zero_for_same_point() {
    let p = GeoPoint {
        lat: -1.286389,
        lng: 36.817223,
    };
    assert!(GeoService::haversine_meters(&p, &p) < 1.0);
}

#[test]
fn resolves_point_inside_zone() {
    let point = GeoPoint {
        lat: -1.2864,
        lng: 36.8172,
    };
    match GeoService::resolve_location(&point, &[nairobi_cbd()]) {
        GeoValidationResult::Matched { location_id } => assert_eq!(location_id, "nairobi"),
        other => panic!("expected match, got {other:?}"),
    }
}

#[test]
fn rejects_point_outside_zone() {
    let point = GeoPoint { lat: 0.0, lng: 0.0 };
    assert!(matches!(
        GeoService::resolve_location(&point, &[nairobi_cbd()]),
        GeoValidationResult::OutsideZones
    ));
}

#[test]
fn no_zones_when_none_enabled() {
    let mut zone = nairobi_cbd();
    zone.enabled = false;
    let point = GeoPoint {
        lat: -1.2864,
        lng: 36.8172,
    };
    assert!(matches!(
        GeoService::resolve_location(&point, &[zone]),
        GeoValidationResult::NoZonesConfigured
    ));
}

#[test]
fn geo_enforcement_modes_exist() {
    assert_eq!(GeoEnforcement::Reject.as_str(), "reject");
    assert_eq!(GeoEnforcement::Flag.as_str(), "flag");
}

#[test]
fn validate_coordinates_rejects_invalid_lat() {
    assert!(GeoService::validate_coordinates(91.0, 0.0).is_err());
}

#[test]
fn gps_inside_ip_outside_is_mismatch() {
    let gps = GeoPoint {
        lat: -1.2864,
        lng: 36.8172,
    };
    let gps_result = GeoValidationResult::Matched {
        location_id: "nairobi".into(),
    };
    let ip_lookup =
        IpGeoLookup::Found(powerupgameon_api::features::locations::domain::IpGeoPoint {
            lat: 0.0,
            lng: 0.0,
        });
    assert_eq!(
        IpGeoService::cross_check_gps_and_ip(&gps, &gps_result, ip_lookup, &[nairobi_cbd()], 150.0,),
        IpGeoCrossCheck::Mismatch
    );
}

#[test]
fn gps_and_ip_nearby_passes() {
    let gps = GeoPoint {
        lat: -1.2864,
        lng: 36.8172,
    };
    let gps_result = GeoValidationResult::Matched {
        location_id: "nairobi".into(),
    };
    let ip_lookup =
        IpGeoLookup::Found(powerupgameon_api::features::locations::domain::IpGeoPoint {
            lat: -1.2865,
            lng: 36.8173,
        });
    assert_eq!(
        IpGeoService::cross_check_gps_and_ip(&gps, &gps_result, ip_lookup, &[nairobi_cbd()], 150.0,),
        IpGeoCrossCheck::Pass
    );
}

#[test]
fn private_ip_skips_cross_check() {
    assert!(!IpGeoService::is_public_ip("127.0.0.1"));
    assert!(!IpGeoService::is_public_ip("192.168.0.1"));
}

#[test]
fn unavailable_ip_lookup_skips_cross_check() {
    let gps = GeoPoint {
        lat: -1.2864,
        lng: 36.8172,
    };
    let gps_result = GeoValidationResult::Matched {
        location_id: "nairobi".into(),
    };
    assert_eq!(
        IpGeoService::cross_check_gps_and_ip(
            &gps,
            &gps_result,
            IpGeoLookup::Unavailable,
            &[nairobi_cbd()],
            150.0,
        ),
        IpGeoCrossCheck::Skipped
    );
}
