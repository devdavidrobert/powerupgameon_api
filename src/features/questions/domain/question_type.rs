#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    MultipleChoice,
    TrueFalse,
    Rating,
    Input,
}

impl QuestionType {
    pub fn as_str(self) -> &'static str {
        match self {
            QuestionType::MultipleChoice => "multiple_choice",
            QuestionType::TrueFalse => "true_false",
            QuestionType::Rating => "rating",
            QuestionType::Input => "input",
        }
    }

    pub fn parse(value: Option<&str>) -> Self {
        match value.unwrap_or("multiple_choice") {
            "true_false" => QuestionType::TrueFalse,
            "rating" => QuestionType::Rating,
            "input" => QuestionType::Input,
            _ => QuestionType::MultipleChoice,
        }
    }

    pub fn from_question_doc(doc: &serde_json::Map<String, serde_json::Value>) -> Self {
        Self::parse(doc.get("type").and_then(|v| v.as_str()))
    }
}
