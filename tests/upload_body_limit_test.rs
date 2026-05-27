//! Upload routes must accept bodies larger than the default JSON limit (256 KB).

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart},
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use tower::ServiceExt;

fn multipart_request(data: &[u8]) -> Request<Body> {
    let boundary = "test-upload-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"prize.png\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn read_upload(mut multipart: Multipart) -> StatusCode {
    while let Ok(Some(field)) = multipart.next_field().await {
        let _ = field.bytes().await;
    }
    StatusCode::OK
}

#[tokio::test]
async fn upload_route_accepts_bodies_above_default_json_limit() {
    const JSON_BODY_LIMIT: usize = 256 * 1024;
    const UPLOAD_BODY_LIMIT: usize = 3 * 1024 * 1024;

    let app = Router::new()
        .route(
            "/upload",
            post(read_upload).layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
        )
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT));

    let payload = vec![0u8; 300 * 1024];
    let response = app.oneshot(multipart_request(&payload)).await.unwrap();

    assert_ne!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "upload routes must not inherit the 256 KB JSON body cap"
    );
    assert_eq!(response.status(), StatusCode::OK);
}
