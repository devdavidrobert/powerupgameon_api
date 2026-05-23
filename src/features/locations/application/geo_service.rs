use crate::features::locations::domain::{GeoPoint, GeoValidationResult, Location};

const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

pub struct GeoService;

impl GeoService {
    pub fn validate_coordinates(lat: f64, lng: f64) -> Result<(), &'static str> {
        if !(-90.0..=90.0).contains(&lat) {
            return Err("lat must be between -90 and 90.");
        }
        if !(-180.0..=180.0).contains(&lng) {
            return Err("lng must be between -180 and 180.");
        }
        Ok(())
    }

    pub fn haversine_meters(a: &GeoPoint, b: &GeoPoint) -> f64 {
        let lat1 = a.lat.to_radians();
        let lat2 = b.lat.to_radians();
        let dlat = (b.lat - a.lat).to_radians();
        let dlng = (b.lng - a.lng).to_radians();

        let sin_dlat = (dlat / 2.0).sin();
        let sin_dlng = (dlng / 2.0).sin();
        let h = sin_dlat * sin_dlat + lat1.cos() * lat2.cos() * sin_dlng * sin_dlng;
        2.0 * EARTH_RADIUS_METERS * h.sqrt().asin()
    }

    pub fn resolve_location(point: &GeoPoint, locations: &[Location]) -> GeoValidationResult {
        let enabled: Vec<&Location> = locations.iter().filter(|l| l.enabled).collect();
        if enabled.is_empty() {
            return GeoValidationResult::NoZonesConfigured;
        }

        let mut matches: Vec<(&Location, f64)> = enabled
            .iter()
            .filter_map(|loc| {
                let center = GeoPoint {
                    lat: loc.center_lat,
                    lng: loc.center_lng,
                };
                let dist = Self::haversine_meters(point, &center);
                if dist <= loc.radius_meters {
                    Some((*loc, dist))
                } else {
                    None
                }
            })
            .collect();

        if matches.is_empty() {
            return GeoValidationResult::OutsideZones;
        }

        matches.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        GeoValidationResult::Matched {
            location_id: matches[0].0.id.clone(),
        }
    }
}
