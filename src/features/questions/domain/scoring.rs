use super::question_type::QuestionType;
use serde_json::{Map, Value};

/// Questions with a configured correct answer count toward the spin pass score.
pub fn question_is_gradable(question: &Map<String, Value>) -> bool {
    if question_accepts_any_answer(question) {
        return false;
    }
    match QuestionType::from_question_doc(question) {
        QuestionType::MultipleChoice | QuestionType::TrueFalse => true,
        QuestionType::Input => question
            .get("correctAnswer")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .is_some_and(|s| !s.is_empty()),
        QuestionType::Rating => question
            .get("correctRating")
            .and_then(|v| v.as_i64())
            .is_some(),
    }
}

pub fn question_accepts_any_answer(question: &Map<String, Value>) -> bool {
    question
        .get("acceptAnyAnswer")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuizScoreResult {
    pub score: i64,
    /// Gradable questions only (denominator for percentage).
    pub total: i64,
    pub question_count: i64,
    pub percentage: i64,
}

pub fn compute_quiz_score(
    questions: &[Map<String, Value>],
    answers: &[Value],
) -> Result<QuizScoreResult, String> {
    if questions.len() != answers.len() {
        return Err("answers length mismatch".into());
    }

    let question_count = questions.len() as i64;
    let mut score = 0i64;
    let mut gradable_total = 0i64;

    for (i, q) in questions.iter().enumerate() {
        let ans = &answers[i];
        validate_submission_answer(q, i, ans)?;
        if question_is_gradable(q) {
            gradable_total += 1;
            if answer_is_correct(q, ans) {
                score += 1;
            }
        }
    }

    let percentage = if gradable_total > 0 {
        ((score as f64 / gradable_total as f64) * 100.0).round() as i64
    } else {
        100
    };

    Ok(QuizScoreResult {
        score,
        total: gradable_total,
        question_count,
        percentage,
    })
}

pub fn qualifies_for_spin(percentage: i64, spin_pass_percent: i64) -> bool {
    percentage >= spin_pass_percent.clamp(0, 100)
}

pub fn answer_is_correct(question: &serde_json::Map<String, Value>, answer: &Value) -> bool {
    let question_type = QuestionType::from_question_doc(question);
    match question_type {
        QuestionType::MultipleChoice | QuestionType::TrueFalse => {
            if question_accepts_any_answer(question) {
                return answer_as_i64(answer).is_some();
            }
            let ans = answer_as_i64(answer);
            let correct = question
                .get("correctIndex")
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            ans == Some(correct)
        }
        QuestionType::Rating => {
            let ans = answer_as_i64(answer);
            let correct = question
                .get("correctRating")
                .and_then(|v| v.as_i64());
            match (ans, correct) {
                (Some(_), None) => true,
                (Some(a), Some(c)) => a == c,
                _ => false,
            }
        }
        QuestionType::Input => {
            let submitted = answer_as_string(answer);
            let expected = question
                .get("correctAnswer")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match (submitted, expected) {
                // Open-ended: any non-empty answer that passed validation counts as correct.
                (Some(_), None) => true,
                (Some(a), Some(e)) => normalize_text(&a) == normalize_text(e),
                _ => false,
            }
        }
    }
}

pub fn validate_submission_answer(
    question: &serde_json::Map<String, Value>,
    index: usize,
    answer: &Value,
) -> Result<(), String> {
    let question_type = QuestionType::from_question_doc(question);
    match question_type {
        QuestionType::MultipleChoice | QuestionType::TrueFalse => {
            let ans = answer_as_i64(answer).ok_or_else(|| {
                format!("Invalid answer for question {index}: expected option index.")
            })?;
            let options_len = question
                .get("options")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0) as i64;
            if ans < 0 || ans >= options_len {
                return Err(format!("Invalid answer index for question {index}."));
            }
            Ok(())
        }
        QuestionType::Rating => {
            let ans = answer_as_i64(answer).ok_or_else(|| {
                format!("Invalid answer for question {index}: expected rating number.")
            })?;
            let min = question
                .get("rating")
                .and_then(|r| r.get("min"))
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            let max = question
                .get("rating")
                .and_then(|r| r.get("max"))
                .and_then(|v| v.as_i64())
                .unwrap_or(5);
            if ans < min || ans > max {
                return Err(format!("Rating for question {index} must be between {min} and {max}."));
            }
            Ok(())
        }
        QuestionType::Input => {
            let text = answer_as_string(answer).ok_or_else(|| {
                format!("Invalid answer for question {index}: expected text.")
            })?;
            if text.trim().is_empty() {
                return Err(format!("Answer for question {index} cannot be empty."));
            }
            validate_input_rules(question, &text)
                .map_err(|e| format!("Invalid answer for question {index}: {e}"))
        }
    }
}

