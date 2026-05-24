use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult};
use crate::features::media::domain::{
    build_gcs_multipart_body, extension_for_content_type, firebase_download_url,
    validate_image_upload,
};
use reqwest::Client;
use serde_json::json;
use std::sync::LazyLock;

static HTTP: LazyLock<Client> = LazyLock::new(Client::new);

pub async fn upload_public_image(
    state: &AppState,
    object_path: String,
    content_type: &str,
    bytes: &[u8],
) -> ApiResult<String> {
    validate_image_upload(content_type, bytes.len())
        .map_err(|message| ApiError::bad_request(message))?;

    let ext = extension_for_content_type(content_type)
        .ok_or_else(|| ApiError::bad_request("Unsupported image content type."))?;

    if !object_path.ends_with(&format!(".{ext}")) {
        return Err(ApiError::bad_request("Object path extension mismatch."));
    }

    let bucket = state.config.storage_bucket(&state.db.project_id);
    let download_token = uuid::Uuid::new_v4().to_string();
    let boundary = uuid::Uuid::new_v4().to_string();

    let metadata_json = json!({
        "name": object_path,
        "metadata": {
            "firebaseStorageDownloadTokens": download_token
        }
    })
    .to_string();

    let access_token = state
        .firebase_auth
        .access_token()
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;

    let upload_url = format!(
        "https://storage.googleapis.com/upload/storage/v1/b/{bucket}/o?uploadType=multipart"
    );
    let body = build_gcs_multipart_body(&boundary, &metadata_json, content_type, bytes);

    let response = HTTP
        .post(upload_url)
        .bearer_auth(access_token)
        .header(
            "Content-Type",
            format!("multipart/related; boundary={boundary}"),
        )
        .body(body)
        .send()
        .await
        .map_err(|err| ApiError::Internal(err.into()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        tracing::error!(status = %status, %body, bucket = %bucket, "GCS image upload failed");

        if status == reqwest::StatusCode::NOT_FOUND && body.contains("bucket does not exist") {
            return Err(ApiError::with_code(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "STORAGE_BUCKET_NOT_FOUND",
                format!(
                    "Storage bucket \"{bucket}\" was not found. Set FIREBASE_STORAGE_BUCKET in the API environment."
                ),
            ));
        }

        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(ApiError::with_code(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "STORAGE_UPLOAD_FORBIDDEN",
                "Service account lacks permission to upload to Firebase Storage.",
            ));
        }

        return Err(ApiError::Internal(anyhow::anyhow!("Failed to upload image.")));
    }

    Ok(firebase_download_url(&bucket, &object_path, &download_token))
}
