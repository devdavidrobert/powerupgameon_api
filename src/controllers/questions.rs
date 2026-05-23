use crate::app_state::AppState;
use crate::error::{ApiError, ApiResult, SuccessResponse};
use crate::models::question::QuestionModel;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;

fn to_public(mut doc: Map<String, Value>) -> Map<String, Value> {
    doc.remove("correctIndex");
    doc
}

pub async fn get_all_questions(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    let questions = QuestionModel::find_all(&state).await?;
    Ok(SuccessResponse::data(
        questions.into_iter().map(to_public).collect(),
    ))
}

pub async fn get_all_questions_admin(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<SuccessResponse<Vec<Map<String, Value>>>>> {
    Ok(SuccessResponse::data(QuestionModel::find_all(&state).await?))
}

pub async fn get_question(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    let question = QuestionModel::find_by_id(&state, &id)
        .await?
        .ok_or_else(|| ApiError::bad_request("Question not found."))?;
    Ok(SuccessResponse::data(to_public(question)))
}

#[derive(Deserialize)]
pub struct QuestionBody {
    pub text: Option<String>,
    pub options: Option<Vec<String>>,
    pub correct_index: Option<i64>,
    pub order: Option<i64>,
}

pub async fn create_question(
    State(state): State<Arc<AppState>>,
    Json(body): Json<QuestionBody>,
) -> ApiResult<(axum::http::StatusCode, Json<SuccessResponse<Map<String, Value>>>)> {
    let text = body.text.as_deref().unwrap_or("").trim();
    let options = body.options.clone().unwrap_or_default();
    if text.is_empty() || options.len() < 2 {
        return Err(ApiError::bad_request(
            "text and at least 2 options are required.",
        ));
    }
    let correct_index = body.correct_index.unwrap_or(-1);
    if correct_index < 0 || correct_index as usize >= options.len() {
        return Err(ApiError::bad_request(
            "correctIndex must be a valid index within options array.",
        ));
    }
    let all = QuestionModel::find_all(&state).await?;
    let order = body.order.unwrap_or(all.len() as i64 + 1);
    let mut data = Map::new();
    data.insert("text".into(), json!(text));
    data.insert(
        "options".into(),
        json!(options.into_iter().map(|o| o.trim().to_string()).collect::<Vec<_>>()),
    );
    data.insert("correctIndex".into(), json!(correct_index));
    data.insert("order".into(), json!(order));
    let question = QuestionModel::create(&state, data).await?;
    QuestionModel::invalidate_list_cache();
    Ok((axum::http::StatusCode::CREATED, SuccessResponse::data(question)))
}

pub async fn update_question(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<QuestionBody>,
) -> ApiResult<Json<SuccessResponse<Map<String, Value>>>> {
    if QuestionModel::find_by_id(&state, &id).await?.is_none() {
        return Err(ApiError::bad_request("Question not found."));
    }
    let mut updates = Map::new();
    if let Some(text) = body.text {
        updates.insert("text".into(), json!(text.trim()));
    }
    if let Some(options) = body.options {
        updates.insert(
            "options".into(),
            json!(options.into_iter().map(|o| o.trim().to_string()).collect::<Vec<_>>()),
        );
    }
    if let Some(correct_index) = body.correct_index {
        updates.insert("correctIndex".into(), json!(correct_index));
    }
    if let Some(order) = body.order {
        updates.insert("order".into(), json!(order));
    }
    let updated = QuestionModel::update(&state, &id, updates).await?;
    QuestionModel::invalidate_list_cache();
    Ok(SuccessResponse::data(updated))
}

pub async fn delete_question(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<SuccessResponse<Value>>> {
    if QuestionModel::find_by_id(&state, &id).await?.is_none() {
        return Err(ApiError::bad_request("Question not found."));
    }
    QuestionModel::delete(&state, &id).await?;
    QuestionModel::invalidate_list_cache();
    Ok(SuccessResponse::message("Question deleted."))
}
