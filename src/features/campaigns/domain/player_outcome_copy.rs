use serde::{Deserialize, Serialize};

pub const MAX_OUTCOME_FIELD_LEN: usize = 500;
pub const MAX_OUTCOME_TITLE_LEN: usize = 120;
pub const MAX_EXIT_BUTTON_LABEL_LEN: usize = 80;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerOutcomeCopy {
    #[serde(rename = "winTitle", skip_serializing_if = "Option::is_none")]
    pub win_title: Option<String>,
    #[serde(rename = "winMessage", skip_serializing_if = "Option::is_none")]
    pub win_message: Option<String>,
    #[serde(rename = "consolationTitle", skip_serializing_if = "Option::is_none")]
    pub consolation_title: Option<String>,
    #[serde(rename = "consolationMessage", skip_serializing_if = "Option::is_none")]
    pub consolation_message: Option<String>,
    #[serde(rename = "belowThresholdTitle", skip_serializing_if = "Option::is_none")]
    pub below_threshold_title: Option<String>,
    #[serde(rename = "belowThresholdMessage", skip_serializing_if = "Option::is_none")]
    pub below_threshold_message: Option<String>,
    #[serde(rename = "exitButtonLabel", skip_serializing_if = "Option::is_none")]
    pub exit_button_label: Option<String>,
    #[serde(rename = "exitButtonUrl", skip_serializing_if = "Option::is_none")]
    pub exit_button_url: Option<String>,
}

pub fn trim_optional(value: Option<String>, max_len: usize) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(max_len).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_optional_drops_blank_values() {
        assert_eq!(trim_optional(Some("  ".into()), 10), None);
        assert_eq!(trim_optional(Some("Hello".into()), 10), Some("Hello".into()));
    }
}
