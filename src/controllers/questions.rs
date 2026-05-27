use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::features::campaigns::presentation::{
    CampaignContext, PublicCampaignContext, SlugIdPath,
};
use crate::features::media::application::read_uploaded_image;
use crate::features::media::domain::extension_for_content_type;
use crate::features::media::infrastructure::upload_public_image;
use crate::features::questions::domain::question_type::QuestionType;
use crate::features::questions::domain::validation::{
    build_question_document, merge_question_updates,
};
use crate::models::question::QuestionModel;
use axum::{
    extract::{Multipart, Path, State},
    http::{header, HeaderMap},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

const ADMIN_ONLY_FIELDS: &[&str] = &["correctIndex", "correctAnswer", "correctRating", "correctIndices"];

fn to_public(mut doc: Map<String, Value>) -> Map<String, Value> {
    for key in ADMIN_ONLY_FIELDS {
        doc.remove(*key);
    }
    doc
}

fn public_cache_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=30, stale-while-revalidate=120"
            .parse()
            .unwrap(),
    );
    headers
}

pub async fn get_all_questions(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
) -> ApiResult<(HeaderMap, Json<SuccessResponse<Vec<Map<String, Value>>>>)> {
    let questions = QuestionModel::find_all(&state, &ctx.paths).await?;
    Ok((
        public_cache_headers(),
        SuccessResponse::data(questions.into_iter().map(to_public).collect()),
    ))
}

pub async fn get_all_questions_admin(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    Ok(SuccessResponse::data(
        QuestionModel::find_all(&state, &ctx.paths).await?,
    ))
}

pub async fn get_question(
    State(state): State<Arc<AppState>>,
    PublicCampaignContext(ctx): PublicCampaignContext,
    Path(path): Path<SlugIdPath>,
) -> ApiResult<(HeaderMap, Json<SuccessResponse<Map<String, Value>>>)> {
    let question = QuestionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Question not found."))?;
    Ok((
        public_cache_headers(),
        SuccessResponse::data(to_public(question)),
    ))
}

#[derive(Deserialize)]
pub struct QuestionBody {
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub question_type: Option<String>,
    pub options: Option<Vec<Value>>,
    #[serde(rename = "correctIndex")]
    pub correct_index: Option<i64>,
    #[serde(rename = "inputRules")]
    pub input_rules: Option<Value>,
    #[serde(rename = "correctAnswer")]
    pub correct_answer: Option<String>,
    pub rating: Option<Value>,
    #[serde(rename = "correctRating")]
    pub correct_rating: Option<i64>,
    #[serde(rename = "acceptAnyAnswer")]
    pub accept_any_answer: Option<bool>,
    #[serde(rename = "allowMultipleSelections")]
    pub allow_multiple_selections: Option<bool>,
    #[serde(rename = "correctIndices")]
    pub correct_indices: Option<Vec<i64>>,
    pub order: Option<i64>,
}

pub async fn create_question(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Json(body): Json<QuestionBody>,
) -> ApiResult<(
    axum::http::StatusCode,
    Json<SuccessResponse<Map<String, Value>>>,
)> {
    let text = body.text.as_deref().unwrap_or("").trim();
    if text.is_empty() {
        return Err(ApiError::bad_request("text is required."));
    }

    let question_type = QuestionType::parse(body.question_type.as_deref());
    let all = QuestionModel::find_all(&state, &ctx.paths).await?;
    let order = body.order.unwrap_or(all.len() as i64 + 1);

    let data = build_question_document(
        text,
        question_type,
        body.options,
        body.correct_index,
        body.input_rules,
        body.correct_answer,
        body.rating,
        body.correct_rating,
        order,
        body.accept_any_answer,
        body.allow_multiple_selections,
        body.correct_indices,
    )?;

    let question = QuestionModel::create(&state, &ctx.paths, data).await?;
    Ok((
        axum::http::StatusCode::CREATED,
        SuccessResponse::data(question),
    ))
}

pub async fn update_question(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
    Json(body): Json<QuestionBody>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let existing = QuestionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Question not found."))?;

    let merged = merge_question_updates(
        &existing,
        body.text,
        body.question_type
            .as_deref()
            .map(|t| QuestionType::parse(Some(t))),
        body.options,
        body.correct_index,
        body.input_rules,
        body.correct_answer,
        body.rating,
        body.correct_rating,
        body.order,
        body.accept_any_answer,
        body.allow_multiple_selections,
        body.correct_indices,
    )?;

    let updated = QuestionModel::update(&state, &ctx.paths, &path.id, merged).await?;
    Ok(SuccessResponse::data(updated))
}

pub async fn delete_question(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<SlugIdPath>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if QuestionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .is_none()
    {
        return Err(ApiError::bad_request("Question not found."));
    }
    QuestionModel::delete(&state, &ctx.paths, &path.id).await?;
    Ok(SuccessResponse::message("Question deleted."))
}

#[derive(serde::Deserialize)]
pub struct QuestionOptionImagePath {
    pub slug: String,
    pub id: String,
    pub option_index: String,
}

pub async fn upload_question_option_image(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Path(path): Path<QuestionOptionImagePath>,
    mut multipart: Multipart,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let existing = QuestionModel::find_by_id(&state, &ctx.paths, &path.id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Question not found."))?;

    let option_index: usize = path
        .option_index
        .parse()
        .map_err(|_| ApiError::bad_request("option index must be a non-negative integer."))?;

    let options_array = existing
        .get("options")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Question has no options."))?;

    if option_index >= options_array.len() {
        return Err(ApiError::bad_request("option index out of range."));
    }

    let uploaded = read_uploaded_image(&mut multipart).await?;
    let ext = extension_for_content_type(&uploaded.content_type)
        .ok_or_else(|| ApiError::bad_request("Unsupported image content type."))?;
    let object_path = format!(
        "campaigns/{}/questions/{}/options/{}-{}.{}",
        ctx.slug(),
        path.id,
        option_index,
        uuid::Uuid::new_v4(),
        ext
    );

    let url =
        upload_public_image(&state, object_path, &uploaded.content_type, &uploaded.bytes).await?;

    let mut options: Vec<Value> = options_array.clone();
    match &mut options[option_index] {
        Value::Object(obj) => {
            obj.insert("imageUrl".into(), json!(url));
        }
        Value::String(label) => {
            let label = label.clone();
            options[option_index] = json!({ "label": label, "imageUrl": url });
        }
        _ => {
            return Err(ApiError::bad_request("invalid option at index."));
        }
    }

    let merged = merge_question_updates(
        &existing,
        None,
        None,
        Some(options),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    let updated = QuestionModel::update(&state, &ctx.paths, &path.id, merged).await?;
    Ok(SuccessResponse::data(updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn question_body_deserializes_extended_fields() {
        let body: QuestionBody = serde_json::from_value(json!({
            "text": "Rate us",
            "type": "rating",
            "rating": { "min": 1, "max": 5 },
            "correctRating": 5,
            "order": 1
        }))
        .expect("deserialize");

        assert_eq!(body.text.as_deref(), Some("Rate us"));
        assert_eq!(body.question_type.as_deref(), Some("rating"));
        assert_eq!(body.correct_rating, Some(5));
    }

    #[test]
    fn to_public_strips_all_admin_fields() {
        let public = to_public(json!({
            "text": "Q",
            "correctIndex": 0,
            "correctAnswer": "secret",
            "correctRating": 5
        })
        .as_object()
        .unwrap()
        .clone());

        assert!(public.get("correctIndex").is_none());
        assert!(public.get("correctAnswer").is_none());
        assert!(public.get("correctRating").is_none());
    }
}
