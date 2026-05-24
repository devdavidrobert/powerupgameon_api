use super::question_type::QuestionType;
use crate::error::ApiError;
use serde_json::{json, Map, Value};

const TRUE_FALSE_OPTIONS: [&str; 2] = ["True", "False"];

pub fn build_question_document(
    text: &str,
    question_type: QuestionType,
    options: Option<Vec<String>>,
    correct_index: Option<i64>,
    input_rules: Option<Value>,
    correct_answer: Option<String>,
    rating: Option<Value>,
    correct_rating: Option<i64>,
    order: i64,
) -> Result<Map<String, Value>, ApiError> {
    let mut data = Map::new();
    data.insert("text".into(), json!(text));
    data.insert("type".into(), json!(question_type.as_str()));
    data.insert("order".into(), json!(order));

    match question_type {
        QuestionType::MultipleChoice => {
            let options = normalize_options(options)?;
            if options.len() < 2 {
                return Err(ApiError::bad_request(
                    "multiple choice questions need at least 2 options.",
                ));
            }
            let correct_index = require_correct_index(correct_index, options.len())?;
            data.insert("options".into(), json!(options));
            data.insert("correctIndex".into(), json!(correct_index));
        }
        QuestionType::TrueFalse => {
            let options: Vec<String> = TRUE_FALSE_OPTIONS.iter().map(|s| s.to_string()).collect();
            let correct_index = require_correct_index(correct_index, options.len())?;
            data.insert("options".into(), json!(options));
            data.insert("correctIndex".into(), json!(correct_index));
        }
        QuestionType::Rating => {
            let rating_obj = rating.ok_or_else(|| {
                ApiError::bad_request("rating config is required for rating questions.")
            })?;
            let min = rating_obj
                .get("min")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            let max = rating_obj
                .get("max")
                .and_then(|v| v.as_i64())
                .unwrap_or(5);
            if min >= max {
                return Err(ApiError::bad_request("rating min must be less than max."));
            }
            data.insert("rating".into(), rating_obj);
            if let Some(correct) = correct_rating {
                if correct < min || correct > max {
                    return Err(ApiError::bad_request(
                        "correctRating must be within the rating range.",
                    ));
                }
                data.insert("correctRating".into(), json!(correct));
            }
        }
        QuestionType::Input => {
            if let Some(rules) = input_rules {
                data.insert("inputRules".into(), rules);
            } else {
                data.insert(
                    "inputRules".into(),
                    json!({ "valueMode": "text", "placeholder": "Your answer..." }),
                );
            }
            if let Some(answer) = correct_answer
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
            {
                data.insert("correctAnswer".into(), json!(answer));
            }
        }
    }

    Ok(data)
}

pub fn merge_question_updates(
    existing: &Map<String, Value>,
    text: Option<String>,
    question_type: Option<QuestionType>,
    options: Option<Vec<String>>,
    correct_index: Option<i64>,
    input_rules: Option<Value>,
    correct_answer: Option<String>,
    rating: Option<Value>,
    correct_rating: Option<i64>,
    order: Option<i64>,
) -> Result<Map<String, Value>, ApiError> {
    let mut merged = existing.clone();
    if let Some(text) = text {
        merged.insert("text".into(), json!(text.trim()));
    }
    if let Some(order) = order {
        merged.insert("order".into(), json!(order));
    }

    let resolved_type = question_type
        .unwrap_or_else(|| QuestionType::from_question_doc(&merged));

    if let Some(qt) = question_type {
        merged.insert("type".into(), json!(qt.as_str()));
    }

    match resolved_type {
        QuestionType::MultipleChoice => {
            if let Some(options) = options {
                let options = normalize_options(Some(options))?;
                if options.len() < 2 {
                    return Err(ApiError::bad_request(
                        "multiple choice questions need at least 2 options.",
                    ));
                }
                merged.insert("options".into(), json!(options));
            }
            if let Some(correct_index) = correct_index {
                let len = merged
                    .get("options")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let idx = require_correct_index(Some(correct_index), len)?;
                merged.insert("correctIndex".into(), json!(idx));
            }
            strip_type_specific_fields(&mut merged, resolved_type);
        }
        QuestionType::TrueFalse => {
            merged.insert(
                "options".into(),
                json!(TRUE_FALSE_OPTIONS.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            );
            if let Some(correct_index) = correct_index {
                let idx = require_correct_index(Some(correct_index), 2)?;
                merged.insert("correctIndex".into(), json!(idx));
            }
            strip_type_specific_fields(&mut merged, resolved_type);
        }
        QuestionType::Rating => {
            if let Some(rating) = rating {
                merged.insert("rating".into(), rating);
            }
            match correct_rating {
                Some(value) => {
                    merged.insert("correctRating".into(), json!(value));
                }
                None => {
                    merged.remove("correctRating");
                }
            }
            strip_type_specific_fields(&mut merged, resolved_type);
        }
        QuestionType::Input => {
            if let Some(rules) = input_rules {
                merged.insert("inputRules".into(), rules);
            }
            match correct_answer {
                Some(answer) if answer.trim().is_empty() => {
                    merged.remove("correctAnswer");
                }
                Some(answer) => {
                    merged.insert("correctAnswer".into(), json!(answer.trim()));
                }
                None => {
                    merged.remove("correctAnswer");
                }
            }
            strip_type_specific_fields(&mut merged, resolved_type);
        }
    }

    Ok(merged)
}

fn strip_type_specific_fields(doc: &mut Map<String, Value>, question_type: QuestionType) {
    match question_type {
        QuestionType::MultipleChoice | QuestionType::TrueFalse => {
            doc.remove("rating");
            doc.remove("correctRating");
            doc.remove("inputRules");
            doc.remove("correctAnswer");
        }
        QuestionType::Rating => {
            doc.remove("options");
            doc.remove("correctIndex");
            doc.remove("inputRules");
            doc.remove("correctAnswer");
        }
        QuestionType::Input => {
            doc.remove("options");
            doc.remove("correctIndex");
            doc.remove("rating");
            doc.remove("correctRating");
        }
    }
}

fn normalize_options(options: Option<Vec<String>>) -> Result<Vec<String>, ApiError> {
    let options = options.unwrap_or_default();
    let trimmed: Vec<String> = options
        .into_iter()
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect();
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_builds_without_correct_answer() {
        let doc = build_question_document(
            "Your feedback?",
            QuestionType::Input,
            None,
            None,
            Some(json!({ "valueMode": "text" })),
            None,
            None,
            None,
            2,
        )
        .expect("input document");

        assert_eq!(doc.get("type").and_then(|v| v.as_str()), Some("input"));
        assert!(doc.get("correctAnswer").is_none());
    }

    #[test]
    fn true_false_builds_without_client_options() {
        let doc = build_question_document(
            "Is Steam energy?",
            QuestionType::TrueFalse,
            None,
            Some(0),
            None,
            None,
            None,
            None,
            1,
        )
        .expect("true_false document");

        assert_eq!(doc.get("type").and_then(|v| v.as_str()), Some("true_false"));
        let options = doc.get("options").and_then(|v| v.as_array()).unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(doc.get("correctIndex").and_then(|v| v.as_i64()), Some(0));
    }
}

fn require_correct_index(correct_index: Option<i64>, options_len: usize) -> Result<i64, ApiError> {
    let correct_index = correct_index.unwrap_or(-1);
    if correct_index < 0 || correct_index as usize >= options_len {
        return Err(ApiError::bad_request(
            "correctIndex must be a valid index within the options array.",
        ));
    }
    Ok(correct_index)
}
