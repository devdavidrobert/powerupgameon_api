pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;

pub const ALLOWED_CONTENT_TYPES: &[&str] =
    &["image/png", "image/jpeg", "image/webp", "image/svg+xml"];

pub fn validate_image_upload(content_type: &str, size: usize) -> Result<(), &'static str> {
    if size == 0 {
        return Err("Image file is empty.");
    }
    if size > MAX_IMAGE_BYTES {
        return Err("Image must be 2 MB or smaller.");
    }
    if !ALLOWED_CONTENT_TYPES.contains(&content_type) {
        return Err("Image must be PNG, JPEG, WebP, or SVG.");
    }
    Ok(())
}

pub fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

pub fn firebase_download_url(bucket: &str, object_path: &str, download_token: &str) -> String {
    let encoded = urlencoding::encode(object_path);
    format!(
        "https://firebasestorage.googleapis.com/v0/b/{bucket}/o/{encoded}?alt=media&token={download_token}"
    )
}

pub fn build_gcs_multipart_body(
    boundary: &str,
    metadata_json: &str,
    content_type: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(metadata_json.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

pub fn infer_content_type_from_filename(file_name: &str, fallback: &str) -> String {
    let lowered = file_name.to_ascii_lowercase();
    if lowered.ends_with(".png") {
        return "image/png".into();
    }
    if lowered.ends_with(".jpg") || lowered.ends_with(".jpeg") {
        return "image/jpeg".into();
    }
    if lowered.ends_with(".webp") {
        return "image/webp".into();
    }
    if lowered.ends_with(".svg") {
        return "image/svg+xml".into();
    }
    fallback.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_oversized_files() {
        assert!(validate_image_upload("image/png", MAX_IMAGE_BYTES + 1).is_err());
    }

    #[test]
    fn firebase_download_url_encodes_path_segments_and_includes_token() {
        let url = firebase_download_url(
            "demo.appspot.com",
            "campaigns/summer/prizes/p1.png",
            "abc-123",
        );
        assert!(url.contains("campaigns%2Fsummer%2Fprizes%2Fp1.png"));
        assert!(url.contains("alt=media&token=abc-123"));
    }
}
