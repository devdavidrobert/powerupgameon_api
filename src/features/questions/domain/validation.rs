use super::question_type::QuestionType;
use crate::error::ApiError;
use serde_json::{json, Map, Value};

const TRUE_FALSE_DEFAULT: [&str; 2] = ["True", "False"];

#[derive(Clone)]
struct ParsedOption {
    label: String,
    image_url: Option<String>,
}

fn parse_option(value: &Value) -> Result<ParsedOption, ApiError> {
    match value {
        Value::String(s) => {
            let label = s.trim().to_string();
            if label.is_empty() {
                return Err(ApiError::bad_request("option label cannot be empty."));
            }
            Ok(ParsedOption {
                label,
                image_url: None,
            })
        }
        Value::Object(obj) => {
            let label = obj
                .get("label")
                .or_else(|| obj.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if label.is_empty() {
                return Err(ApiError::bad_request("option label cannot be empty."));
            }
            let image_url = obj
                .get("imageUrl")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from);
            Ok(ParsedOption { label, image_url })
        }
        _ => Err(ApiError::bad_request("invalid option format.")),
    }
}

fn options_to_json(options: &[ParsedOption]) -> Vec<Value> {
    options
        .iter()
        .map(|o| {
            let mut m = Map::new();
            m.insert("label".into(), json!(o.label));
            if let Some(url) = &o.image_url {
                m.insert("imageUrl".into(), json!(url));
            }
            Value::Object(m)
        })
        .collect()
}

fn default_true_false_options() -> Vec<ParsedOption> {
    TRUE_FALSE_DEFAULT
        .iter()
        .map(|s| ParsedOption {
            label: s.to_string(),
            image_url: None,
        })
        .collect()
}

fn normalize_options(options: Option<Vec<Value>>) -> Result<Vec<ParsedOption>, ApiError> {
    let options = options.unwrap_or_default();
    options.iter().map(parse_option).collect()
}

fn resolve_true_false_options(options: Option<Vec<Value>>) -> Result<Vec<ParsedOption>, ApiError> {
    match options {
        Some(vals) if !vals.is_empty() => {
            let parsed = normalize_options(Some(vals))?;
            if parsed.len() != 2 {
                return Err(ApiError::bad_request(
                    "true/false questions need exactly 2 options.",
                ));
            }
            Ok(parsed)
        }
        _ => Ok(default_true_false_options()),
    }
}

