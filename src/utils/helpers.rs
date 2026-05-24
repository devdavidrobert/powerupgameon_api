use crate::error::{ApiError, ApiResult};
use rand::Rng;
use serde_json::{Map, Value};

pub fn submission_identity_from_registration(reg: &Map<String, Value>) -> ApiResult<(String, String)> {
    let full_name = reg
        .get("fullName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("Registration is missing a valid fullName.")
        })?;
    let normalized_name = reg
        .get("normalizedName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("Registration is missing a valid normalizedName.")
        })?;
    Ok((full_name.to_string(), normalized_name.to_string()))
}

pub fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn fisher_yates_shuffle<T>(mut array: Vec<T>) -> Vec<T> {
    let mut rng = rand::thread_rng();
    for i in (1..array.len()).rev() {
        let j = rng.gen_range(0..=i);
        array.swap(i, j);
    }
    array
}

pub fn to_iso_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.as_i64().is_some() => {
            chrono::DateTime::from_timestamp_millis(n.as_i64().unwrap())
                .map(|dt| dt.to_rfc3339())
        }
        _ => None,
    }
}

pub fn encode_cursor(cursor: &serde_json::Value) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(cursor).unwrap_or_default())
}

pub fn decode_cursor(encoded: &str) -> Option<serde_json::Value> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}
