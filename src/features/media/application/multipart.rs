use crate::error::{ApiError, ApiResult};
use crate::features::media::domain::infer_content_type_from_filename;
use axum::extract::Multipart;

pub struct UploadedImage {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

pub async fn read_uploaded_image(multipart: &mut Multipart) -> ApiResult<UploadedImage> {
    let mut file_name: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::bad_request(format!("Invalid upload payload: {err}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        file_name = field.file_name().map(str::to_string);
        content_type = field
            .content_type()
            .map(str::to_string)
            .or_else(|| Some("application/octet-stream".into()));
        bytes = Some(
            field
                .bytes()
                .await
                .map_err(|err| ApiError::bad_request(format!("Could not read upload: {err}")))?
                .to_vec(),
        );
        break;
    }

    let Some(bytes) = bytes else {
        return Err(ApiError::bad_request("Missing image file."));
    };

    let mut content_type = content_type.unwrap_or_else(|| "application/octet-stream".into());
    if content_type == "application/octet-stream" {
        if let Some(name) = file_name.as_deref() {
            content_type = infer_content_type_from_filename(name, &content_type);
        }
    }

    Ok(UploadedImage {
        content_type,
        bytes,
    })
}
