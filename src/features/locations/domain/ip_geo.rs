#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IpGeoPoint {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpGeoLookup {
    Found(IpGeoPoint),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpGeoCrossCheck {
    Pass,
    Mismatch,
    Skipped,
}
