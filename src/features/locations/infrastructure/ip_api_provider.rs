use crate::features::locations::domain::{IpGeoLookup, IpGeoPoint};
use serde::Deserialize;

const DEFAULT_TIMEOUT_SECS: u64 = 3;

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    status: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

pub struct IpApiProvider;

impl IpApiProvider {
    pub async fn lookup(ip: &str, api_url_template: Option<&str>) -> IpGeoLookup {
        let url = match api_url_template.filter(|s| !s.is_empty()) {
            Some(template) => template.replace("{ip}", ip),
            None => format!("http://ip-api.com/json/{ip}?fields=status,lat,lon"),
        };

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(_) => return IpGeoLookup::Unavailable,
        };

        let response = match client.get(&url).send().await {
            Ok(r) => r,
            Err(_) => return IpGeoLookup::Unavailable,
        };

        let body: IpApiResponse = match response.json().await {
            Ok(b) => b,
            Err(_) => return IpGeoLookup::Unavailable,
        };

        if body.status != "success" {
            return IpGeoLookup::Unavailable;
        }

        match (body.lat, body.lon) {
            (Some(lat), Some(lng)) => IpGeoLookup::Found(IpGeoPoint { lat, lng }),
            _ => IpGeoLookup::Unavailable,
        }
    }
}