pub fn build_question_document(
    text: &str,
    question_type: QuestionType,
    options: Option<Vec<Value>>,
    correct_index: Option<i64>,
    input_rules: Option<Value>,
    correct_answer: Option<String>,
    rating: Option<Value>,
    correct_rating: Option<i64>,
    order: i64,
    accept_any_answer: Option<bool>,
    allow_multiple_selections: Option<bool>,
    correct_indices: Option<Vec<i64>>,
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
            data.insert("options".into(), json!(options_to_json(&options)));
            apply_multiple_choice_config(
                &mut data,
                accept_any_answer,
                allow_multiple_selections,
                correct_index,
                correct_indices,
                options.len(),
            )?;
        }
        QuestionType::TrueFalse => {
            let options = resolve_true_false_options(options)?;
            data.insert("options".into(), json!(options_to_json(&options)));
            apply_choice_grading(&mut data, accept_any_answer, correct_index, options.len())?;
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
    options: Option<Vec<Value>>,
    correct_index: Option<i64>,
    input_rules: Option<Value>,
    correct_answer: Option<String>,
    rating: Option<Value>,
    correct_rating: Option<i64>,
    order: Option<i64>,
    accept_any_answer: Option<bool>,
    allow_multiple_selections: Option<bool>,
    correct_indices: Option<Vec<i64>>,
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
                merged.insert("options".into(), json!(options_to_json(&options)));
            }
            let options_len = merged
                .get("options")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let resolved_accept = accept_any_answer.unwrap_or_else(|| {
                merged
                    .get("acceptAnyAnswer")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });
            let resolved_multi = allow_multiple_selections.unwrap_or_else(|| {
                merged
                    .get("allowMultipleSelections")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });
            apply_multiple_choice_config_to_merged(
                &mut merged,
                Some(resolved_accept),
                Some(resolved_multi),
                correct_index,
                correct_indices,
                options_len,
            )?;
            strip_type_specific_fields(&mut merged, resolved_type);
        }
        QuestionType::TrueFalse => {
            if let Some(opts) = options {
                let parsed = resolve_true_false_options(Some(opts))?;
                merged.insert("options".into(), json!(options_to_json(&parsed)));
            }
            let resolved_accept = accept_any_answer.unwrap_or_else(|| {
                merged
                    .get("acceptAnyAnswer")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            });
            apply_choice_grading_to_merged(
                &mut merged,
                Some(resolved_accept),
                correct_index,
                2,
            )?;
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
            None,
            None,
            None,
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
            None,
            None,
            None,
        )
        .expect("true_false document");

        assert_eq!(doc.get("type").and_then(|v| v.as_str()), Some("true_false"));
        let options = doc.get("options").and_then(|v| v.as_array()).unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(doc.get("correctIndex").and_then(|v| v.as_i64()), Some(0));
    }

    #[test]
    fn true_false_accepts_custom_labels() {
        let doc = build_question_document(
            "Do you agree?",
            QuestionType::TrueFalse,
            Some(vec![json!({"label": "Yes"}), json!({"label": "No"})]),
            Some(1),
            None,
            None,
            None,
            None,
            1,
            None,
            None,
            None,
        )
        .expect("custom true_false");

        let options = doc.get("options").and_then(|v| v.as_array()).unwrap();
        assert_eq!(options[0].get("label").and_then(|v| v.as_str()), Some("Yes"));
        assert_eq!(options[1].get("label").and_then(|v| v.as_str()), Some("No"));
    }

    #[test]
    fn multiple_choice_accepts_option_images() {
        let doc = build_question_document(
            "Pick a drink",
            QuestionType::MultipleChoice,
            Some(vec![
                json!({"label": "Steam", "imageUrl": "https://cdn.example/steam.png"}),
                json!({"label": "Other"}),
            ]),
            Some(0),
            None,
            None,
            None,
            None,
            1,
            None,
            None,
            None,
        )
        .expect("mc with images");

        let options = doc.get("options").and_then(|v| v.as_array()).unwrap();
        assert_eq!(
            options[0].get("imageUrl").and_then(|v| v.as_str()),
            Some("https://cdn.example/steam.png")
        );
    }

    #[test]
    fn multiple_choice_builds_without_correct_index_in_questionnaire_mode() {
        let doc = build_question_document(
            "How did you hear about us?",
            QuestionType::MultipleChoice,
            Some(vec![json!("Social media"), json!("Friend")]),
            None,
            None,
            None,
            None,
            None,
            1,
            Some(true),
            None,
            None,
        )
        .expect("questionnaire mc");

        assert_eq!(
            doc.get("acceptAnyAnswer").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(doc.get("correctIndex").is_none());
    }

    #[test]
    fn multiple_choice_builds_with_multi_select_correct_indices() {
        let doc = build_question_document(
            "Pick all that apply",
            QuestionType::MultipleChoice,
            Some(vec![json!("A"), json!("B"), json!("C")]),
            None,
            None,
            None,
            None,
            None,
            1,
            None,
            Some(true),
            Some(vec![0, 2]),
        )
        .expect("multi-select mc");

        assert_eq!(
            doc.get("allowMultipleSelections").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            doc.get("correctIndices").and_then(|v| v.as_array()).map(|a| a.len()),
            Some(2)
        );
        assert!(doc.get("correctIndex").is_none());
    }

    #[test]
    fn legacy_string_options_still_work() {
        let parsed = normalize_options(Some(vec![json!("Option A"), json!("Option B")])).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].label, "Option A");
    }
}

