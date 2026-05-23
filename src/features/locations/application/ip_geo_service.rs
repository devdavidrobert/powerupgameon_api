use crate::features::locations::domain::{
    GeoPoint, GeoValidationResult, IpGeoCrossCheck, IpGeoLookup, Location,
};
use crate::features::locations::application::GeoService;

const METERS_PER_KM: f64 = 1000.0;

pub struct IpGeoService;

impl IpGeoService {
    pub fn is_public_ip(ip: &str) -> bool {
        let ip = ip.trim();
        if ip.is_empty() || ip == "unknown" {
            return false;
        }
        if ip == "::1" || ip.starts_with("fe80:") || ip.starts_with("fc") || ip.starts_with("fd") {
            return false;
        }
        if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
            return match addr {
                std::net::IpAddr::V4(v4) => {
                    !v4.is_loopback()
                        && !v4.is_private()
                        && !v4.is_link_local()
                        && !v4.is_unspecified()
                }
                std::net::IpAddr::V6(v6) => {
                    !v6.is_loopback() && !v6.is_unspecified()
                }
            };
        }
        false
    }

    pub fn cross_check_gps_and_ip(
        gps_point: &GeoPoint,
        gps_result: &GeoValidationResult,
        ip_lookup: IpGeoLookup,
        locations: &[Location],
        max_distance_km: f64,
    ) -> IpGeoCrossCheck {
        let GeoValidationResult::Matched { .. } = gps_result else {
            return IpGeoCrossCheck::Skipped;
        };

        let IpGeoLookup::Found(ip_coords) = ip_lookup else {
            return IpGeoCrossCheck::Skipped;
        };

        if locations.iter().any(|l| l.enabled) == false {
            return IpGeoCrossCheck::Skipped;
        }

        let ip_point = GeoPoint {
            lat: ip_coords.lat,
            lng: ip_coords.lng,
        };

        match GeoService::resolve_location(&ip_point, locations) {
            GeoValidationResult::OutsideZones => IpGeoCrossCheck::Mismatch,
            GeoValidationResult::NoZonesConfigured => IpGeoCrossCheck::Skipped,
            GeoValidationResult::Matched { .. } => {
                let distance_m = GeoService::haversine_meters(gps_point, &ip_point);
                let max_m = max_distance_km * METERS_PER_KM;
                if distance_m > max_m {
                    IpGeoCrossCheck::Mismatch
                } else {
                    IpGeoCrossCheck::Pass
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::locations::domain::{GeoValidationResult, IpGeoPoint};

    fn nairobi_cbd() -> Location {
        Location {
            id: "nairobi".into(),
            name: "Nairobi CBD".into(),
            center_lat: -1.286389,
            center_lng: 36.817223,
            radius_meters: 5000.0,
            enabled: true,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn rejects_private_ips() {
        assert!(!IpGeoService::is_public_ip("127.0.0.1"));
        assert!(!IpGeoService::is_public_ip("192.168.1.1"));
        assert!(!IpGeoService::is_public_ip("unknown"));
    }

    #[test]
    fn accepts_public_ipv4() {
        assert!(IpGeoService::is_public_ip("8.8.8.8"));
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
        let ip_lookup = IpGeoLookup::Found(IpGeoPoint {
            lat: 0.0,
            lng: 0.0,
        });
        assert_eq!(
            IpGeoService::cross_check_gps_and_ip(
                &gps,
                &gps_result,
                ip_lookup,
                &[nairobi_cbd()],
                150.0,
            ),
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
        let ip_lookup = IpGeoLookup::Found(IpGeoPoint {
            lat: -1.2865,
            lng: 36.8173,
        });
        assert_eq!(
            IpGeoService::cross_check_gps_and_ip(
                &gps,
                &gps_result,
                ip_lookup,
                &[nairobi_cbd()],
                150.0,
            ),
            IpGeoCrossCheck::Pass
        );
    }

    #[test]
    fn unavailable_ip_lookup_skips_check() {
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
}
