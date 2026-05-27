use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdminLiveTopic {
    Registrations,
    Submissions,
}

impl AdminLiveTopic {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registrations => "registrations",
            Self::Submissions => "submissions",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "registrations" => Some(Self::Registrations),
            "submissions" => Some(Self::Submissions),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdminLiveChange {
    Added,
    Modified,
    Removed,
}

impl AdminLiveChange {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Removed => "removed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminLiveEvent {
    pub topic: String,
    pub change: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<Value>,
}

impl AdminLiveEvent {
    pub fn new(
        topic: AdminLiveTopic,
        change: AdminLiveChange,
        id: impl Into<String>,
        row: Option<Value>,
    ) -> Self {
        Self {
            topic: topic.as_str().to_string(),
            change: change.as_str().to_string(),
            id: id.into(),
            row,
        }
    }
}

pub fn parse_admin_live_topics(raw: Option<&str>) -> Vec<AdminLiveTopic> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return vec![AdminLiveTopic::Registrations, AdminLiveTopic::Submissions];
    };

    let topics: Vec<AdminLiveTopic> = raw
        .split(',')
        .filter_map(|part| AdminLiveTopic::parse(part))
        .collect();

    if topics.is_empty() {
        vec![AdminLiveTopic::Registrations, AdminLiveTopic::Submissions]
    } else {
        topics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_topics_defaults_to_both() {
        let topics = parse_admin_live_topics(None);
        assert_eq!(topics.len(), 2);
    }

    #[test]
    fn parse_topics_filters_unknown_values() {
        let topics = parse_admin_live_topics(Some("registrations,unknown"));
        assert_eq!(topics, vec![AdminLiveTopic::Registrations]);
    }
}