fn apply_multiple_choice_config(
    data: &mut Map<String, Value>,
    accept_any_answer: Option<bool>,
    allow_multiple_selections: Option<bool>,
    correct_index: Option<i64>,
    correct_indices: Option<Vec<i64>>,
    options_len: usize,
) -> Result<(), ApiError> {
    if allow_multiple_selections == Some(true) {
        data.insert("allowMultipleSelections".into(), json!(true));
        data.remove("correctIndex");
        if accept_any_answer == Some(true) {
            data.insert("acceptAnyAnswer".into(), json!(true));
            data.remove("correctIndices");
            return Ok(());
        }
        data.remove("acceptAnyAnswer");
        let indices = require_correct_indices(correct_indices, options_len)?;
        data.insert("correctIndices".into(), json!(indices));
        return Ok(());
    }

    data.remove("allowMultipleSelections");
    data.remove("correctIndices");
    apply_choice_grading(data, accept_any_answer, correct_index, options_len)
}

fn apply_multiple_choice_config_to_merged(
    merged: &mut Map<String, Value>,
    accept_any_answer: Option<bool>,
    allow_multiple_selections: Option<bool>,
    correct_index: Option<i64>,
    correct_indices: Option<Vec<i64>>,
    options_len: usize,
) -> Result<(), ApiError> {
    if allow_multiple_selections == Some(true) {
        merged.insert("allowMultipleSelections".into(), json!(true));
        merged.remove("correctIndex");
        if accept_any_answer == Some(true) {
            merged.insert("acceptAnyAnswer".into(), json!(true));
            merged.remove("correctIndices");
            return Ok(());
        }
        merged.remove("acceptAnyAnswer");
        if let Some(indices) = correct_indices {
            let validated = require_correct_indices(Some(indices), options_len)?;
            merged.insert("correctIndices".into(), json!(validated));
        } else if accept_any_answer == Some(false)
            && merged.get("correctIndices").is_none()
        {
            return Err(ApiError::bad_request(
                "correctIndices is required when multi-select scoring is enabled.",
            ));
        }
        return Ok(());
    }

    if allow_multiple_selections == Some(false) {
        merged.remove("allowMultipleSelections");
        merged.remove("correctIndices");
    }

    apply_choice_grading_to_merged(merged, accept_any_answer, correct_index, options_len)
}

fn require_correct_indices(
    correct_indices: Option<Vec<i64>>,
    options_len: usize,
) -> Result<Vec<i64>, ApiError> {
    let Some(indices) = correct_indices else {
        return Err(ApiError::bad_request(
            "correctIndices is required when multi-select scoring is enabled.",
        ));
    };
    if indices.is_empty() {
        return Err(ApiError::bad_request(
            "correctIndices must include at least one option index.",
        ));
    }

    let mut normalized: Vec<i64> = indices
        .into_iter()
        .filter(|idx| *idx >= 0 && (*idx as usize) < options_len)
        .collect();
    normalized.sort_unstable();
    normalized.dedup();

    if normalized.is_empty() {
        return Err(ApiError::bad_request(
            "correctIndices must be valid option indexes.",
        ));
    }

    Ok(normalized)
}

fn apply_choice_grading(
    data: &mut Map<String, Value>,
    accept_any_answer: Option<bool>,
    correct_index: Option<i64>,
    options_len: usize,
) -> Result<(), ApiError> {
    if accept_any_answer == Some(true) {
        data.insert("acceptAnyAnswer".into(), json!(true));
        data.remove("correctIndex");
        return Ok(());
    }

    data.remove("acceptAnyAnswer");
    let idx = require_correct_index(correct_index, options_len)?;
    data.insert("correctIndex".into(), json!(idx));
    Ok(())
}

fn apply_choice_grading_to_merged(
    merged: &mut Map<String, Value>,
    accept_any_answer: Option<bool>,
    correct_index: Option<i64>,
    options_len: usize,
) -> Result<(), ApiError> {
    if accept_any_answer == Some(true) {
        merged.insert("acceptAnyAnswer".into(), json!(true));
        merged.remove("correctIndex");
        return Ok(());
    }

    if accept_any_answer == Some(false) {
        merged.remove("acceptAnyAnswer");
    }

    if let Some(correct_index) = correct_index {
        let idx = require_correct_index(Some(correct_index), options_len)?;
        merged.insert("correctIndex".into(), json!(idx));
    } else if accept_any_answer == Some(false)
        && merged.get("correctIndex").is_none()
    {
        return Err(ApiError::bad_request(
            "correctIndex is required when scoring is enabled.",
        ));
    }

    Ok(())
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
