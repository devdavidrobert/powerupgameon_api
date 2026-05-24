use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinPrize {
    pub id: String,
    pub name: String,
    pub order: i64,
    #[serde(rename = "isRealPrize")]
    pub is_real_prize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpinResult {
    #[serde(rename = "campaignSlug")]
    pub campaign_slug: String,
    pub prize: SpinPrize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent: Option<bool>,
}

impl SpinResult {
    pub fn to_json(&self) -> Value {
        let mut data = json!({
            "campaignSlug": self.campaign_slug,
            "prize": {
                "id": self.prize.id,
                "name": self.prize.name,
                "order": self.prize.order,
                "isRealPrize": self.prize.is_real_prize,
            },
        });
        if let Some(idempotent) = self.idempotent {
            data["idempotent"] = json!(idempotent);
        }
        data
    }
}