fn validate_input_rules(
    question: &serde_json::Map<String, Value>,
    text: &str,
) -> Result<(), &'static str> {
    let rules = match question.get("inputRules").and_then(|v| v.as_object()) {
        Some(r) => r,
        None => return Ok(()),
    };
    let mode = rules
        .get("valueMode")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let trimmed = text.trim();

    if let Some(min_len) = rules.get("minLength").and_then(|v| v.as_i64()) {
        if (trimmed.chars().count() as i64) < min_len {
            return Err("answer is too short");
        }
    }
    if let Some(max_len) = rules.get("maxLength").and_then(|v| v.as_i64()) {
        if (trimmed.chars().count() as i64) > max_len {
            return Err("answer is too long");
        }
    }

    match mode {
        "number" => {
            if trimmed.parse::<f64>().is_err() {
                return Err("answer must be a number");
            }
            if let Some(min) = rules.get("min").and_then(|v| v.as_f64()) {
                if trimmed.parse::<f64>().unwrap_or(f64::NAN) < min {
                    return Err("number is below minimum");
                }
            }
            if let Some(max) = rules.get("max").and_then(|v| v.as_f64()) {
                if trimmed.parse::<f64>().unwrap_or(f64::NAN) > max {
                    return Err("number is above maximum");
                }
            }
        }
        "multiline" => {
            if !trimmed.contains('\n') && trimmed.lines().count() <= 1 && trimmed.len() < 2 {
                // allow single-line answers for multiline questions
            }
        }
        "mixed" => {
            let has_letter = trimmed.chars().any(|c| c.is_alphabetic());
            let has_digit = trimmed.chars().any(|c| c.is_ascii_digit());
            if !has_letter || !has_digit {
                return Err("answer must include both letters and numbers");
            }
        }
        _ => {}
    }

    Ok(())
}

fn normalize_text(s: &str) -> String {
    s.trim().to_lowercase()
}

fn answer_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn answer_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn mc_question(correct: i64) -> Map<String, Value> {
        json!({
            "type": "multiple_choice",
            "options": ["A", "B"],
            "correctIndex": correct
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn questionnaire_multiple_choice_is_not_gradable() {
        let q = json!({
            "type": "multiple_choice",
            "options": ["A", "B"],
            "acceptAnyAnswer": true
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(!question_is_gradable(&q));
        assert!(answer_is_correct(&q, &json!(0)));
        assert!(answer_is_correct(&q, &json!(1)));

        let maps = vec![q];
        let answers = vec![json!(1)];
        let result = compute_quiz_score(&maps, &answers).expect("score");
        assert_eq!(result.score, 0);
        assert_eq!(result.total, 0);
        assert_eq!(result.percentage, 100);
    }

    #[test]
    fn scores_multiple_choice_by_index() {
        let q = mc_question(1);
        assert!(answer_is_correct(&q, &json!(1)));
        assert!(!answer_is_correct(&q, &json!(0)));
    }

    #[test]
    fn scores_rating_by_value() {
        let q = json!({
            "type": "rating",
            "rating": { "min": 1, "max": 5 },
            "correctRating": 4
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(answer_is_correct(&q, &json!(4)));
        assert!(!answer_is_correct(&q, &json!(3)));
    }

    #[test]
    fn scores_input_case_insensitive_when_correct_answer_set() {
        let q = json!({
            "type": "input",
            "correctAnswer": "Steam Energy"
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(answer_is_correct(&q, &json!("steam energy")));
    }

    #[test]
    fn scores_open_input_as_correct_for_any_answer() {
        let q = json!({
            "type": "input",
            "inputRules": { "valueMode": "text" }
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(answer_is_correct(&q, &json!("anything goes")));
    }

    #[test]
    fn compute_score_ignores_open_ended_questions() {
        let questions = vec![
            json!({
                "type": "multiple_choice",
                "options": ["A", "B"],
                "correctIndex": 0
            }),
            json!({
                "type": "input",
                "inputRules": { "valueMode": "text" }
            }),
            json!({
                "type": "rating",
                "rating": { "min": 1, "max": 5 }
            }),
        ];
        let answers = vec![json!(0), json!("feedback"), json!(3)];
        let maps: Vec<Map<String, Value>> = questions
            .iter()
            .map(|q| q.as_object().unwrap().clone())
            .collect();
        let result = compute_quiz_score(&maps, &answers).expect("score");
        assert_eq!(result.score, 1);
        assert_eq!(result.total, 1);
        assert_eq!(result.question_count, 3);
        assert_eq!(result.percentage, 100);
    }

    #[test]
    fn qualifies_for_spin_uses_campaign_threshold() {
        assert!(qualifies_for_spin(80, 80));
        assert!(!qualifies_for_spin(79, 80));
        assert!(qualifies_for_spin(100, 0));
    }

    #[test]
    fn validates_mixed_input_rules() {
        let q = json!({
            "type": "input",
            "inputRules": { "valueMode": "mixed" },
            "correctAnswer": "abc1"
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(validate_submission_answer(&q, 0, &json!("x9")).is_ok());
        assert!(validate_submission_answer(&q, 0, &json!("letters")).is_err());
    }
}
