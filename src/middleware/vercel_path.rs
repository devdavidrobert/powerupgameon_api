use axum::{
    extract::Request,
    http::{header::HeaderMap, uri::PathAndQuery, Uri},
    middleware::Next,
    response::Response,
};

/// Vercel rewrites all traffic to `/api/main`, so Axum must restore the original path
/// before route matching. Without this, nested `/api/campaigns/{slug}/...` routes 400.
pub async fn restore_vercel_path(mut req: Request, next: Next) -> Response {
    if should_restore_path(req.uri().path()) {
        if let Some(restored) = resolve_original_path(req.uri().query(), req.headers()) {
            if let Ok(new_uri) = build_uri(&restored, req.uri().query()) {
                *req.uri_mut() = new_uri;
            }
        }
    }
    next.run(req).await
}

fn should_restore_path(path: &str) -> bool {
    path == "/api/main" || path.ends_with("/main")
}

pub fn resolve_original_path(query: Option<&str>, headers: &HeaderMap) -> Option<String> {
    if let Some(path) = path_from_query(query) {
        return Some(path);
    }

    for header in [
        "x-invoke-path",
        "x-original-url",
        "x-forwarded-uri",
        "x-vercel-sc-path",
        "x-matched-path",
    ] {
        if let Some(value) = headers.get(header).and_then(|v| v.to_str().ok()) {
            if let Some(path) = path_from_header_value(value) {
                return Some(path);
            }
        }
    }

    if let Some(value) = headers
        .get("x-now-route-matches")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(path) = path_from_route_matches(value) {
            return Some(path);
        }
    }

    None
}

fn path_from_query(query: Option<&str>) -> Option<String> {
    let query = query?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "__path" || key == "path" {
            return normalize_path(&percent_decode(value));
        }
    }
    None
}

fn path_from_header_value(value: &str) -> Option<String> {
    let path = if value.starts_with("http://") || value.starts_with("https://") {
        Uri::try_from(value)
            .ok()
            .map(|uri| uri.path().to_string())
            .filter(|p| !p.is_empty())
    } else {
        Some(value.split('?').next()?.to_string())
    }?;

    normalize_path(&path)
}

fn path_from_route_matches(value: &str) -> Option<String> {
    for pair in value.split('&') {
        let (key, raw) = pair.split_once('=')?;
        if key == "1" || key == "path" {
            return normalize_path(&percent_decode(raw));
        }
    }
    None
}

fn normalize_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    })
}

fn build_uri(path: &str, original_query: Option<&str>) -> Result<Uri, axum::http::Error> {
    let path_and_query = if let Some(query) = original_query {
        let filtered = query
            .split('&')
            .filter(|pair| {
                !pair.starts_with("__path=") && !pair.starts_with("path=")
            })
            .collect::<Vec<_>>()
            .join("&");
        if filtered.is_empty() {
            PathAndQuery::from_maybe_shared(path.to_string())?
        } else {
            PathAndQuery::from_maybe_shared(format!("{path}?{filtered}"))?
        }
    } else {
        PathAndQuery::from_maybe_shared(path.to_string())?
    };

    Uri::builder().path_and_query(path_and_query).build()
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_path_from_vercel_query_param() {
        let path = resolve_original_path(Some("__path=api/campaigns/test3/settings"), &HeaderMap::new())
            .expect("path");
        assert_eq!(path, "/api/campaigns/test3/settings");
    }

    #[test]
    fn restores_encoded_path_from_query_param() {
        let path = resolve_original_path(
            Some("__path=api%2Fcampaigns%2Ftest3%2Fsettings"),
            &HeaderMap::new(),
        )
        .expect("path");
        assert_eq!(path, "/api/campaigns/test3/settings");
    }

    #[test]
    fn ignores_main_entrypoint_without_restore_data() {
        assert!(resolve_original_path(None, &HeaderMap::new()).is_none());
    }
}
