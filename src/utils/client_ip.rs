use axum::http::HeaderMap;

pub fn get_client_ip(headers: &HeaderMap, trust_proxy: bool, fallback: &str) -> String {
    if trust_proxy {
        if let Some(forwarded) = headers.get("x-forwarded-for") {
            if let Ok(value) = forwarded.to_str() {
                if let Some(first) = value.split(',').next() {
                    let trimmed = first.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
            }
        }
    }
    if fallback.is_empty() {
        "unknown".into()
    } else {
        fallback.to_string()
    }
}
